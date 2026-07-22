use super::*;
use crate::{Language, LanguageRegistry};
use collections::HashMap;
use futures::FutureExt;
use std::{collections::BinaryHeap, ops::Range, sync::Arc, time::Duration};
use sum_tree::{Bias, Dimensions, SeekTarget, SumTree};
use text::{Anchor, BufferSnapshot, OffsetRangeExt, Point, ToOffset, ToPoint};

use super::parse_helpers::*;
use super::queries::join_ranges;
use super::tree::*;
use super::types::*;

#[derive(Copy, Clone, Debug)]
pub struct ParseTimeout;

impl std::error::Error for ParseTimeout {}

impl std::fmt::Display for ParseTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse timeout")
    }
}

impl SyntaxSnapshot {
    #[ztracing::instrument(skip_all)]
    pub fn reparse(
        &mut self,
        text: &BufferSnapshot,
        registry: Option<Arc<LanguageRegistry>>,
        root_language: Arc<Language>,
    ) {
        self.reparse_(text, registry, root_language, None).ok();
    }

    #[ztracing::instrument(skip_all)]
    pub fn reparse_with_timeout(
        &mut self,
        text: &BufferSnapshot,
        registry: Option<Arc<LanguageRegistry>>,
        root_language: Arc<Language>,
        budget: Duration,
    ) -> Result<(), ParseTimeout> {
        self.reparse_(text, registry, root_language, Some(budget))
    }

    #[ztracing::instrument(skip_all, fields(lang = root_language.config.name.0.as_str()))]
    fn reparse_(
        &mut self,
        text: &BufferSnapshot,
        registry: Option<Arc<LanguageRegistry>>,
        root_language: Arc<Language>,
        mut budget: Option<Duration>,
    ) -> Result<(), ParseTimeout> {
        let budget = &mut budget;
        let edit_ranges = text
            .edits_since::<usize>(&self.parsed_version)
            .map(|edit| edit.new)
            .collect::<Vec<_>>();
        self.reparse_with_ranges(
            text,
            root_language.clone(),
            edit_ranges,
            registry.as_ref(),
            budget,
        )?;

        if let Some(registry) = registry
            && registry.version() != self.language_registry_version
        {
            let mut resolved_injection_ranges = Vec::new();
            let mut cursor = self
                .layers
                .filter::<_, ()>(text, |summary| summary.contains_unknown_injections);
            cursor.next();
            while let Some(layer) = cursor.item() {
                let SyntaxLayerContent::Pending { language_name } = &layer.content else {
                    unreachable!()
                };
                if registry
                    .language_for_name_or_extension(language_name)
                    .now_or_never()
                    .and_then(|language| language.ok())
                    .is_some()
                {
                    let range = layer.range.to_offset(text);
                    log::trace!("reparse range {range:?} for language {language_name:?}");
                    resolved_injection_ranges.push(range);
                }

                cursor.next();
            }
            drop(cursor);

            if !resolved_injection_ranges.is_empty() {
                self.reparse_with_ranges(
                    text,
                    root_language,
                    resolved_injection_ranges,
                    Some(&registry),
                    budget,
                )?;
            }
            self.language_registry_version = registry.version();
        }

        self.update_count += 1;
        Ok(())
    }

