use super::{
    Highlights,
    fold_map::Chunk,
    wrap_map::{self, WrapEdit, WrapPatch, WrapPoint, WrapSnapshot},
};
use crate::{
    EditorStyle, GutterDimensions,
    display_map::{Companion, dimensions::RowDelta, wrap_map::WrapRow},
};
use collections::{Bound, HashMap, HashSet};
use gpui::{AnyElement, App, EntityId, Pixels, Window};
use language::{LanguageAwareStyling, Patch, Point};
use multi_buffer::{
    Anchor, ExcerptBoundaryInfo, MultiBuffer, MultiBufferOffset, MultiBufferPoint, MultiBufferRow,
    MultiBufferSnapshot, RowInfo, ToOffset, ToPoint as _,
};
use parking_lot::Mutex;
use std::{
    cell::{Cell, RefCell},
    cmp::{self, Ordering},
    fmt::Debug,
    ops::{Deref, DerefMut, Not, Range, RangeBounds, RangeInclusive},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
};
use sum_tree::{Bias, ContextLessSummary, Dimensions, SumTree, TreeMap};
use text::{BufferId, Edit};
use ui::{ElementId, IntoElement};

#[path = "block_map/block.rs"]
mod block;
#[path = "block_map/model.rs"]
mod model;
#[path = "block_map/types.rs"]
mod types;
pub use model::*;
pub use types::*;
#[path = "block_map/custom_block.rs"]
mod custom_block;
#[path = "block_map/iterators.rs"]
mod iterators;
#[path = "block_map/map_blocks.rs"]
mod map_blocks;
#[path = "block_map/map_core.rs"]
mod map_core;
#[path = "block_map/map_sync.rs"]
mod map_sync;
#[path = "block_map/reader.rs"]
mod reader;
#[path = "block_map/snapshot.rs"]
mod snapshot;
#[path = "block_map/summaries.rs"]
mod summaries;
#[path = "block_map/writer.rs"]
mod writer;

const NEWLINES: &[u8; rope::Chunk::MASK_BITS] = &[b'\n'; _];
const BULLETS: &[u8; rope::Chunk::MASK_BITS] = &[b'*'; _];

/// Tracks custom blocks such as diagnostics that should be displayed within buffer.
///
/// See the [`display_map` module documentation](crate::display_map) for more information.
pub struct BlockMap {
    pub(super) wrap_snapshot: RefCell<WrapSnapshot>,
    next_block_id: AtomicUsize,
    custom_blocks: Vec<Arc<CustomBlock>>,
    custom_blocks_by_id: TreeMap<CustomBlockId, Arc<CustomBlock>>,
    transforms: RefCell<SumTree<Transform>>,
    buffer_header_height: u32,
    excerpt_header_height: u32,
    pub(super) folded_buffers: HashSet<BufferId>,
    buffers_with_disabled_headers: HashSet<BufferId>,
    pub(super) deferred_edits: Cell<Patch<WrapRow>>,
}

pub struct BlockMapReader<'a> {
    pub blocks: &'a Vec<Arc<CustomBlock>>,
    pub snapshot: BlockSnapshot,
}

pub struct BlockMapWriter<'a> {
    block_map: &'a mut BlockMap,
    companion: Option<BlockMapWriterCompanion<'a>>,
}

/// Auxiliary data needed when modifying a BlockMap whose parent DisplayMap has a companion.
struct BlockMapWriterCompanion<'a> {
    display_map_id: EntityId,
    companion_wrap_snapshot: WrapSnapshot,
    companion: &'a Companion,
    inverse: Option<BlockMapInverseWriter<'a>>,
}

struct BlockMapInverseWriter<'a> {
    companion_multibuffer: &'a MultiBuffer,
    companion_writer: Box<BlockMapWriter<'a>>,
}

#[derive(Clone)]
pub struct BlockSnapshot {
    pub(super) wrap_snapshot: WrapSnapshot,
    transforms: SumTree<Transform>,
    custom_blocks_by_id: TreeMap<CustomBlockId, Arc<CustomBlock>>,
    pub(super) buffer_header_height: u32,
    pub(super) excerpt_header_height: u32,
    pub(super) buffers_with_disabled_headers: HashSet<BufferId>,
}

impl Deref for BlockSnapshot {
    type Target = WrapSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.wrap_snapshot
    }
}

