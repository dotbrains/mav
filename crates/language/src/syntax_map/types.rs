use crate::syntax_map::MAX_BYTES_TO_QUERY;
use crate::{Grammar, Language, LanguageId};
use gpui::SharedString;
use std::{ops::Range, sync::Arc};
use streaming_iterator::StreamingIterator;
use text::{Anchor, Rope};
use tree_sitter::{Node, Query, QueryCapture, QueryCaptures, QueryCursor, QueryMatches};

#[derive(Default)]
pub struct SyntaxMapCaptures<'a> {
    pub(super) layers: Vec<SyntaxMapCapturesLayer<'a>>,
    pub(super) active_layer_count: usize,
    pub(super) grammars: Vec<&'a Grammar>,
}

#[derive(Default)]
pub struct SyntaxMapMatches<'a> {
    pub(super) layers: Vec<SyntaxMapMatchesLayer<'a>>,
    pub(super) active_layer_count: usize,
    pub(super) grammars: Vec<&'a Grammar>,
}

#[derive(Debug)]
pub struct SyntaxMapCapture<'a> {
    pub node: Node<'a>,
    pub index: u32,
    pub grammar_index: usize,
}

#[derive(Debug)]
pub struct SyntaxMapMatch<'a> {
    pub language: Arc<Language>,
    pub depth: usize,
    pub pattern_index: usize,
    pub captures: &'a [QueryCapture<'a>],
    pub grammar_index: usize,
}

pub(super) struct SyntaxMapCapturesLayer<'a> {
    pub(super) depth: usize,
    pub(super) captures: QueryCaptures<'a, 'a, TextProvider<'a>, &'a [u8]>,
    pub(super) next_capture: Option<QueryCapture<'a>>,
    pub(super) grammar_index: usize,
    pub(super) _query_cursor: QueryCursorHandle,
}

pub(super) struct SyntaxMapMatchesLayer<'a> {
    pub(super) language: Arc<Language>,
    pub(super) depth: usize,
    pub(super) next_pattern_index: usize,
    pub(super) next_captures: Vec<QueryCapture<'a>>,
    pub(super) has_next: bool,
    pub(super) matches: QueryMatches<'a, 'a, TextProvider<'a>, &'a [u8]>,
    pub(super) query: &'a Query,
    pub(super) grammar_index: usize,
    pub(super) _query_cursor: QueryCursorHandle,
}

#[derive(Clone)]
pub(super) struct SyntaxLayerEntry {
    pub(super) depth: usize,
    pub(super) range: Range<Anchor>,
    pub(super) content: SyntaxLayerContent,
}

#[derive(Clone)]
pub(super) enum SyntaxLayerContent {
    Parsed {
        tree: tree_sitter::Tree,
        language: Arc<Language>,
        included_sub_ranges: Option<Vec<Range<Anchor>>>,
    },
    Pending {
        language_name: Arc<str>,
    },
}

impl SyntaxLayerContent {
    pub(super) fn language_id(&self) -> Option<LanguageId> {
        match self {
            SyntaxLayerContent::Parsed { language, .. } => Some(language.id),
            SyntaxLayerContent::Pending { .. } => None,
        }
    }

    pub(super) fn tree(&self) -> Option<&tree_sitter::Tree> {
        match self {
            SyntaxLayerContent::Parsed { tree, .. } => Some(tree),
            SyntaxLayerContent::Pending { .. } => None,
        }
    }
}

/// A layer of syntax highlighting, corresponding to a single syntax
/// tree in a particular language.
#[derive(Debug)]
pub struct SyntaxLayer<'a> {
    /// The language for this layer.
    pub language: &'a Arc<Language>,
    pub included_sub_ranges: Option<&'a [Range<Anchor>]>,
    pub(crate) depth: usize,
    pub(super) tree: &'a tree_sitter::Tree,
    pub(crate) offset: (usize, tree_sitter::Point),
}

/// A layer of syntax highlighting. Like [SyntaxLayer], but holding
/// owned data instead of references.
#[derive(Clone)]
pub struct OwnedSyntaxLayer {
    /// The language for this layer.
    pub language: Arc<Language>,
    pub(super) tree: tree_sitter::Tree,
    pub offset: (usize, tree_sitter::Point),
}

#[derive(Debug, Clone)]
pub(super) struct SyntaxLayerSummary {
    pub(super) min_depth: usize,
    pub(super) max_depth: usize,
    pub(super) range: Range<Anchor>,
    pub(super) last_layer_range: Range<Anchor>,
    pub(super) last_layer_language: Option<LanguageId>,
    pub(super) contains_unknown_injections: bool,
}

