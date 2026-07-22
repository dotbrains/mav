use super::*;
use crate::{Grammar, Language, LanguageRegistry};
use std::{iter, ops::Range, sync::Arc};
use sum_tree::{Bias, Dimensions, SeekTarget, SumTree};
use text::{Anchor, BufferSnapshot, Point, Rope, ToOffset, ToPoint};
use tree_sitter::Query;

use super::tree::*;
use super::types::*;

impl SyntaxMap {
    pub fn new(text: &BufferSnapshot) -> Self {
        Self {
            snapshot: SyntaxSnapshot::new(text),
            language_registry: None,
        }
    }

    pub fn set_language_registry(&mut self, registry: Arc<LanguageRegistry>) {
        self.language_registry = Some(registry);
    }

    pub fn snapshot(&self) -> SyntaxSnapshot {
        self.snapshot.clone()
    }

    pub fn language_registry(&self) -> Option<Arc<LanguageRegistry>> {
        self.language_registry.clone()
    }

    pub fn interpolate(&mut self, text: &BufferSnapshot) {
        self.snapshot.interpolate(text);
    }

    #[cfg(test)]
    pub fn reparse(&mut self, language: Arc<Language>, text: &BufferSnapshot) {
        self.snapshot
            .reparse(text, self.language_registry.clone(), language);
    }

    pub fn did_parse(&mut self, snapshot: SyntaxSnapshot) {
        self.snapshot = snapshot;
    }

    pub fn clear(&mut self, text: &BufferSnapshot) {
        let update_count = self.snapshot.update_count + 1;
        self.snapshot = SyntaxSnapshot::new(text);
        self.snapshot.update_count = update_count;
    }
}

