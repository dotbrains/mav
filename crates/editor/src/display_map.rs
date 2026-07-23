//! This module defines where the text should be displayed in an [`Editor`][Editor].
//!
//! Not literally though - rendering, layout and all that jazz is a responsibility of [`EditorElement`][EditorElement].
//! Instead, [`DisplayMap`] decides where Inlays/Inlay hints are displayed, when
//! to apply a soft wrap, where to add fold indicators, whether there are any tabs in the buffer that
//! we display as spaces and where to display custom blocks (like diagnostics).
//! Seems like a lot? That's because it is. [`DisplayMap`] is conceptually made up
//! of several smaller structures that form a hierarchy (starting at the bottom):
//! - [`InlayMap`] that decides where the [`Inlay`]s should be displayed.
//! - [`FoldMap`] that decides where the fold indicators should be; it also tracks parts of a source file that are currently folded.
//! - [`TabMap`] that keeps track of hard tabs in a buffer.
//! - [`WrapMap`] that handles soft wrapping.
//! - [`BlockMap`] that tracks custom blocks such as diagnostics that should be displayed within buffer.
//! - [`DisplayMap`] that adds background highlights to the regions of text.
//!   Each one of those builds on top of preceding map.
//!
//! ## Structure of the display map layers
//!
//! Each layer in the map (and the multibuffer itself to some extent) has a few
//! structures that are used to implement the public API available to the layer
//! above:
//! - a `Transform` type - this represents a region of text that the layer in
//!   question is "managing", that it transforms into a more "processed" text
//!   for the layer above. For example, the inlay map has an `enum Transform`
//!   that has two variants:
//!     - `Isomorphic`, representing a region of text that has no inlay hints (i.e.
//!       is passed through the map transparently)
//!     - `Inlay`, representing a location where an inlay hint is to be inserted.
//! - a `TransformSummary` type, which is usually a struct with two fields:
//!   [`input: TextSummary`][`TextSummary`] and [`output: TextSummary`][`TextSummary`]. Here,
//!   `input` corresponds to "text in the layer below", and `output` corresponds to the text
//!   exposed to the layer above. So in the inlay map case, a `Transform::Isomorphic`'s summary is
//!   just `input = output = summary`, where `summary` is the [`TextSummary`] stored in that
//!   variant. Conversely, a `Transform::Inlay` always has an empty `input` summary, because it's
//!   not "replacing" any text that exists on disk. The `output` is the summary of the inlay text
//!   to be injected. - Various newtype wrappers for co-ordinate spaces (e.g. [`WrapRow`]
//!   represents a row index, after soft-wrapping (and all lower layers)).
//! - A `Snapshot` type (e.g. [`InlaySnapshot`]) that captures the state of a layer at a specific
//!   point in time.
//! - various APIs which drill through the layers below to work with the underlying text. Notably:
//!   - `fn text_summary_for_offset()` returns a [`TextSummary`] for the range in the co-ordinate
//!     space that the map in question is responsible for.
//!   - `fn <A>_point_to_<B>_point()` converts a point in co-ordinate space `A` into co-ordinate
//!     space `B`.
//!   - A [`RowInfo`] iterator (e.g. [`InlayBufferRows`]) and a [`Chunk`] iterator
//!     (e.g. [`InlayChunks`])
//!   - A `sync` function (e.g. [`InlayMap::sync`]) that takes a snapshot and list of [`Edit<T>`]s,
//!     and returns a new snapshot and a list of transformed [`Edit<S>`]s. Note that the generic
//!     parameter on `Edit` changes, since these methods take in edits in the co-ordinate space of
//!     the lower layer, and return edits in their own co-ordinate space. The term "edit" is
//!     slightly misleading, since an [`Edit<T>`] doesn't tell you what changed - rather it can be
//!     thought of as a "region to invalidate". In theory, it would be correct to always use a
//!     single edit that covers the entire range. However, this would lead to lots of unnecessary
//!     recalculation.
//!
//! See the docs for the [`inlay_map`] module for a more in-depth explanation of how a single layer
//! works.
//!
//! [Editor]: crate::Editor
//! [EditorElement]: crate::element::EditorElement
//! [`TextSummary`]: multi_buffer::MBTextSummary
//! [`WrapRow`]: wrap_map::WrapRow
//! [`InlayBufferRows`]: inlay_map::InlayBufferRows
//! [`InlayChunks`]: inlay_map::InlayChunks
//! [`Edit<T>`]: text::Edit
//! [`Edit<S>`]: text::Edit
//! [`Chunk`]: language::Chunk