/// Forward-only cursor mapping [`WrapPoint`]s to [`BlockPoint`]s, reusing its
/// tree position across calls. This is the streaming equivalent of
/// [`BlockSnapshot::to_block_point`]; callers must provide non-decreasing wrap
/// points.
pub struct BlockPointCursor<'a> {
    snapshot: &'a BlockSnapshot,
    cursor: sum_tree::Cursor<'a, 'static, Transform, Dimensions<WrapRow, BlockRow>>,
}

impl BlockPointCursor<'_> {
    /// Resets the cursor to the start so it can seek backward again.
    pub fn reset(&mut self) {
        self.cursor.reset();
    }

    pub fn map(&mut self, wrap_point: WrapPoint) -> BlockPoint {
        let cursor = &mut self.cursor;
        if cursor.did_seek() {
            cursor.seek_forward(&wrap_point.row(), Bias::Right);
        } else {
            cursor.seek(&wrap_point.row(), Bias::Right);
        }
        if let Some(transform) = cursor.item() {
            if transform.block.is_some() {
                BlockPoint::new(cursor.start().1, 0)
            } else {
                let input_start = Point::new(cursor.start().0.0, 0);
                let output_start = Point::new(cursor.start().1.0, 0);
                let input_overshoot = wrap_point.0 - input_start;
                BlockPoint(output_start + input_overshoot)
            }
        } else {
            self.snapshot.max_point()
        }
    }
}

pub type RenderBlock = Arc<dyn Send + Sync + Fn(&mut BlockContext) -> AnyElement>;

/// Where to place a block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockPlacement<T> {
    /// Place the block above the given position.
    Above(T),
    /// Place the block below the given position.
    Below(T),
    /// Place the block next the given position.
    Near(T),
    /// Replace the given range of positions with the block.
    Replace(RangeInclusive<T>),
}

impl<T> BlockPlacement<T> {
    pub fn start(&self) -> &T {
        match self {
            BlockPlacement::Above(position) => position,
            BlockPlacement::Below(position) => position,
            BlockPlacement::Near(position) => position,
            BlockPlacement::Replace(range) => range.start(),
        }
    }

    fn end(&self) -> &T {
        match self {
            BlockPlacement::Above(position) => position,
            BlockPlacement::Below(position) => position,
            BlockPlacement::Near(position) => position,
            BlockPlacement::Replace(range) => range.end(),
        }
    }

    pub fn as_ref(&self) -> BlockPlacement<&T> {
        match self {
            BlockPlacement::Above(position) => BlockPlacement::Above(position),
            BlockPlacement::Below(position) => BlockPlacement::Below(position),
            BlockPlacement::Near(position) => BlockPlacement::Near(position),
            BlockPlacement::Replace(range) => BlockPlacement::Replace(range.start()..=range.end()),
        }
    }

    pub fn map<R>(self, mut f: impl FnMut(T) -> R) -> BlockPlacement<R> {
        match self {
            BlockPlacement::Above(position) => BlockPlacement::Above(f(position)),
            BlockPlacement::Below(position) => BlockPlacement::Below(f(position)),
            BlockPlacement::Near(position) => BlockPlacement::Near(f(position)),
            BlockPlacement::Replace(range) => {
                let (start, end) = range.into_inner();
                BlockPlacement::Replace(f(start)..=f(end))
            }
        }
    }

    fn tie_break(&self) -> u8 {
        match self {
            BlockPlacement::Replace(_) => 0,
            BlockPlacement::Above(_) => 1,
            BlockPlacement::Near(_) => 2,
            BlockPlacement::Below(_) => 3,
        }
    }
}

impl BlockPlacement<Anchor> {
    #[ztracing::instrument(skip_all)]
    fn cmp(&self, other: &Self, buffer: &MultiBufferSnapshot) -> Ordering {
        self.start()
            .cmp(other.start(), buffer)
            .then_with(|| other.end().cmp(self.end(), buffer))
            .then_with(|| self.tie_break().cmp(&other.tie_break()))
    }

