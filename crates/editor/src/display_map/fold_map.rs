pub(crate) use crate::display_map::inlay_map::InlayChunk;

pub(crate) use super::{
    Highlights,
    inlay_map::{InlayBufferRows, InlayChunks, InlayEdit, InlayOffset, InlayPoint, InlaySnapshot},
};
pub(crate) use gpui::{
    AnyElement, App, ElementId, HighlightStyle, Pixels, SharedString, Stateful, Window,
};
pub(crate) use language::{Edit, HighlightId, LanguageAwareStyling, Point};
pub(crate) use multi_buffer::{
    Anchor, AnchorRangeExt, MBTextSummary, MultiBufferOffset, MultiBufferRow, MultiBufferSnapshot,
    RowInfo, ToOffset,
};
pub(crate) use project::InlayId;
pub(crate) use std::{
    any::TypeId,
    cmp::{self, Ordering},
    fmt, iter,
    ops::{Add, AddAssign, Deref, DerefMut, Range, Sub, SubAssign},
    sync::Arc,
    usize,
};
pub(crate) use sum_tree::{Bias, Cursor, Dimensions, FilterCursor, SumTree, Summary, TreeMap};
pub(crate) use ui::IntoElement as _;
pub(crate) use util::post_inc;

mod chunks;
mod cursor_helpers;
mod fold_types;
mod map;
mod offset;
mod placeholder;
mod point;
mod rows;
mod snapshot;
#[cfg(test)]
mod tests;
mod transform;
mod writer;

pub use chunks::{Chunk, ChunkRenderer, ChunkRendererContext, ChunkRendererId, FoldChunks};
pub use cursor_helpers::FoldPointCursor;
pub(crate) use cursor_helpers::{
    consolidate_fold_edits, consolidate_inlay_edits, intersecting_folds, push_isomorphic,
};
pub(crate) use fold_types::FoldMetadata;
pub use fold_types::{Fold, FoldId, FoldRange, FoldSummary};
pub use map::FoldMap;
pub use offset::{FoldEdit, FoldOffset};
pub use placeholder::FoldPlaceholder;
pub use point::FoldPoint;
pub use rows::FoldRows;
pub use snapshot::FoldSnapshot;
pub(crate) use transform::{Transform, TransformPlaceholder, TransformSummary};
pub(crate) use writer::FoldMapWriter;