#[macro_use]
mod dimensions;

mod block_map;
mod companion;
mod crease_map;
mod custom_highlights;
mod display_points;
mod fold_map;
mod highlights;
mod inlay_map;
mod invisibles;
mod map_blocks_highlights;
mod map_core;
mod map_folds;
#[path = "display_map/tests/root_core.rs"]
#[cfg(test)]
mod root_core_tests;
#[path = "display_map/tests/root_highlights.rs"]
#[cfg(test)]
mod root_highlight_tests;
mod snapshot_access;
mod snapshot_chunks;
mod snapshot_text;
mod tab_map;
mod wrap_map;

pub use crate::display_map::{fold_map::FoldMap, inlay_map::InlayMap, tab_map::TabMap};
pub use block_map::{
    Block, BlockChunks as DisplayChunks, BlockContext, BlockId, BlockMap, BlockPlacement,
    BlockPoint, BlockProperties, BlockRows, BlockStyle, CompanionView, CompanionViewMut,
    CustomBlockId, EditorMargins, RenderBlock, StickyHeaderExcerpt,
};
pub(crate) use companion::{Companion, CompanionExcerptPatch};
pub use crease_map::*;
pub use display_points::{DisplayPoint, DisplayPointConverter, DisplayRow};
pub use fold_map::{
    ChunkRenderer, ChunkRendererContext, ChunkRendererId, Fold, FoldId, FoldPlaceholder, FoldPoint,
};
pub use highlights::{
    ChunkReplacement, EditPredictionStyles, HighlightStyleId, HighlightStyleInterner,
    HighlightStyles, HighlightedChunk, Highlights, SemanticTokenHighlight,
};
pub use inlay_map::{InlayOffset, InlayPoint};
pub use invisibles::{is_invisible, replacement};
pub use wrap_map::{WrapPoint, WrapRow, WrapSnapshot};

use collections::{HashMap, HashSet};
use gpui::{
    App, Context, Entity, EntityId, Font, HighlightStyle, LineLayout, Pixels, UnderlineStyle,
    WeakEntity,
};
use language::{
    LanguageAwareStyling, Point, Subscription as BufferSubscription,
    language_settings::{AllLanguageSettings, LanguageSettings},
};

use multi_buffer::{
    Anchor, AnchorRangeExt, MultiBuffer, MultiBufferOffset, MultiBufferOffsetUtf16,
    MultiBufferPoint, MultiBufferRow, MultiBufferSnapshot, RowInfo, ToOffset, ToPoint,
};
use project::project_settings::DiagnosticSeverity;
use project::{InlayId, lsp_store::LspFoldingRange};
use serde::Deserialize;
use settings::Settings;
use smallvec::SmallVec;
use sum_tree::{Bias, TreeMap};
use text::{BufferId, LineIndent, Patch};
use ui::SharedString;
use unicode_segmentation::UnicodeSegmentation;
use ztracing::instrument;

use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::{
    any::TypeId,
    borrow::Cow,
    fmt::Debug,
    num::NonZeroU32,
    ops::{Add, Range, Sub},
    sync::Arc,
};