    #[ztracing::instrument(skip_all)]
    fn reparse_with_ranges(
        &mut self,
        text: &BufferSnapshot,
        root_language: Arc<Language>,
        invalidated_ranges: Vec<Range<usize>>,
        registry: Option<&Arc<LanguageRegistry>>,
        budget: &mut Option<Duration>,
    ) -> Result<(), ParseTimeout> {
        log::trace!(
            "reparse. invalidated ranges:{:?}",
            LogOffsetRanges(&invalidated_ranges, text),
        );

        let max_depth = self.layers.summary().max_depth;
        let mut cursor = self.layers.cursor::<SyntaxLayerSummary>(text);
        cursor.next();
        let mut layers = SumTree::new(text);

        let mut changed_regions = ChangeRegionSet::default();
        let mut queue = BinaryHeap::new();
        let mut combined_injection_ranges = HashMap::default();
        queue.push(ParseStep {
            depth: 0,
            language: ParseStepLanguage::Loaded {
                language: root_language,
            },
            included_ranges: vec![tree_sitter::Range {
                start_byte: 0,
                end_byte: text.len(),
                start_point: Point::zero().to_ts_point(),
                end_point: text.max_point().to_ts_point(),
            }],
            range: Anchor::min_max_range_for_buffer(text.remote_id()),
            mode: ParseMode::Single,
        });

        loop {
            let step = queue.pop();
            let position = if let Some(step) = &step {
                log::trace!(
                    "parse step depth:{}, range:{:?}, language:{} ({:?})",
                    step.depth,
                    LogAnchorRange(&step.range, text),
                    step.language.name(),
                    step.language.id(),
                );
                SyntaxLayerPosition {
                    depth: step.depth,
                    range: step.range.clone(),
                    language: step.language.id(),
                }
            } else {
                SyntaxLayerPosition {
                    depth: max_depth + 1,
                    range: Anchor::min_max_range_for_buffer(text.remote_id()),
                    language: None,
                }
            };

            let mut done = cursor.item().is_none();
            while !done && position.cmp(&cursor.end(), text).is_gt() {
                done = true;

                let bounded_position = SyntaxLayerPositionBeforeChange {
                    position: position.clone(),
                    change: changed_regions.start_position(text.remote_id()),
                };
                if bounded_position.cmp(cursor.start(), text).is_gt() {
                    let slice = cursor.slice(&bounded_position, Bias::Left);
                    if !slice.is_empty() {
                        layers.append(slice, text);
                        if changed_regions.prune(cursor.end(), text) {
                            done = false;
                        }
                    }
                }

                while position.cmp(&cursor.end(), text).is_gt() {
                    let Some(layer) = cursor.item() else { break };

                    if changed_regions.intersects(layer, text) {
                        if let SyntaxLayerContent::Parsed { language, .. } = &layer.content {
                            log::trace!(
                                "discard layer. language:{}, range:{:?}. changed_regions:{:?}",
                                language.name(),
                                LogAnchorRange(&layer.range, text),
                                LogChangedRegions(&changed_regions, text),
                            );
                        }

                        changed_regions.insert(
                            ChangedRegion {
                                depth: layer.depth + 1,
                                range: layer.range.clone(),
                            },
                            text,
                        );
                    } else {
                        layers.push(layer.clone(), text);
                    }

                    cursor.next();
                    if changed_regions.prune(cursor.end(), text) {
                        done = false;
                    }
                }
            }

            let Some(step) = step else { break };
            let Dimensions(step_start_byte, step_start_point, _) =
                step.range.start.summary::<Dimensions<usize, Point>>(text);
            let step_end_byte = step.range.end.to_offset(text);

            let mut old_layer = cursor.item();
            if let Some(layer) = old_layer {
                if layer.range.to_offset(text) == (step_start_byte..step_end_byte)
                    && layer.content.language_id() == step.language.id()
                {
                    cursor.next();
                } else {
                    old_layer = None;
                }
            }

            let content = match step.language {
                ParseStepLanguage::Loaded { language } => {
                    let Some(grammar) = language.grammar() else {
                        continue;
                    };
                    let tree;
                    let changed_ranges;

                    let mut included_ranges = step.included_ranges;
                    let is_combined = matches!(step.mode, ParseMode::Combined { .. });

                    for range in &mut included_ranges {
                        range.start_byte -= step_start_byte;
                        range.end_byte -= step_start_byte;
                        range.start_point = (Point::from_ts_point(range.start_point)
                            - step_start_point)
                            .to_ts_point();
                        range.end_point = (Point::from_ts_point(range.end_point)
                            - step_start_point)
                            .to_ts_point();
                    }

                    if let Some((SyntaxLayerContent::Parsed { tree: old_tree, .. }, layer_range)) =
                        old_layer.map(|layer| (&layer.content, layer.range.clone()))
                    {
                        log::trace!(
                            "existing layer. language:{}, range:{:?}, included_ranges:{:?}",
                            language.name(),
                            LogAnchorRange(&layer_range, text),
                            LogIncludedRanges(&old_tree.included_ranges())
                        );

                        if let ParseMode::Combined {
                            mut parent_layer_changed_ranges,
                            ..
                        } = step.mode
                        {
                            for range in &mut parent_layer_changed_ranges {
                                range.start = range.start.saturating_sub(step_start_byte);
                                range.end = range.end.saturating_sub(step_start_byte);
                            }

                            let changed_indices;
                            (included_ranges, changed_indices) = splice_included_ranges(
                                old_tree.included_ranges(),
                                &parent_layer_changed_ranges,
                                &included_ranges,
                            );
                            insert_newlines_between_ranges(
                                changed_indices,
                                &mut included_ranges,
                                text,
                                step_start_byte,
                                step_start_point,
                            );
                        }

                        if included_ranges.is_empty() {
                            included_ranges.push(tree_sitter::Range {
                                start_byte: 0,
                                end_byte: 0,
                                start_point: Default::default(),
                                end_point: Default::default(),
                            });
                        }

                        log::trace!(
                            "update layer. language:{}, range:{:?}, included_ranges:{:?}",
                            language.name(),
                            LogAnchorRange(&step.range, text),
                            LogIncludedRanges(&included_ranges),
                        );

                        let result = parse_text(
                            grammar,
                            text.as_rope(),
                            step_start_byte,
                            &included_ranges,
                            Some(old_tree),
                            budget,
                        );
                        match result {
                            Ok(t) => tree = t,
                            Err(e) if e.downcast_ref::<ParseTimeout>().is_some() => {
                                return Err(ParseTimeout);
                            }
                            Err(e) => {
                                log::error!("error parsing text: {e:?}");
                                continue;
                            }
                        };

                        changed_ranges = join_ranges(
                            invalidated_ranges
                                .iter()
                                .filter(|&range| {
                                    range.start <= step_end_byte && range.end >= step_start_byte
                                })
                                .cloned(),
                            old_tree.changed_ranges(&tree).map(|r| {
                                step_start_byte + r.start_byte..step_start_byte + r.end_byte
                            }),
                        );
                    } else {
                        if matches!(step.mode, ParseMode::Combined { .. }) {
                            insert_newlines_between_ranges(
                                0..included_ranges.len(),
                                &mut included_ranges,
                                text,
                                step_start_byte,
                                step_start_point,
                            );
                        }

                        if included_ranges.is_empty() {
                            included_ranges.push(tree_sitter::Range {
                                start_byte: 0,
                                end_byte: 0,
                                start_point: Default::default(),
                                end_point: Default::default(),
                            });
                        }

                        log::trace!(
                            "create layer. language:{}, range:{:?}, included_ranges:{:?}",
                            language.name(),
                            LogAnchorRange(&step.range, text),
                            LogIncludedRanges(&included_ranges),
                        );

                        let result = parse_text(
                            grammar,
                            text.as_rope(),
                            step_start_byte,
                            &included_ranges,
                            None,
                            budget,
                        );
                        match result {
                            Ok(t) => tree = t,
                            Err(e) if e.downcast_ref::<ParseTimeout>().is_some() => {
                                return Err(ParseTimeout);
                            }
                            Err(e) => {
                                log::error!("error parsing text: {e:?}");
                                continue;
                            }
                        };
                        changed_ranges = vec![step_start_byte..step_end_byte];
                    }

                    if let (Some((config, registry)), false) = (
                        grammar.injection_config.as_ref().zip(registry.as_ref()),
                        changed_ranges.is_empty(),
                    ) {
                        // Handle invalidation and reactivation of injections on comment update
                        let mut expanded_ranges: Vec<_> = changed_ranges
                            .iter()
                            .map(|range| {
                                let start_row = range.start.to_point(text).row.saturating_sub(1);
                                let end_row = range.end.to_point(text).row.saturating_add(2);
                                text.point_to_offset(Point::new(start_row, 0))
                                    ..text.point_to_offset(Point::new(end_row, 0)).min(text.len())
                            })
                            .collect();
                        expanded_ranges.sort_unstable_by_key(|r| r.start);
                        expanded_ranges.dedup_by(|b, a| {
                            let overlaps = b.start <= a.end;
                            if overlaps {
                                a.end = a.end.max(b.end);
                            }
                            overlaps
                        });

                        for range in &expanded_ranges {
                            changed_regions.insert(
                                ChangedRegion {
                                    depth: step.depth + 1,
                                    range: text.anchor_before(range.start)
                                        ..text.anchor_after(range.end),
                                },
                                text,
                            );
                        }
                        get_injections(
                            config,
                            text,
                            step.range.clone(),
                            tree.root_node_with_offset(
                                step_start_byte,
                                step_start_point.to_ts_point(),
                            ),
                            registry,
                            step.depth + 1,
                            &expanded_ranges,
                            &mut combined_injection_ranges,
                            &mut queue,
                        );
                    }

                    let included_sub_ranges: Option<Vec<Range<Anchor>>> = if is_combined {
                        Some(
                            included_ranges
                                .into_iter()
                                .filter(|r| r.start_byte < r.end_byte)
                                .map(|r| {
                                    text.anchor_before(r.start_byte + step_start_byte)
                                        ..text.anchor_after(r.end_byte + step_start_byte)
                                })
                                .collect(),
                        )
                    } else {
                        None
                    };
                    SyntaxLayerContent::Parsed {
                        tree,
                        language,
                        included_sub_ranges,
                    }
                }
                ParseStepLanguage::Pending { name } => SyntaxLayerContent::Pending {
                    language_name: name,
                },
            };

            layers.push(
                SyntaxLayerEntry {
                    depth: step.depth,
                    range: step.range,
                    content,
                },
                text,
            );
        }

        drop(cursor);
        self.layers = layers;
        self.interpolated_version = text.version.clone();
        self.parsed_version = text.version.clone();
        #[cfg(debug_assertions)]
        self.check_invariants(text);
        Ok(())
    }
}
