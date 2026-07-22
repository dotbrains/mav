//! The inlay map. See the [`display_map`][super] docs for an overview of how the inlay map fits
//! into the rest of the [`DisplayMap`][super::DisplayMap]. Much of the documentation for this
//! module generalizes to other layers.
//!
//! The core of this module is the [`InlayMap`] struct, which maintains a vec of [`Inlay`]s, and
//! [`InlaySnapshot`], which holds a sum tree of [`Transform`]s.

use crate::{
    ChunkRenderer, HighlightStyles,
    inlays::{Inlay, InlayContent},
};
use collections::BTreeSet;
use language::{Chunk, Edit, LanguageAwareStyling, Point, TextSummary};
use multi_buffer::{
    MBTextSummary, MultiBufferOffset, MultiBufferRow, MultiBufferRows, MultiBufferSnapshot,
    RowInfo, ToOffset,
};
use project::InlayId;
use smallvec::SmallVec;
use std::{
    cmp, iter,
    ops::{Add, AddAssign, Range, Sub, SubAssign},
    sync::Arc,
};
use sum_tree::{Bias, Cursor, Dimensions, SumTree};
use text::{ChunkBitmaps, Patch};
use ui::{ActiveTheme, IntoElement as _, ParentElement as _, Styled as _, div};

use super::{Highlights, custom_highlights::CustomHighlightsChunks, fold_map::ChunkRendererId};

/// Decides where the [`Inlay`]s should be displayed.
///
/// See the [`display_map` module documentation](crate::display_map) for more information.
mod chunks_rows;
mod cursors;
mod helpers;
mod map;
mod snapshot;
mod types;

pub use cursors::{BufferOffsetToInlayPointCursor, InlayPointCursor};
pub use types::{InlayBufferRows, InlayEdit, InlayOffset, InlayPoint};
pub use types::{InlayChunk, InlayChunks, InlayMap, InlaySnapshot};

use helpers::push_isomorphic;

#[cfg(test)]
#[cfg(test)]
mod tests;