use crate::{
    EditorStyle, RowExt, hover_links::InlayHighlight, inlays::Inlay, movement::TextLayoutDetails,
};
use block_map::{BlockPointCursor, BlockRow, BlockSnapshot};
use fold_map::{FoldPointCursor, FoldSnapshot};
use highlights::diagnostic_style;
use inlay_map::{BufferOffsetToInlayPointCursor, InlaySnapshot};
use tab_map::{TabPointCursor, TabSnapshot};
use wrap_map::{WrapMap, WrapPatch, WrapPointCursor};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FoldStatus {
    Folded,
    Foldable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NavigationOverlayKey(TypeId);

impl NavigationOverlayKey {
    pub const fn unique<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

/// Keys for tagging text highlights.
///
/// Note the order is important as it determines the priority of the highlights, lower means higher priority
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HighlightKey {
    // Note we want semantic tokens > colorized brackets
    // to allow language server highlights to work over brackets.
    ColorizeBracket(usize),
    SemanticToken,
    // below is sorted lexicographically, as there is no relevant ordering for these aside from coming after the above
    BufferSearchHighlights,
    ConsoleAnsiHighlight(usize),
    DebugStackFrameLine,
    DocumentHighlightRead,
    DocumentHighlightWrite,
    EditPredictionHighlight,
    Editor,
    HighlightOnYank,
    HighlightsTreeView(usize),
    HoverState,
    HoveredLinkState,
    InlineAssist,
    InputComposition,
    MatchingBracket,
    NavigationOverlay(NavigationOverlayKey),
    PendingInput,
    PickerPreview,
    ProjectSearchView,
    Rename,
    SearchWithinRange,
    SelectedTextHighlight,
    SyntaxTreeView(usize),
    VimExchange,
}

pub trait ToDisplayPoint {
    fn to_display_point(&self, map: &DisplaySnapshot) -> DisplayPoint;
}

type TextHighlights = Arc<HashMap<HighlightKey, Arc<(HighlightStyle, Vec<Range<Anchor>>)>>>;
type SemanticTokensHighlights =
    Arc<HashMap<BufferId, (Arc<[SemanticTokenHighlight]>, Arc<HighlightStyleInterner>)>>;
type InlayHighlights = TreeMap<HighlightKey, TreeMap<InlayId, (HighlightStyle, InlayHighlight)>>;

/// Decides how text in a [`MultiBuffer`] should be displayed in a buffer, handling inlay hints,
/// folding, hard tabs, soft wrapping, custom blocks (like diagnostics), and highlighting.
///
/// See the [module level documentation](self) for more information.
pub struct DisplayMap {
    entity_id: EntityId,
    /// The buffer that we are displaying.
    buffer: Entity<MultiBuffer>,
    buffer_subscription: BufferSubscription<MultiBufferOffset>,
    /// Decides where the [`Inlay`]s should be displayed.
    inlay_map: InlayMap,
    /// Decides where the fold indicators should be and tracks parts of a source file that are currently folded.
    fold_map: FoldMap,
    /// Keeps track of hard tabs in a buffer.
    tab_map: TabMap,
    /// Handles soft wrapping.
    wrap_map: Entity<WrapMap>,
    /// Tracks custom blocks such as diagnostics that should be displayed within buffer.
    block_map: BlockMap,
    /// Regions of text that should be highlighted.
    text_highlights: TextHighlights,
    /// Regions of inlays that should be highlighted.
    inlay_highlights: InlayHighlights,
    /// The semantic tokens from the language server.
    pub semantic_token_highlights: SemanticTokensHighlights,
    /// A container for explicitly foldable ranges, which supersede indentation based fold range suggestions.
    crease_map: CreaseMap,
    pub(crate) fold_placeholder: FoldPlaceholder,
    pub clip_at_line_ends: bool,
    pub(crate) masked: bool,
    pub(crate) diagnostics_max_severity: DiagnosticSeverity,
    pub(crate) companion: Option<(WeakEntity<DisplayMap>, Entity<Companion>)>,
    lsp_folding_crease_ids: HashMap<BufferId, Vec<CreaseId>>,
}

#[derive(Clone)]
pub struct DisplaySnapshot {
    pub display_map_id: EntityId,
    pub companion_display_snapshot: Option<Arc<DisplaySnapshot>>,
    pub crease_snapshot: CreaseSnapshot,
    block_snapshot: BlockSnapshot,
    text_highlights: TextHighlights,
    inlay_highlights: InlayHighlights,
    semantic_token_highlights: SemanticTokensHighlights,
    clip_at_line_ends: bool,
    masked: bool,
    diagnostics_max_severity: DiagnosticSeverity,
    pub(crate) fold_placeholder: FoldPlaceholder,
    /// When true, LSP folding ranges are used via the crease map and the
    /// indent-based fallback in `crease_for_buffer_row` is skipped.
    pub(crate) use_lsp_folding_ranges: bool,
}