#[derive(Clone, Debug)]
pub(super) struct SyntaxLayerPosition {
    pub(super) depth: usize,
    pub(super) range: Range<Anchor>,
    pub(super) language: Option<LanguageId>,
}

#[derive(Clone, Debug)]
pub(super) struct ChangeStartPosition {
    pub(super) depth: usize,
    pub(super) position: Anchor,
}

#[derive(Clone, Debug)]
pub(super) struct SyntaxLayerPositionBeforeChange {
    pub(super) position: SyntaxLayerPosition,
    pub(super) change: ChangeStartPosition,
}

pub(super) struct ParseStep {
    pub(super) depth: usize,
    pub(super) language: ParseStepLanguage,
    pub(super) range: Range<Anchor>,
    pub(super) included_ranges: Vec<tree_sitter::Range>,
    pub(super) mode: ParseMode,
}

#[derive(Debug)]
pub(super) enum ParseStepLanguage {
    Loaded { language: Arc<Language> },
    Pending { name: Arc<str> },
}

impl ParseStepLanguage {
    pub(super) fn name(&self) -> SharedString {
        match self {
            ParseStepLanguage::Loaded { language } => language.name().0,
            ParseStepLanguage::Pending { name } => name.clone().into(),
        }
    }

    pub(super) fn id(&self) -> Option<LanguageId> {
        match self {
            ParseStepLanguage::Loaded { language } => Some(language.id),
            ParseStepLanguage::Pending { .. } => None,
        }
    }
}

pub(super) enum ParseMode {
    Single,
    Combined {
        parent_layer_range: Range<usize>,
        parent_layer_changed_ranges: Vec<Range<usize>>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ChangedRegion {
    pub(super) depth: usize,
    pub(super) range: Range<Anchor>,
}

#[derive(Default)]
pub(super) struct ChangeRegionSet(pub(super) Vec<ChangedRegion>);

pub(super) struct TextProvider<'a>(pub(super) &'a Rope);

pub(super) struct ByteChunks<'a>(pub(super) text::Chunks<'a>);

pub struct QueryCursorHandle(pub(super) Option<QueryCursor>);
impl OwnedSyntaxLayer {
    /// Returns the root syntax node for this layer.
    pub fn node(&self) -> Node<'_> {
        self.tree
            .root_node_with_offset(self.offset.0, self.offset.1)
    }
}

impl<'a> SyntaxLayer<'a> {
    /// Returns an owned version of this layer.
    pub fn to_owned(&self) -> OwnedSyntaxLayer {
        OwnedSyntaxLayer {
            tree: self.tree.clone(),
            offset: self.offset,
            language: self.language.clone(),
        }
    }

    /// Returns the root node for this layer.
    pub fn node(&self) -> Node<'a> {
        self.tree
            .root_node_with_offset(self.offset.0, self.offset.1)
    }

    pub(crate) fn override_id(&self, offset: usize, text: &text::BufferSnapshot) -> Option<u32> {
        let text = TextProvider(text.as_rope());
        let config = self.language.grammar.as_ref()?.override_config.as_ref()?;

        let mut query_cursor = QueryCursorHandle::new();
        query_cursor.set_byte_range(offset.saturating_sub(1)..offset.saturating_add(1));
        query_cursor.set_containing_byte_range(
            offset.saturating_sub(MAX_BYTES_TO_QUERY / 2)
                ..offset.saturating_add(MAX_BYTES_TO_QUERY / 2),
        );

        let mut smallest_match: Option<(u32, Range<usize>)> = None;
        let mut matches = query_cursor.matches(&config.query, self.node(), text);
        while let Some(mat) = matches.next() {
            for capture in mat.captures {
                let Some(override_entry) = config.values.get(&capture.index) else {
                    continue;
                };

                let range = capture.node.byte_range();
                if override_entry.range_is_inclusive {
                    if offset < range.start || offset > range.end {
                        continue;
                    }
                } else if offset <= range.start || offset >= range.end {
                    continue;
                }

                if let Some((_, smallest_range)) = &smallest_match {
                    if range.len() < smallest_range.len() {
                        smallest_match = Some((capture.index, range))
                    }
                    continue;
                }

                smallest_match = Some((capture.index, range));
            }
        }

        smallest_match.map(|(index, _)| index)
    }
}