    #[ztracing::instrument(skip_all)]
    fn to_wrap_row(&self, wrap_snapshot: &WrapSnapshot) -> Option<BlockPlacement<WrapRow>> {
        let buffer_snapshot = wrap_snapshot.buffer_snapshot();
        match self {
            BlockPlacement::Above(position) => {
                let mut position = position.to_point(buffer_snapshot);
                position.column = 0;
                let wrap_row = wrap_snapshot.make_wrap_point(position, Bias::Left).row();
                Some(BlockPlacement::Above(wrap_row))
            }
            BlockPlacement::Near(position) => {
                let mut position = position.to_point(buffer_snapshot);
                position.column = buffer_snapshot.line_len(MultiBufferRow(position.row));
                let wrap_row = wrap_snapshot.make_wrap_point(position, Bias::Left).row();
                Some(BlockPlacement::Near(wrap_row))
            }
            BlockPlacement::Below(position) => {
                let mut position = position.to_point(buffer_snapshot);
                position.column = buffer_snapshot.line_len(MultiBufferRow(position.row));
                let wrap_row = wrap_snapshot.make_wrap_point(position, Bias::Left).row();
                Some(BlockPlacement::Below(wrap_row))
            }
            BlockPlacement::Replace(range) => {
                let mut start = range.start().to_point(buffer_snapshot);
                let mut end = range.end().to_point(buffer_snapshot);
                if start == end {
                    None
                } else {
                    start.column = 0;
                    let start_wrap_row = wrap_snapshot.make_wrap_point(start, Bias::Left).row();
                    end.column = buffer_snapshot.line_len(MultiBufferRow(end.row));
                    let end_wrap_row = wrap_snapshot.make_wrap_point(end, Bias::Left).row();
                    Some(BlockPlacement::Replace(start_wrap_row..=end_wrap_row))
                }
            }
        }
    }
}

#[ztracing::instrument(skip(tree, wrap_snapshot))]
fn push_isomorphic(tree: &mut SumTree<Transform>, rows: RowDelta, wrap_snapshot: &WrapSnapshot) {
    if rows == RowDelta(0) {
        return;
    }

    let wrap_row_start = tree.summary().input_rows;
    let wrap_row_end = wrap_row_start + rows;
    let wrap_summary = wrap_snapshot.text_summary_for_range(wrap_row_start..wrap_row_end);
    let summary = TransformSummary {
        input_rows: WrapRow(rows.0),
        output_rows: BlockRow(rows.0),
        longest_row: BlockRow(wrap_summary.longest_row),
        longest_row_chars: wrap_summary.longest_row_chars,
        has_replacement_blocks: false,
    };
    let mut merged = false;
    tree.update_last(
        |last_transform| {
            if last_transform.block.is_none() {
                last_transform.summary.add_summary(&summary);
                merged = true;
            }
        },
        (),
    );
    if !merged {
        tree.push(
            Transform {
                summary,
                block: None,
            },
            (),
        );
    }
}

// Count the number of bytes prior to a target point. If the string doesn't contain the target
// point, return its total extent. Otherwise return the target point itself.
fn offset_for_row(s: &str, target: RowDelta) -> (RowDelta, usize) {
    let mut row = 0;
    let mut offset = 0;
    for (ix, line) in s.split('\n').enumerate() {
        if ix > 0 {
            row += 1;
            offset += 1;
        }
        if row >= target.0 {
            break;
        }
        offset += line.len();
    }
    (RowDelta(row), offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        display_map::{
            Companion, fold_map::FoldMap, fold_map::FoldPlaceholder, inlay_map::InlayMap,
            tab_map::TabMap, wrap_map::WrapMap,
        },
        test::test_font,
    };
    use buffer_diff::BufferDiff;
    use gpui::{App, AppContext as _, Element, div, font, px};
    use itertools::Itertools;
    use language::{Buffer, Capability, Point};
    use multi_buffer::{MultiBuffer, PathKey};
    use rand::prelude::*;
    use settings::SettingsStore;
    use std::env;
    use util::RandomCharIter;

    mod basic_tests;
    mod companion_tests;
    mod edge_case_tests;
    mod folded_buffer_tests;
    mod random_tests;
    mod replacement_tests;

    fn init_test(cx: &mut gpui::App) {
        let settings = SettingsStore::test(cx);
        cx.set_global(settings);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        assets::Assets.load_test_fonts(cx);
    }

    impl Block {
        fn as_custom(&self) -> Option<&CustomBlock> {
            match self {
                Block::Custom(block) => Some(block),
                _ => None,
            }
        }
    }

    impl BlockSnapshot {
        fn to_point(&self, point: BlockPoint, bias: Bias) -> Point {
            self.wrap_snapshot
                .to_point(self.to_wrap_point(point, bias), bias)
        }
    }
}