impl SyntaxSnapshot {
    fn new(text: &BufferSnapshot) -> Self {
        Self {
            layers: SumTree::new(text),
            parsed_version: clock::Global::default(),
            interpolated_version: clock::Global::default(),
            language_registry_version: 0,
            update_count: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn root_language(&self) -> Option<Arc<Language>> {
        match &self.layers.first()?.content {
            SyntaxLayerContent::Parsed { language, .. } => Some(language.clone()),
            SyntaxLayerContent::Pending { .. } => None,
        }
    }

    pub fn update_count(&self) -> usize {
        self.update_count
    }

    #[ztracing::instrument(skip_all)]
    pub fn interpolate(&mut self, text: &BufferSnapshot) {
        let edits = text
            .anchored_edits_since::<Dimensions<usize, Point>>(&self.interpolated_version)
            .collect::<Vec<_>>();
        self.interpolated_version = text.version().clone();

        if edits.is_empty() {
            return;
        }

        let mut layers = SumTree::new(text);
        let mut first_edit_ix_for_depth = 0;
        let mut prev_depth = 0;
        let mut cursor = self.layers.cursor::<SyntaxLayerSummary>(text);
        cursor.next();

        'outer: loop {
            let depth = cursor.end().max_depth;
            if depth > prev_depth {
                first_edit_ix_for_depth = 0;
                prev_depth = depth;
            }

            // Preserve any layers at this depth that precede the first edit.
            if let Some((_, edit_range)) = edits.get(first_edit_ix_for_depth) {
                let target = ChangeStartPosition {
                    depth,
                    position: edit_range.start,
                };
                if target.cmp(cursor.start(), text).is_gt() {
                    let slice = cursor.slice(&target, Bias::Left);
                    layers.append(slice, text);
                }
            }
            // If this layer follows all of the edits, then preserve it and any
            // subsequent layers at this same depth.
            else if cursor.item().is_some() {
                let slice = cursor.slice(
                    &SyntaxLayerPosition {
                        depth: depth + 1,
                        range: Anchor::min_max_range_for_buffer(text.remote_id()),
                        language: None,
                    },
                    Bias::Left,
                );
                layers.append(slice, text);
                continue;
            };

            let Some(layer) = cursor.item() else { break };
            let Dimensions(start_byte, start_point, _) =
                layer.range.start.summary::<Dimensions<usize, Point>>(text);

            // Ignore edits that end before the start of this layer, and don't consider them
            // for any subsequent layers at this same depth.
            loop {
                let Some((_, edit_range)) = edits.get(first_edit_ix_for_depth) else {
                    continue 'outer;
                };
                if edit_range.end.cmp(&layer.range.start, text).is_le() {
                    first_edit_ix_for_depth += 1;
                } else {
                    break;
                }
            }

            let mut layer = layer.clone();
            if let SyntaxLayerContent::Parsed { tree, .. } = &mut layer.content {
                for (edit, edit_range) in &edits[first_edit_ix_for_depth..] {
                    // Ignore any edits that follow this layer.
                    if edit_range.start.cmp(&layer.range.end, text).is_ge() {
                        break;
                    }

                    // Apply any edits that intersect this layer to the layer's syntax tree.
                    let tree_edit = if edit_range.start.cmp(&layer.range.start, text).is_ge() {
                        tree_sitter::InputEdit {
                            start_byte: edit.new.start.0 - start_byte,
                            old_end_byte: edit.new.start.0 - start_byte
                                + (edit.old.end.0 - edit.old.start.0),
                            new_end_byte: edit.new.end.0 - start_byte,
                            start_position: (edit.new.start.1 - start_point).to_ts_point(),
                            old_end_position: (edit.new.start.1 - start_point
                                + (edit.old.end.1 - edit.old.start.1))
                                .to_ts_point(),
                            new_end_position: (edit.new.end.1 - start_point).to_ts_point(),
                        }
                    } else {
                        let node = tree.root_node();
                        tree_sitter::InputEdit {
                            start_byte: 0,
                            old_end_byte: node.end_byte(),
                            new_end_byte: 0,
                            start_position: Default::default(),
                            old_end_position: node.end_position(),
                            new_end_position: Default::default(),
                        }
                    };

                    tree.edit(&tree_edit);
                }

                debug_assert!(
                    tree.root_node().end_byte() <= text.len(),
                    "tree's size {}, is larger than text size {}",
                    tree.root_node().end_byte(),
                    text.len(),
                );
            }

            layers.push(layer, text);
            cursor.next();
        }

        layers.append(cursor.suffix(), text);
        drop(cursor);
        self.layers = layers;
    }

    pub fn single_tree_captures<'a>(
        range: Range<usize>,
        text: &'a Rope,
        tree: &'a tree_sitter::Tree,
        language: &'a Arc<Language>,
        query: fn(&Grammar) -> Option<&Query>,
    ) -> SyntaxMapCaptures<'a> {
        SyntaxMapCaptures::new(
            range,
            text,
            [SyntaxLayer {
                language,
                tree,
                included_sub_ranges: None,
                depth: 0,
                offset: (0, tree_sitter::Point::new(0, 0)),
            }]
            .into_iter(),
            query,
        )
    }

    pub fn captures<'a>(
        &'a self,
        range: Range<usize>,
        buffer: &'a BufferSnapshot,
        query: fn(&Grammar) -> Option<&Query>,
    ) -> SyntaxMapCaptures<'a> {
        SyntaxMapCaptures::new(
            range.clone(),
            buffer.as_rope(),
            self.layers_for_range(range, buffer, true),
            query,
        )
    }

    pub fn matches<'a>(
        &'a self,
        range: Range<usize>,
        buffer: &'a BufferSnapshot,
        query: fn(&Grammar) -> Option<&Query>,
    ) -> SyntaxMapMatches<'a> {
        SyntaxMapMatches::new(
            range.clone(),
            buffer.as_rope(),
            self.layers_for_range(range, buffer, true),
            query,
            TreeSitterOptions::default(),
        )
    }

    pub fn matches_with_options<'a>(
        &'a self,
        range: Range<usize>,
        buffer: &'a BufferSnapshot,
        options: TreeSitterOptions,
        query: fn(&Grammar) -> Option<&Query>,
    ) -> SyntaxMapMatches<'a> {
        SyntaxMapMatches::new(
            range.clone(),
            buffer.as_rope(),
            self.layers_for_range(range, buffer, true),
            query,
            options,
        )
    }

    pub fn languages<'a>(
        &'a self,
        buffer: &'a BufferSnapshot,
        include_hidden: bool,
    ) -> impl Iterator<Item = &'a Arc<Language>> {
        let mut cursor = self.layers.cursor::<()>(buffer);
        cursor.next();
        iter::from_fn(move || {
            while let Some(layer) = cursor.item() {
                let mut info = None;
                if let SyntaxLayerContent::Parsed { language, .. } = &layer.content {
                    if include_hidden || !language.config.hidden {
                        info = Some(language);
                    }
                }
                cursor.next();
                if info.is_some() {
                    return info;
                }
            }
            None
        })
    }

    #[cfg(test)]
    pub fn layers<'a>(&'a self, buffer: &'a BufferSnapshot) -> Vec<SyntaxLayer<'a>> {
        self.layers_for_range(0..buffer.len(), buffer, true)
            .collect()
    }

    pub fn layers_for_range<'a, T: ToOffset>(
        &'a self,
        range: Range<T>,
        buffer: &'a BufferSnapshot,
        include_hidden: bool,
    ) -> impl 'a + Iterator<Item = SyntaxLayer<'a>> {
        let start_offset = range.start.to_offset(buffer);
        let end_offset = range.end.to_offset(buffer);
        let start = buffer.anchor_before(start_offset);
        let end = buffer.anchor_after(end_offset);

        let mut cursor = self.layers.filter::<_, ()>(buffer, move |summary| {
            if summary.max_depth > summary.min_depth {
                true
            } else {
                let is_before_start = summary.range.end.cmp(&start, buffer).is_lt();
                let is_after_end = summary.range.start.cmp(&end, buffer).is_gt();
                !is_before_start && !is_after_end
            }
        });

        cursor.next();
        iter::from_fn(move || {
            while let Some(layer) = cursor.item() {
                let mut info = None;
                if let SyntaxLayerContent::Parsed {
                    tree,
                    language,
                    included_sub_ranges,
                } = &layer.content
                {
                    let layer_start_offset = layer.range.start.to_offset(buffer);
                    let layer_start_point = layer.range.start.to_point(buffer).to_ts_point();
                    if include_hidden || !language.config.hidden {
                        info = Some(SyntaxLayer {
                            tree,
                            language,
                            included_sub_ranges: included_sub_ranges.as_deref(),
                            depth: layer.depth,
                            offset: (layer_start_offset, layer_start_point),
                        });
                    }
                }
                cursor.next();
                if info.is_some() {
                    return info;
                }
            }
            None
        })
    }

    pub fn contains_unknown_injections(&self) -> bool {
        self.layers.summary().contains_unknown_injections
    }

    pub fn language_registry_version(&self) -> usize {
        self.language_registry_version
    }
}
