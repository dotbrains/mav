mod anchor;
mod byte_iterators;
mod chunk_iterators;
mod cursor;
mod diff_state;
mod diff_transform;
mod dimensions;
mod edit_state;
mod events;
mod excerpt_ranges;
mod excerpt_summary;
mod expansion;
mod lifecycle;
#[cfg(test)]
mod multi_buffer_tests;
mod path_key;
mod row_info;
mod row_iterators;
mod snapshot_diff_hunks;
mod snapshot_dimensions;
mod snapshot_ranges;
mod snapshot_text;
#[cfg(any(test, feature = "test-support"))]
mod test_support;
mod transaction;
mod transform_dimensions;

use self::transaction::History;

pub use anchor::{Anchor, AnchorRangeExt};
pub use byte_iterators::{MultiBufferBytes, ReversedMultiBufferBytes};
pub use chunk_iterators::{MultiBufferChunks, ReversedMultiBufferChunks};
pub use dimensions::{
    BufferOffset, BufferOffsetUtf16, MultiBufferDimension, MultiBufferOffset,
    MultiBufferOffsetUniformSampler, MultiBufferOffsetUtf16, MultiBufferPoint, MultiBufferRow,
    ToOffset, ToPoint,
};
pub use events::{Event, MultiBufferDiffHunk};
pub use excerpt_summary::{DiffTransformSummary, ExcerptRange, ExcerptSummary, MBTextSummary};
pub(crate) use excerpt_summary::{Excerpt, ExcerptChunks};
pub use expansion::{ExpandExcerptDirection, IndentGuide};
pub use row_info::{ExcerptBoundary, ExcerptBoundaryInfo, ExpandInfo, RowInfo};
pub use row_iterators::MultiBufferRows;

use anchor::{AnchorSeekTarget, ExcerptAnchor};
use anyhow::{Result, anyhow};
use buffer_diff::{
    BufferDiff, BufferDiffEvent, BufferDiffSnapshot, DiffChanged, DiffHunkSecondaryStatus,
    DiffHunkStatus, DiffHunkStatusKind,
};
use clock::ReplicaId;
use collections::{BTreeMap, Bound, HashMap, HashSet, IndexSet};
use cursor::*;
use diff_state::*;
use diff_transform::*;
use edit_state::*;
use excerpt_ranges::*;
use futures_lite::future::yield_now;
use gpui::{App, Context, Entity, EventEmitter};
use itertools::Itertools;
use language::{
    AutoindentMode, Buffer, BufferChunks, BufferEditSource, BufferRow, BufferSnapshot, Capability,
    CharClassifier, CharKind, CharScopeContext, Chunk, CursorShape, DiagnosticEntryRef, File,
    IndentGuideSettings, IndentSize, Language, LanguageAwareStyling, LanguageScope, OffsetRangeExt,
    OffsetUtf16, Outline, OutlineItem, Point, PointUtf16, Selection, TextDimension, TextObject,
    ToOffset as _, ToPoint as _, TransactionId, TreeSitterOptions, Unclipped,
    language_settings::{AllLanguageSettings, LanguageSettings},
};

#[cfg(any(test, feature = "test-support"))]
use gpui::AppContext as _;

use rope::DimensionPair;
use settings::Settings;
use smallvec::SmallVec;
use std::{
    any::type_name,
    borrow::Cow,
    cell::{Cell, OnceCell, Ref, RefCell},
    cmp::{self, Ordering},
    fmt,
    future::Future,
    io,
    iter::{self, FromIterator},
    mem,
    ops::{self, Add, AddAssign, ControlFlow, Range, RangeBounds, Sub, SubAssign},
    rc::Rc,
    str,
    sync::{Arc, OnceLock},
    time::Duration,
};
use sum_tree::{Bias, Cursor, Dimension, Dimensions, SumTree, TreeMap};
use text::{
    BufferId, Edit, LineIndent, TextSummary,
    subscription::{Subscription, Topic},
};
use theme::SyntaxTheme;
use transform_dimensions::*;
use unicode_segmentation::UnicodeSegmentation;
use ztracing::instrument;

pub use self::path_key::PathKey;

pub static EXCERPT_CONTEXT_LINES: OnceLock<fn(&App) -> u32> = OnceLock::new();

pub fn excerpt_context_lines(cx: &App) -> u32 {
    EXCERPT_CONTEXT_LINES.get().map(|f| f(cx)).unwrap_or(2)
}

/// One or more [`Buffers`](Buffer) being edited in a single view.
///
/// See <https://mav.dev/features#multi-buffers>
pub struct MultiBuffer {
    /// A snapshot of the [`Excerpt`]s in the MultiBuffer.
    /// Use [`MultiBuffer::snapshot`] to get a up-to-date snapshot.
    snapshot: RefCell<MultiBufferSnapshot>,
    /// Contains the state of the buffers being edited
    buffers: BTreeMap<BufferId, BufferState>,
    /// Mapping from buffer IDs to their diff states
    diffs: HashMap<BufferId, DiffState>,
    subscriptions: Topic<MultiBufferOffset>,
    /// If true, the multi-buffer only contains a single [`Buffer`] and a single [`Excerpt`]
    singleton: bool,
    /// The history of the multi-buffer.
    history: History,
    /// The explicit title of the multi-buffer.
    /// If `None`, it will be derived from the underlying path or content.
    title: Option<String>,
    /// The writing capability of the multi-buffer.
    capability: Capability,
    buffer_changed_since_sync: Rc<Cell<bool>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct PathKeyIndex(u64);

struct BufferState {
    buffer: Entity<Buffer>,
    _subscriptions: [gpui::Subscription; 2],
}

#[derive(Clone)]
struct BufferStateSnapshot {
    pub(crate) path_key: PathKey,
    path_key_index: PathKeyIndex,
    buffer_snapshot: BufferSnapshot,
}

impl fmt::Debug for BufferStateSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufferStateSnapshot")
            .field("path_key", &self.path_key)
            .field("buffer_id", &self.buffer_snapshot.remote_id())
            .finish()
    }
}

/// The contents of a [`MultiBuffer`] at a single point in time.
#[derive(Clone, Default)]
pub struct MultiBufferSnapshot {
    excerpts: SumTree<Excerpt>,
    buffers: TreeMap<BufferId, BufferStateSnapshot>,
    path_keys: Arc<IndexSet<PathKey>>,
    diffs: SumTree<DiffStateSnapshot>,
    diff_transforms: SumTree<DiffTransform>,
    non_text_state_update_count: usize,
    edit_count: usize,
    is_dirty: bool,
    has_deleted_file: bool,
    has_conflict: bool,
    has_inverted_diff: bool,
    singleton: bool,
    trailing_excerpt_update_count: usize,
    all_diff_hunks_expanded: bool,
    show_deleted_hunks: bool,
    use_extended_diff_range: bool,
    show_headers: bool,
}

impl<K, V> MultiBufferDimension for DimensionPair<K, V>
where
    K: MultiBufferDimension,
    V: MultiBufferDimension,
{
    type TextDimension = DimensionPair<K::TextDimension, V::TextDimension>;

    fn from_summary(summary: &MBTextSummary) -> Self {
        Self {
            key: K::from_summary(summary),
            value: Some(V::from_summary(summary)),
        }
    }

    fn add_text_dim(&mut self, summary: &Self::TextDimension) {
        self.key.add_text_dim(&summary.key);
        if let Some(value) = &mut self.value {
            if let Some(other_value) = summary.value.as_ref() {
                value.add_text_dim(other_value);
            }
        }
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        self.key.add_mb_text_summary(summary);
        if let Some(value) = &mut self.value {
            value.add_mb_text_summary(summary);
        }
    }
}

impl MultiBuffer {
    pub fn edit<I, S, T>(
        &mut self,
        edits: I,
        autoindent_mode: Option<AutoindentMode>,
        cx: &mut Context<Self>,
    ) where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.edit_internal(edits, autoindent_mode, true, cx);
    }

    pub fn edit_non_coalesce<I, S, T>(
        &mut self,
        edits: I,
        autoindent_mode: Option<AutoindentMode>,
        cx: &mut Context<Self>,
    ) where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        self.edit_internal(edits, autoindent_mode, false, cx);
    }

    fn edit_internal<I, S, T>(
        &mut self,
        edits: I,
        autoindent_mode: Option<AutoindentMode>,
        coalesce_adjacent: bool,
        cx: &mut Context<Self>,
    ) where
        I: IntoIterator<Item = (Range<S>, T)>,
        S: ToOffset,
        T: Into<Arc<str>>,
    {
        if self.read_only() || self.buffers.is_empty() {
            return;
        }
        self.sync_mut(cx);
        let edits = edits
            .into_iter()
            .map(|(range, new_text)| {
                let mut range = range.start.to_offset(self.snapshot.get_mut())
                    ..range.end.to_offset(self.snapshot.get_mut());
                if range.start > range.end {
                    mem::swap(&mut range.start, &mut range.end);
                }
                (range, new_text.into())
            })
            .collect::<Vec<_>>();

        return edit_internal(self, edits, autoindent_mode, coalesce_adjacent, cx);

        // Non-generic part of edit, hoisted out to avoid blowing up LLVM IR.
        fn edit_internal(
            this: &mut MultiBuffer,
            edits: Vec<(Range<MultiBufferOffset>, Arc<str>)>,
            mut autoindent_mode: Option<AutoindentMode>,
            coalesce_adjacent: bool,
            cx: &mut Context<MultiBuffer>,
        ) {
            let original_indent_columns = match &mut autoindent_mode {
                Some(AutoindentMode::Block {
                    original_indent_columns,
                }) => mem::take(original_indent_columns),
                _ => Default::default(),
            };

            let buffer_edits = MultiBuffer::convert_edits_to_buffer_edits(
                edits,
                this.snapshot.get_mut(),
                &original_indent_columns,
            );

            let mut buffer_ids = Vec::with_capacity(buffer_edits.len());
            for (buffer_id, mut edits) in buffer_edits {
                buffer_ids.push(buffer_id);
                edits.sort_by_key(|edit| edit.range.start);
                this.buffers[&buffer_id].buffer.update(cx, |buffer, cx| {
                    let mut edits = edits.into_iter().peekable();
                    let mut insertions = Vec::new();
                    let mut original_indent_columns = Vec::new();
                    let mut deletions = Vec::new();
                    let empty_str: Arc<str> = Arc::default();
                    while let Some(BufferEdit {
                        mut range,
                        mut new_text,
                        mut is_insertion,
                        original_indent_column,
                    }) = edits.next()
                    {
                        while let Some(BufferEdit {
                            range: next_range,
                            is_insertion: next_is_insertion,
                            new_text: next_new_text,
                            ..
                        }) = edits.peek()
                        {
                            let should_coalesce = if coalesce_adjacent {
                                range.end >= next_range.start
                            } else {
                                range.end > next_range.start
                            };

                            if should_coalesce {
                                range.end = cmp::max(next_range.end, range.end);
                                is_insertion |= *next_is_insertion;
                                new_text = format!("{new_text}{next_new_text}").into();
                                edits.next();
                            } else {
                                break;
                            }
                        }

                        if is_insertion {
                            original_indent_columns.push(original_indent_column);
                            insertions.push((
                                buffer.anchor_before(range.start)..buffer.anchor_before(range.end),
                                new_text.clone(),
                            ));
                        } else if !range.is_empty() {
                            deletions.push((
                                buffer.anchor_before(range.start)..buffer.anchor_before(range.end),
                                empty_str.clone(),
                            ));
                        }
                    }

                    let deletion_autoindent_mode =
                        if let Some(AutoindentMode::Block { .. }) = autoindent_mode {
                            Some(AutoindentMode::Block {
                                original_indent_columns: Default::default(),
                            })
                        } else {
                            autoindent_mode.clone()
                        };
                    let insertion_autoindent_mode =
                        if let Some(AutoindentMode::Block { .. }) = autoindent_mode {
                            Some(AutoindentMode::Block {
                                original_indent_columns,
                            })
                        } else {
                            autoindent_mode.clone()
                        };

                    if coalesce_adjacent {
                        buffer.edit(deletions, deletion_autoindent_mode, cx);
                        buffer.edit(insertions, insertion_autoindent_mode, cx);
                    } else {
                        buffer.edit_non_coalesce(deletions, deletion_autoindent_mode, cx);
                        buffer.edit_non_coalesce(insertions, insertion_autoindent_mode, cx);
                    }
                })
            }

            cx.emit(Event::BuffersEdited { buffer_ids });
        }
    }

    fn convert_edits_to_buffer_edits(
        edits: Vec<(Range<MultiBufferOffset>, Arc<str>)>,
        snapshot: &MultiBufferSnapshot,
        original_indent_columns: &[Option<u32>],
    ) -> HashMap<BufferId, Vec<BufferEdit>> {
        let mut buffer_edits: HashMap<BufferId, Vec<BufferEdit>> = Default::default();
        let mut cursor = snapshot.cursor::<MultiBufferOffset, BufferOffset>();
        for (ix, (range, new_text)) in edits.into_iter().enumerate() {
            let original_indent_column = original_indent_columns.get(ix).copied().flatten();

            cursor.seek(&range.start);
            let mut start_region = cursor.region().expect("start offset out of bounds");
            if !start_region.is_main_buffer {
                cursor.next();
                if let Some(region) = cursor.region() {
                    start_region = region;
                } else {
                    continue;
                }
            }

            if range.end < start_region.range.start {
                continue;
            }

            let start_region = start_region.clone();
            if range.end > start_region.range.end {
                cursor.seek_forward(&range.end);
            }
            let mut end_region = cursor.region().expect("end offset out of bounds");
            if !end_region.is_main_buffer {
                cursor.prev();
                if let Some(region) = cursor.region() {
                    end_region = region;
                } else {
                    continue;
                }
            }

            if range.start > end_region.range.end {
                continue;
            }

            let start_overshoot = range.start.saturating_sub(start_region.range.start);
            let end_overshoot = range.end.saturating_sub(end_region.range.start);
            let buffer_start = (start_region.buffer_range.start + start_overshoot)
                .min(start_region.buffer_range.end);
            let buffer_end =
                (end_region.buffer_range.start + end_overshoot).min(end_region.buffer_range.end);

            if start_region.excerpt == end_region.excerpt {
                if start_region.buffer.capability == Capability::ReadWrite
                    && start_region.is_main_buffer
                {
                    buffer_edits
                        .entry(start_region.buffer.remote_id())
                        .or_default()
                        .push(BufferEdit {
                            range: buffer_start..buffer_end,
                            new_text,
                            is_insertion: true,
                            original_indent_column,
                        });
                }
            } else {
                let start_excerpt_range = buffer_start..start_region.buffer_range.end;
                let end_excerpt_range = end_region.buffer_range.start..buffer_end;
                if start_region.buffer.capability == Capability::ReadWrite
                    && start_region.is_main_buffer
                {
                    buffer_edits
                        .entry(start_region.buffer.remote_id())
                        .or_default()
                        .push(BufferEdit {
                            range: start_excerpt_range,
                            new_text: new_text.clone(),
                            is_insertion: true,
                            original_indent_column,
                        });
                }
                if end_region.buffer.capability == Capability::ReadWrite
                    && end_region.is_main_buffer
                {
                    buffer_edits
                        .entry(end_region.buffer.remote_id())
                        .or_default()
                        .push(BufferEdit {
                            range: end_excerpt_range,
                            new_text: new_text.clone(),
                            is_insertion: false,
                            original_indent_column,
                        });
                }
                let end_region_excerpt = end_region.excerpt.clone();

                cursor.seek(&range.start);
                cursor.next_excerpt();
                while let Some(region) = cursor.region() {
                    if region.excerpt == &end_region_excerpt {
                        break;
                    }
                    if region.buffer.capability == Capability::ReadWrite && region.is_main_buffer {
                        buffer_edits
                            .entry(region.buffer.remote_id())
                            .or_default()
                            .push(BufferEdit {
                                range: region.buffer_range.clone(),
                                new_text: new_text.clone(),
                                is_insertion: false,
                                original_indent_column,
                            });
                    }
                    cursor.next_excerpt();
                }
            }
        }
        buffer_edits
    }

    pub fn autoindent_ranges<I, S>(&mut self, ranges: I, cx: &mut Context<Self>)
    where
        I: IntoIterator<Item = Range<S>>,
        S: ToOffset,
    {
        if self.read_only() || self.buffers.is_empty() {
            return;
        }
        self.sync_mut(cx);
        let empty = Arc::<str>::from("");
        let edits = ranges
            .into_iter()
            .map(|range| {
                let mut range = range.start.to_offset(self.snapshot.get_mut())
                    ..range.end.to_offset(&self.snapshot.get_mut());
                if range.start > range.end {
                    mem::swap(&mut range.start, &mut range.end);
                }
                (range, empty.clone())
            })
            .collect::<Vec<_>>();

        return autoindent_ranges_internal(self, edits, cx);

        fn autoindent_ranges_internal(
            this: &mut MultiBuffer,
            edits: Vec<(Range<MultiBufferOffset>, Arc<str>)>,
            cx: &mut Context<MultiBuffer>,
        ) {
            let buffer_edits =
                MultiBuffer::convert_edits_to_buffer_edits(edits, this.snapshot.get_mut(), &[]);

            let mut buffer_ids = Vec::new();
            for (buffer_id, mut edits) in buffer_edits {
                buffer_ids.push(buffer_id);
                edits.sort_unstable_by_key(|edit| edit.range.start);

                let mut ranges: Vec<Range<BufferOffset>> = Vec::new();
                for edit in edits {
                    if let Some(last_range) = ranges.last_mut()
                        && edit.range.start <= last_range.end
                    {
                        last_range.end = last_range.end.max(edit.range.end);
                        continue;
                    }
                    ranges.push(edit.range);
                }

                this.buffers[&buffer_id].buffer.update(cx, |buffer, cx| {
                    buffer.autoindent_ranges(ranges, cx);
                })
            }

            cx.emit(Event::BuffersEdited { buffer_ids });
        }
    }

    pub fn set_active_selections(
        &self,
        selections: &[Selection<Anchor>],
        line_mode: bool,
        cursor_shape: CursorShape,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot(cx);
        let mut selections_by_buffer: HashMap<BufferId, Vec<Selection<text::Anchor>>> =
            Default::default();

        for selection in selections {
            for (buffer_snapshot, buffer_range, _) in
                snapshot.range_to_buffer_ranges(selection.start..selection.end)
            {
                selections_by_buffer
                    .entry(buffer_snapshot.remote_id())
                    .or_default()
                    .push(Selection {
                        id: selection.id,
                        start: buffer_snapshot
                            .anchor_at(buffer_range.start, selection.start.bias()),
                        end: buffer_snapshot.anchor_at(buffer_range.end, selection.end.bias()),
                        reversed: selection.reversed,
                        goal: selection.goal,
                    });
            }
        }

        for (buffer_id, buffer_state) in self.buffers.iter() {
            if !selections_by_buffer.contains_key(buffer_id) {
                buffer_state
                    .buffer
                    .update(cx, |buffer, cx| buffer.remove_active_selections(cx));
            }
        }

        for (buffer_id, selections) in selections_by_buffer {
            self.buffers[&buffer_id].buffer.update(cx, |buffer, cx| {
                buffer.set_active_selections(selections.into(), line_mode, cursor_shape, cx);
            });
        }
    }

    pub fn remove_active_selections(&self, cx: &mut Context<Self>) {
        for buffer in self.buffers.values() {
            buffer
                .buffer
                .update(cx, |buffer, cx| buffer.remove_active_selections(cx));
        }
    }

    #[instrument(skip_all)]
    fn merge_excerpt_ranges<'a>(
        expanded_ranges: impl IntoIterator<Item = &'a ExcerptRange<Point>> + 'a,
    ) -> Vec<ExcerptRange<Point>> {
        let mut sorted: Vec<_> = expanded_ranges.into_iter().collect();
        sorted.sort_by_key(|range| range.context.start);
        let mut merged_ranges: Vec<ExcerptRange<Point>> = Vec::new();
        for range in sorted {
            if let Some(last_range) = merged_ranges.last_mut() {
                if last_range.context.end >= range.context.start
                    || last_range.context.end.row + 1 == range.context.start.row
                {
                    last_range.context.end = range.context.end.max(last_range.context.end);
                    continue;
                }
            }
            merged_ranges.push(range.clone());
        }
        merged_ranges
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.sync_mut(cx);
        let removed_buffer_ids = std::mem::take(&mut self.buffers).into_keys().collect();
        self.diffs.clear();
        let MultiBufferSnapshot {
            excerpts,
            diffs,
            diff_transforms: _,
            non_text_state_update_count: _,
            edit_count: _,
            is_dirty,
            has_deleted_file,
            has_conflict,
            has_inverted_diff,
            singleton: _,
            trailing_excerpt_update_count,
            all_diff_hunks_expanded: _,
            show_deleted_hunks: _,
            use_extended_diff_range: _,
            show_headers: _,
            path_keys: _,
            buffers,
        } = self.snapshot.get_mut();
        let start = ExcerptDimension(MultiBufferOffset::ZERO);
        let prev_len = ExcerptDimension(excerpts.summary().text.len);
        *excerpts = Default::default();
        *buffers = Default::default();
        *diffs = Default::default();
        *trailing_excerpt_update_count += 1;
        *is_dirty = false;
        *has_deleted_file = false;
        *has_conflict = false;
        *has_inverted_diff = false;

        let edits = Self::sync_diff_transforms(
            self.snapshot.get_mut(),
            vec![Edit {
                old: start..prev_len,
                new: start..start,
            }],
            DiffChangeKind::BufferEdited,
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
        cx.emit(Event::BuffersRemoved { removed_buffer_ids });
        cx.notify();
    }

    // If point is at the end of the buffer, the last excerpt is returned
    pub fn point_to_buffer_offset<T: ToOffset>(
        &self,
        point: T,
        cx: &App,
    ) -> Option<(Entity<Buffer>, BufferOffset)> {
        let snapshot = self.read(cx);
        let (buffer, offset) = snapshot.point_to_buffer_offset(point)?;
        Some((
            self.buffers.get(&buffer.remote_id())?.buffer.clone(),
            offset,
        ))
    }

    // If point is at the end of the buffer, the last excerpt is returned
    pub fn point_to_buffer_point<T: ToPoint>(
        &self,
        point: T,
        cx: &App,
    ) -> Option<(Entity<Buffer>, Point)> {
        let snapshot = self.read(cx);
        let (buffer, point) = snapshot.point_to_buffer_point(point.to_point(&snapshot))?;
        Some((self.buffers.get(&buffer.remote_id())?.buffer.clone(), point))
    }

    pub fn buffer_point_to_anchor(
        &self,
        // todo(lw): We shouldn't need this?
        buffer: &Entity<Buffer>,
        point: Point,
        cx: &App,
    ) -> Option<Anchor> {
        let mut found = None;
        let buffer_snapshot = buffer.read(cx).snapshot();
        let text_anchor = buffer_snapshot.anchor_after(&point);
        let snapshot = self.snapshot(cx);
        let path_key_index = snapshot.path_key_index_for_buffer(buffer_snapshot.remote_id())?;
        for excerpt in snapshot.excerpts_for_buffer(buffer_snapshot.remote_id()) {
            if excerpt
                .context
                .start
                .cmp(&text_anchor, &buffer_snapshot)
                .is_gt()
            {
                found = Some(Anchor::in_buffer(path_key_index, excerpt.context.start));
                break;
            } else if excerpt
                .context
                .end
                .cmp(&text_anchor, &buffer_snapshot)
                .is_ge()
            {
                found = Some(Anchor::in_buffer(path_key_index, text_anchor));
                break;
            }
            found = Some(Anchor::in_buffer(path_key_index, excerpt.context.end));
        }

        found
    }

    pub fn wait_for_anchors<'a, Anchors: 'a + Iterator<Item = Anchor>>(
        &self,
        anchors: Anchors,
        cx: &mut Context<Self>,
    ) -> impl 'static + Future<Output = Result<()>> + use<Anchors> {
        let mut error = None;
        let mut futures = Vec::new();
        for anchor in anchors {
            if let Some(excerpt_anchor) = anchor.excerpt_anchor() {
                if let Some(buffer) = self.buffers.get(&excerpt_anchor.text_anchor.buffer_id) {
                    buffer.buffer.update(cx, |buffer, _| {
                        futures.push(buffer.wait_for_anchors([excerpt_anchor.text_anchor()]))
                    });
                } else {
                    error = Some(anyhow!(
                        "buffer {:?} is not part of this multi-buffer",
                        excerpt_anchor.text_anchor.buffer_id
                    ));
                    break;
                }
            }
        }
        async move {
            if let Some(error) = error {
                Err(error)?;
            }
            for future in futures {
                future.await?;
            }
            Ok(())
        }
    }

    pub fn text_anchor_for_position<T: ToOffset>(
        &self,
        position: T,
        cx: &App,
    ) -> Option<(Entity<Buffer>, text::Anchor)> {
        let snapshot = self.read(cx);
        let anchor = snapshot.anchor_before(position).excerpt_anchor()?;
        let buffer = self
            .buffers
            .get(&anchor.text_anchor.buffer_id)?
            .buffer
            .clone();
        Some((buffer, anchor.text_anchor()))
    }

    fn on_buffer_event(
        &mut self,
        buffer: Entity<Buffer>,
        event: &language::BufferEvent,
        cx: &mut Context<Self>,
    ) {
        use language::BufferEvent;
        let buffer_id = buffer.read(cx).remote_id();
        cx.emit(match event {
            &BufferEvent::Edited { source } => Event::Edited {
                edited_buffer: Some(buffer),
                source,
            },
            BufferEvent::DirtyChanged => Event::DirtyChanged,
            BufferEvent::Saved => Event::Saved,
            BufferEvent::FileHandleChanged => Event::FileHandleChanged,
            BufferEvent::Reloaded => Event::Reloaded,
            BufferEvent::LanguageChanged(has_language) => {
                Event::LanguageChanged(buffer_id, *has_language)
            }
            BufferEvent::Reparsed => Event::Reparsed(buffer_id),
            BufferEvent::DiagnosticsUpdated => Event::DiagnosticsUpdated,
            BufferEvent::CapabilityChanged => {
                self.capability = buffer.read(cx).capability();
                return;
            }
            BufferEvent::Operation { .. } | BufferEvent::ReloadNeeded => return,
        });
    }

    fn buffer_diff_changed(
        &mut self,
        diff: Entity<BufferDiff>,
        range: Option<Range<text::Anchor>>,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.buffer(diff.read(cx).buffer_id) else {
            return;
        };
        let snapshot = self.sync_mut(cx);

        let diff = diff.read(cx);
        let buffer_id = diff.buffer_id;

        let Some(path) = snapshot.path_for_buffer(buffer_id).cloned() else {
            return;
        };
        let new_diff = DiffStateSnapshot {
            buffer_id,
            diff: diff.snapshot(cx),
            main_buffer: None,
        };
        let snapshot = self.snapshot.get_mut();
        let base_text_changed = find_diff_state(&snapshot.diffs, buffer_id)
            .is_none_or(|old_diff| !new_diff.base_texts_definitely_eq(old_diff));
        snapshot.diffs.insert_or_replace(new_diff, ());

        let buffer = buffer.read(cx);
        let Some(range) = range else {
            return;
        };
        let diff_change_range = range.to_offset(buffer);

        let excerpt_edits = snapshot.excerpt_edits_for_diff_change(&path, diff_change_range);
        let edits = Self::sync_diff_transforms(
            snapshot,
            excerpt_edits,
            DiffChangeKind::DiffUpdated {
                base_changed: base_text_changed,
            },
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }

    fn inverted_buffer_diff_changed(
        &mut self,
        diff: Entity<BufferDiff>,
        main_buffer: Entity<language::Buffer>,
        diff_change_range: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.sync_mut(cx);

        let base_text_buffer_id = diff.read(cx).base_text_buffer().read(cx).remote_id();
        let Some(path) = snapshot.path_for_buffer(base_text_buffer_id).cloned() else {
            return;
        };

        let main_buffer_snapshot = main_buffer.read(cx).snapshot();
        let diff = diff.read(cx);
        let new_diff = DiffStateSnapshot {
            buffer_id: base_text_buffer_id,
            diff: diff.snapshot(cx),
            main_buffer: Some(main_buffer_snapshot),
        };
        let snapshot = self.snapshot.get_mut();
        snapshot.diffs.insert_or_replace(new_diff, ());

        let Some(diff_change_range) = diff_change_range else {
            return;
        };

        let excerpt_edits = snapshot.excerpt_edits_for_diff_change(&path, diff_change_range);
        let edits = Self::sync_diff_transforms(
            snapshot,
            excerpt_edits,
            DiffChangeKind::DiffUpdated {
                // We don't read this field for inverted diffs.
                base_changed: false,
            },
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }

    pub fn all_buffers_iter(&self) -> impl Iterator<Item = Entity<Buffer>> {
        self.buffers.values().map(|state| state.buffer.clone())
    }

    pub fn all_buffers(&self) -> HashSet<Entity<Buffer>> {
        self.all_buffers_iter().collect()
    }

    pub fn buffer(&self, buffer_id: BufferId) -> Option<Entity<Buffer>> {
        self.buffers
            .get(&buffer_id)
            .map(|state| state.buffer.clone())
    }

    pub fn language_at<T: ToOffset>(&self, point: T, cx: &App) -> Option<Arc<Language>> {
        self.point_to_buffer_offset(point, cx)
            .and_then(|(buffer, offset)| buffer.read(cx).language_at(offset))
    }

    pub fn language_settings<'a>(&'a self, cx: &'a App) -> Cow<'a, LanguageSettings> {
        let snapshot = self.snapshot(cx);
        snapshot
            .excerpts
            .first()
            .and_then(|excerpt| self.buffer(excerpt.range.context.start.buffer_id))
            .map(|buffer| LanguageSettings::for_buffer(&buffer.read(cx), cx))
            .unwrap_or_else(move || self.language_settings_at(MultiBufferOffset::default(), cx))
    }

    pub fn language_settings_at<'a, T: ToOffset>(
        &'a self,
        point: T,
        cx: &'a App,
    ) -> Cow<'a, LanguageSettings> {
        if let Some((buffer, offset)) = self.point_to_buffer_offset(point, cx) {
            LanguageSettings::for_buffer_at(buffer.read(cx), offset, cx)
        } else {
            Cow::Borrowed(&AllLanguageSettings::get_global(cx).defaults)
        }
    }

    pub fn for_each_buffer(&self, f: &mut dyn FnMut(&Entity<Buffer>)) {
        self.buffers.values().for_each(|state| f(&state.buffer))
    }

    pub fn explicit_title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn title<'a>(&'a self, cx: &'a App) -> Cow<'a, str> {
        if let Some(title) = self.title.as_ref() {
            return title.into();
        }

        if let Some(buffer) = self.as_singleton() {
            let buffer = buffer.read(cx);

            if let Some(file) = buffer.file() {
                return file.file_name(cx).into();
            }

            if let Some(title) = self.buffer_content_title(buffer) {
                return title;
            }
        };

        "untitled".into()
    }

    fn buffer_content_title(&self, buffer: &Buffer) -> Option<Cow<'_, str>> {
        let mut is_leading_whitespace = true;
        let mut count = 0;
        let mut prev_was_space = false;
        let mut title = String::new();

        for ch in buffer.snapshot().chars() {
            if is_leading_whitespace && ch.is_whitespace() {
                continue;
            }

            is_leading_whitespace = false;

            if ch == '\n' || count >= 40 {
                break;
            }

            if ch.is_whitespace() {
                if !prev_was_space {
                    title.push(' ');
                    count += 1;
                    prev_was_space = true;
                }
            } else {
                title.push(ch);
                count += 1;
                prev_was_space = false;
            }
        }

        let title = title.trim_end().to_string();

        if title.is_empty() {
            return None;
        }

        Some(title.into())
    }

    pub fn set_title(&mut self, title: String, cx: &mut Context<Self>) {
        self.title = Some(title);
        cx.notify();
    }

    /// Preserve preview tabs containing this multibuffer until additional edits occur.
    pub fn refresh_preview(&self, cx: &mut Context<Self>) {
        for buffer_state in self.buffers.values() {
            buffer_state
                .buffer
                .update(cx, |buffer, _cx| buffer.refresh_preview());
        }
    }

    /// Whether we should preserve the preview status of a tab containing this multi-buffer.
    pub fn preserve_preview(&self, cx: &App) -> bool {
        self.buffers
            .values()
            .all(|state| state.buffer.read(cx).preserve_preview())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn is_parsing(&self, cx: &App) -> bool {
        self.as_singleton().unwrap().read(cx).is_parsing()
    }

    pub fn add_diff(&mut self, diff: Entity<BufferDiff>, cx: &mut Context<Self>) {
        let buffer_id = diff.read(cx).buffer_id;

        if let Some(existing_diff) = self.diff_for(buffer_id)
            && diff.entity_id() == existing_diff.entity_id()
        {
            return;
        }

        self.buffer_diff_changed(
            diff.clone(),
            Some(text::Anchor::min_max_range_for_buffer(buffer_id)),
            cx,
        );
        self.diffs.insert(buffer_id, DiffState::new(diff, cx));
    }

    pub fn add_inverted_diff(
        &mut self,
        diff: Entity<BufferDiff>,
        main_buffer: Entity<language::Buffer>,
        cx: &mut Context<Self>,
    ) {
        let snapshot = diff.read(cx).base_text(cx);
        let base_text_buffer_id = snapshot.remote_id();
        let diff_change_range = 0..snapshot.len();
        self.snapshot.get_mut().has_inverted_diff = true;
        self.inverted_buffer_diff_changed(
            diff.clone(),
            main_buffer.clone(),
            Some(diff_change_range),
            cx,
        );
        self.diffs.insert(
            base_text_buffer_id,
            DiffState::new_inverted(diff, main_buffer, cx),
        );
    }

    pub fn diff_for(&self, buffer_id: BufferId) -> Option<Entity<BufferDiff>> {
        self.diffs.get(&buffer_id).map(|state| state.diff.clone())
    }

    pub fn expand_diff_hunks(&mut self, ranges: Vec<Range<Anchor>>, cx: &mut Context<Self>) {
        self.expand_or_collapse_diff_hunks(ranges, true, cx);
    }

    pub fn collapse_diff_hunks(&mut self, ranges: Vec<Range<Anchor>>, cx: &mut Context<Self>) {
        self.expand_or_collapse_diff_hunks(ranges, false, cx);
    }

    pub fn set_all_diff_hunks_expanded(&mut self, cx: &mut Context<Self>) {
        self.snapshot.get_mut().all_diff_hunks_expanded = true;
        self.expand_or_collapse_diff_hunks(vec![Anchor::Min..Anchor::Max], true, cx);
    }

    pub fn all_diff_hunks_expanded(&self) -> bool {
        self.snapshot.borrow().all_diff_hunks_expanded
    }

    pub fn set_all_diff_hunks_collapsed(&mut self, cx: &mut Context<Self>) {
        self.snapshot.get_mut().all_diff_hunks_expanded = false;
        self.expand_or_collapse_diff_hunks(vec![Anchor::Min..Anchor::Max], false, cx);
    }

    pub fn set_show_deleted_hunks(&mut self, show: bool, cx: &mut Context<Self>) {
        self.snapshot.get_mut().show_deleted_hunks = show;

        self.sync_mut(cx);

        let old_len = self.snapshot.borrow().len();

        let ranges = std::iter::once((Point::zero()..Point::MAX, None));
        let _ = self.expand_or_collapse_diff_hunks_inner(ranges, true, cx);

        let new_len = self.snapshot.borrow().len();

        self.subscriptions.publish(vec![Edit {
            old: MultiBufferOffset(0)..old_len,
            new: MultiBufferOffset(0)..new_len,
        }]);

        cx.emit(Event::DiffHunksToggled);
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }

    pub fn set_use_extended_diff_range(&mut self, use_extended: bool, _cx: &mut Context<Self>) {
        self.snapshot.get_mut().use_extended_diff_range = use_extended;
    }

    pub fn has_multiple_hunks(&self, cx: &App) -> bool {
        self.read(cx)
            .diff_hunks_in_range(Anchor::Min..Anchor::Max)
            .nth(1)
            .is_some()
    }

    pub fn single_hunk_is_expanded(&self, range: Range<Anchor>, cx: &App) -> bool {
        let snapshot = self.read(cx);
        let mut cursor = snapshot.diff_transforms.cursor::<MultiBufferOffset>(());
        let offset_range = range.to_offset(&snapshot);
        cursor.seek(&offset_range.start, Bias::Left);
        while let Some(item) = cursor.item() {
            if *cursor.start() >= offset_range.end && *cursor.start() > offset_range.start {
                break;
            }
            if item.hunk_info().is_some() {
                return true;
            }
            cursor.next();
        }
        false
    }

    pub fn has_expanded_diff_hunks_in_ranges(&self, ranges: &[Range<Anchor>], cx: &App) -> bool {
        let snapshot = self.read(cx);
        let mut cursor = snapshot.diff_transforms.cursor::<MultiBufferOffset>(());
        for range in ranges {
            let range = range.to_point(&snapshot);
            let start = snapshot.point_to_offset(Point::new(range.start.row, 0));
            let end = (snapshot.point_to_offset(Point::new(range.end.row + 1, 0)) + 1usize)
                .min(snapshot.len());
            cursor.seek(&start, Bias::Right);
            while let Some(item) = cursor.item() {
                if *cursor.start() >= end {
                    break;
                }
                if item.hunk_info().is_some() {
                    return true;
                }
                cursor.next();
            }
        }
        false
    }

    pub fn expand_or_collapse_diff_hunks_inner(
        &mut self,
        ranges: impl IntoIterator<Item = (Range<Point>, Option<Anchor>)>,
        expand: bool,
        cx: &mut Context<Self>,
    ) -> Vec<Edit<MultiBufferOffset>> {
        if self.snapshot.borrow().all_diff_hunks_expanded && !expand {
            return Vec::new();
        }
        self.sync_mut(cx);
        let mut snapshot = self.snapshot.get_mut();
        let mut excerpt_edits = Vec::new();
        let mut last_hunk_row = None;
        for (range, end_anchor) in ranges {
            for diff_hunk in snapshot.diff_hunks_in_range(range) {
                if let Some(end_anchor) = &end_anchor
                    && let Some(hunk_end_anchor) =
                        snapshot.anchor_in_excerpt(diff_hunk.excerpt_range.context.end)
                    && hunk_end_anchor.cmp(end_anchor, snapshot).is_gt()
                {
                    continue;
                }
                let hunk_range = diff_hunk.multi_buffer_range;
                if let Some(excerpt_start_anchor) =
                    snapshot.anchor_in_excerpt(diff_hunk.excerpt_range.context.start)
                    && hunk_range.start.to_point(snapshot) < excerpt_start_anchor.to_point(snapshot)
                {
                    continue;
                }
                if last_hunk_row.is_some_and(|row| row >= diff_hunk.row_range.start) {
                    continue;
                }
                let mut start = snapshot.excerpt_offset_for_anchor(&hunk_range.start);
                let mut end = snapshot.excerpt_offset_for_anchor(&hunk_range.end);
                if let Some(excerpt_end_anchor) =
                    snapshot.anchor_in_excerpt(diff_hunk.excerpt_range.context.end)
                {
                    let excerpt_end = snapshot.excerpt_offset_for_anchor(&excerpt_end_anchor);
                    start = start.min(excerpt_end);
                    end = end.min(excerpt_end);
                };
                last_hunk_row = Some(diff_hunk.row_range.start);
                excerpt_edits.push(text::Edit {
                    old: start..end,
                    new: start..end,
                });
            }
        }

        Self::sync_diff_transforms(
            &mut snapshot,
            excerpt_edits,
            DiffChangeKind::ExpandOrCollapseHunks { expand },
        )
    }

    pub fn expand_or_collapse_diff_hunks(
        &mut self,
        ranges: Vec<Range<Anchor>>,
        expand: bool,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.snapshot.borrow().clone();
        let ranges =
            ranges.iter().map(move |range| {
                let excerpt_end = snapshot.excerpt_containing(range.end..range.end).and_then(
                    |(_, excerpt_range)| snapshot.anchor_in_excerpt(excerpt_range.context.end),
                );
                let range = range.to_point(&snapshot);
                let mut peek_end = range.end;
                if range.end.row < snapshot.max_row().0 {
                    peek_end = Point::new(range.end.row + 1, 0);
                };
                (range.start..peek_end, excerpt_end)
            });
        let edits = self.expand_or_collapse_diff_hunks_inner(ranges, expand, cx);
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::DiffHunksToggled);
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }

    #[ztracing::instrument(skip_all)]
    fn sync(&self, cx: &App) {
        let changed = self.buffer_changed_since_sync.replace(false);
        if !changed {
            return;
        }
        let edits = Self::sync_from_buffer_changes(
            &mut self.snapshot.borrow_mut(),
            &self.buffers,
            &self.diffs,
            cx,
        );
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
    }

    fn sync_mut(&mut self, cx: &App) -> &mut MultiBufferSnapshot {
        let snapshot = self.snapshot.get_mut();
        let changed = self.buffer_changed_since_sync.replace(false);
        if !changed {
            return snapshot;
        }
        let edits = Self::sync_from_buffer_changes(snapshot, &self.buffers, &self.diffs, cx);

        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }

        snapshot
    }

    fn sync_from_buffer_changes(
        snapshot: &mut MultiBufferSnapshot,
        buffers: &BTreeMap<BufferId, BufferState>,
        diffs: &HashMap<BufferId, DiffState>,
        cx: &App,
    ) -> Vec<Edit<MultiBufferOffset>> {
        let MultiBufferSnapshot {
            excerpts,
            diffs: buffer_diff,
            buffers: buffer_snapshots,
            path_keys: _,
            diff_transforms: _,
            non_text_state_update_count,
            edit_count,
            is_dirty,
            has_deleted_file,
            has_conflict,
            has_inverted_diff: _,
            singleton: _,
            trailing_excerpt_update_count: _,
            all_diff_hunks_expanded: _,
            show_deleted_hunks: _,
            use_extended_diff_range: _,
            show_headers: _,
        } = snapshot;
        *is_dirty = false;
        *has_deleted_file = false;
        *has_conflict = false;

        if !diffs.is_empty() {
            let mut diffs_to_add = Vec::new();
            for (id, diff) in diffs {
                if find_diff_state(buffer_diff, *id).is_none_or(|existing_diff| {
                    if existing_diff.main_buffer.is_none() {
                        return false;
                    }
                    let base_text = diff.diff.read(cx).base_text_buffer().read(cx);
                    base_text.remote_id() != existing_diff.base_text().remote_id()
                        || base_text
                            .version()
                            .changed_since(existing_diff.base_text().version())
                }) {
                    if diffs_to_add.capacity() == 0 {
                        diffs_to_add.reserve(diffs.len());
                    }
                    diffs_to_add.push(sum_tree::Edit::Insert(diff.snapshot(*id, cx)));
                }
            }
            buffer_diff.edit(diffs_to_add, ());
        }

        let mut paths_to_edit = Vec::new();
        let mut non_text_state_updated = false;
        let mut edited = false;
        for buffer_state in buffers.values() {
            let buffer = buffer_state.buffer.read(cx);
            let last_snapshot = buffer_snapshots
                .get(&buffer.remote_id())
                .expect("each buffer should have a snapshot");
            let current_version = buffer.version();
            let non_text_state_update_count = buffer.non_text_state_update_count();

            let buffer_edited =
                current_version.changed_since(last_snapshot.buffer_snapshot.version());
            let buffer_non_text_state_updated = non_text_state_update_count
                > last_snapshot.buffer_snapshot.non_text_state_update_count();
            if buffer_edited || buffer_non_text_state_updated {
                paths_to_edit.push((
                    last_snapshot.path_key.clone(),
                    last_snapshot.path_key_index,
                    buffer_state.buffer.clone(),
                    if buffer_edited {
                        Some(last_snapshot.buffer_snapshot.version().clone())
                    } else {
                        None
                    },
                ));
            }

            edited |= buffer_edited;
            non_text_state_updated |= buffer_non_text_state_updated;
            *is_dirty |= buffer.is_dirty();
            *has_deleted_file |= buffer
                .file()
                .is_some_and(|file| file.disk_state().is_deleted());
            *has_conflict |= buffer.has_conflict();
        }
        if edited {
            *edit_count += 1;
        }
        if non_text_state_updated {
            *non_text_state_update_count += 1;
        }

        paths_to_edit.sort_unstable_by_key(|(path, _, _, _)| path.clone());

        let mut edits = Vec::new();
        let mut new_excerpts = SumTree::default();
        let mut cursor = excerpts.cursor::<ExcerptSummary>(());

        for (path, path_key_index, buffer, prev_version) in paths_to_edit {
            new_excerpts.append(cursor.slice(&path, Bias::Left), ());
            let buffer = buffer.read(cx);
            let buffer_id = buffer.remote_id();

            buffer_snapshots.insert(
                buffer_id,
                BufferStateSnapshot {
                    path_key: path.clone(),
                    path_key_index,
                    buffer_snapshot: buffer.snapshot(),
                },
            );

            if let Some(prev_version) = &prev_version {
                while let Some(old_excerpt) = cursor.item()
                    && &old_excerpt.path_key == &path
                {
                    edits.extend(
                        buffer
                            .edits_since_in_range::<usize>(
                                prev_version,
                                old_excerpt.range.context.clone(),
                            )
                            .map(|edit| {
                                let excerpt_old_start = cursor.start().len();
                                let excerpt_new_start =
                                    ExcerptDimension(new_excerpts.summary().text.len);
                                let old_start = excerpt_old_start + edit.old.start;
                                let old_end = excerpt_old_start + edit.old.end;
                                let new_start = excerpt_new_start + edit.new.start;
                                let new_end = excerpt_new_start + edit.new.end;
                                Edit {
                                    old: old_start..old_end,
                                    new: new_start..new_end,
                                }
                            }),
                    );

                    let excerpt = Excerpt::new(
                        old_excerpt.path_key.clone(),
                        old_excerpt.path_key_index,
                        &buffer.snapshot(),
                        old_excerpt.range.clone(),
                        old_excerpt.has_trailing_newline,
                    );
                    new_excerpts.push(excerpt, ());
                    cursor.next();
                }
            } else {
                new_excerpts.append(cursor.slice(&path, Bias::Right), ());
            };
        }
        new_excerpts.append(cursor.suffix(), ());

        drop(cursor);
        *excerpts = new_excerpts;

        Self::sync_diff_transforms(snapshot, edits, DiffChangeKind::BufferEdited)
    }

    fn sync_diff_transforms(
        snapshot: &mut MultiBufferSnapshot,
        excerpt_edits: Vec<text::Edit<ExcerptOffset>>,
        change_kind: DiffChangeKind,
    ) -> Vec<Edit<MultiBufferOffset>> {
        if excerpt_edits.is_empty() {
            return vec![];
        }

        let mut excerpts = snapshot.excerpts.cursor::<ExcerptOffset>(());
        let mut old_diff_transforms = snapshot
            .diff_transforms
            .cursor::<Dimensions<ExcerptOffset, MultiBufferOffset>>(());
        let mut new_diff_transforms = SumTree::default();
        let mut old_expanded_hunks = HashSet::default();
        let mut output_edits = Vec::new();
        let mut output_delta = 0_isize;
        let mut at_transform_boundary = true;
        let mut end_of_current_insert = None;

        let mut excerpt_edits = excerpt_edits.into_iter().peekable();
        while let Some(edit) = excerpt_edits.next() {
            excerpts.seek_forward(&edit.new.start, Bias::Right);
            if excerpts.item().is_none() && *excerpts.start() == edit.new.start {
                excerpts.prev();
            }

            // Keep any transforms that are before the edit.
            if at_transform_boundary {
                at_transform_boundary = false;
                let transforms_before_edit = old_diff_transforms.slice(&edit.old.start, Bias::Left);
                Self::append_diff_transforms(&mut new_diff_transforms, transforms_before_edit);
                if let Some(transform) = old_diff_transforms.item()
                    && old_diff_transforms.end().0 == edit.old.start
                    && old_diff_transforms.start().0 < edit.old.start
                {
                    Self::push_diff_transform(&mut new_diff_transforms, transform.clone());
                    old_diff_transforms.next();
                }
            }

            // Compute the start of the edit in output coordinates.
            let edit_start_overshoot = edit.old.start - old_diff_transforms.start().0;
            let edit_old_start = old_diff_transforms.start().1 + edit_start_overshoot;
            let edit_new_start =
                MultiBufferOffset((edit_old_start.0 as isize + output_delta) as usize);

            let changed_diff_hunks = Self::recompute_diff_transforms_for_edit(
                &edit,
                &mut excerpts,
                &mut old_diff_transforms,
                &mut new_diff_transforms,
                &mut end_of_current_insert,
                &mut old_expanded_hunks,
                snapshot,
                change_kind,
            );

            // Compute the end of the edit in output coordinates.
            let edit_old_end_overshoot = edit.old.end - old_diff_transforms.start().0;
            let edit_new_end_overshoot = edit.new.end - new_diff_transforms.summary().excerpt_len();
            let edit_old_end = old_diff_transforms.start().1 + edit_old_end_overshoot;
            let edit_new_end = new_diff_transforms.summary().output.len + edit_new_end_overshoot;
            let output_edit = Edit {
                old: edit_old_start..edit_old_end,
                new: edit_new_start..edit_new_end,
            };

            output_delta += (output_edit.new.end - output_edit.new.start) as isize;
            output_delta -= (output_edit.old.end - output_edit.old.start) as isize;
            if changed_diff_hunks || matches!(change_kind, DiffChangeKind::BufferEdited) {
                output_edits.push(output_edit);
            }

            // If this is the last edit that intersects the current diff transform,
            // then recreate the content up to the end of this transform, to prepare
            // for reusing additional slices of the old transforms.
            if excerpt_edits
                .peek()
                .is_none_or(|next_edit| next_edit.old.start >= old_diff_transforms.end().0)
            {
                let keep_next_old_transform = (old_diff_transforms.start().0 >= edit.old.end)
                    && match old_diff_transforms.item() {
                        Some(DiffTransform::BufferContent {
                            inserted_hunk_info: Some(hunk),
                            ..
                        }) => excerpts.item().is_some_and(|excerpt| {
                            if let Some(diff) = find_diff_state(&snapshot.diffs, excerpt.buffer_id)
                                && diff.main_buffer.is_some()
                            {
                                return true;
                            }
                            hunk.hunk_start_anchor
                                .is_valid(&excerpt.buffer_snapshot(&snapshot))
                        }),
                        _ => true,
                    };

                let mut excerpt_offset = edit.new.end;
                if !keep_next_old_transform {
                    excerpt_offset += old_diff_transforms.end().0 - edit.old.end;
                    old_diff_transforms.next();
                }

                old_expanded_hunks.clear();
                Self::push_buffer_content_transform(
                    snapshot,
                    &mut new_diff_transforms,
                    excerpt_offset,
                    end_of_current_insert,
                );
                at_transform_boundary = true;
            }
        }

        // Keep any transforms that are after the last edit.
        Self::append_diff_transforms(&mut new_diff_transforms, old_diff_transforms.suffix());

        // Ensure there's always at least one buffer content transform.
        if new_diff_transforms.is_empty() {
            new_diff_transforms.push(
                DiffTransform::BufferContent {
                    summary: Default::default(),
                    inserted_hunk_info: None,
                },
                (),
            );
        }

        drop(old_diff_transforms);
        drop(excerpts);
        snapshot.diff_transforms = new_diff_transforms;
        snapshot.edit_count += 1;

        #[cfg(any(test, feature = "test-support"))]
        snapshot.check_invariants();
        output_edits
    }

    fn recompute_diff_transforms_for_edit(
        edit: &Edit<ExcerptOffset>,
        excerpts: &mut Cursor<Excerpt, ExcerptOffset>,
        old_diff_transforms: &mut Cursor<
            DiffTransform,
            Dimensions<ExcerptOffset, MultiBufferOffset>,
        >,
        new_diff_transforms: &mut SumTree<DiffTransform>,
        end_of_current_insert: &mut Option<(ExcerptOffset, DiffTransformHunkInfo)>,
        old_expanded_hunks: &mut HashSet<DiffTransformHunkInfo>,
        snapshot: &MultiBufferSnapshot,
        change_kind: DiffChangeKind,
    ) -> bool {
        log::trace!(
            "recomputing diff transform for edit {:?} => {:?}",
            edit.old.start..edit.old.end,
            edit.new.start..edit.new.end
        );

        // Record which hunks were previously expanded.
        while let Some(item) = old_diff_transforms.item() {
            if let Some(hunk_info) = item.hunk_info() {
                log::trace!(
                    "previously expanded hunk at {:?}",
                    old_diff_transforms.start()
                );
                old_expanded_hunks.insert(hunk_info);
            }
            if old_diff_transforms.end().0 > edit.old.end {
                break;
            }
            old_diff_transforms.next();
        }

        // Avoid querying diff hunks if there's no possibility of hunks being expanded.
        // For inverted diffs, hunks are always shown, so we can't skip this.
        let all_diff_hunks_expanded = snapshot.all_diff_hunks_expanded;
        if old_expanded_hunks.is_empty()
            && change_kind == DiffChangeKind::BufferEdited
            && !all_diff_hunks_expanded
            && !snapshot.has_inverted_diff
        {
            return false;
        }

        // Visit each excerpt that intersects the edit.
        let mut did_expand_hunks = false;
        while let Some(excerpt) = excerpts.item() {
            // Recompute the expanded hunks in the portion of the excerpt that
            // intersects the edit.
            if let Some(diff) = find_diff_state(&snapshot.diffs, excerpt.buffer_id) {
                let buffer_snapshot = &excerpt.buffer_snapshot(&snapshot);
                let excerpt_start = *excerpts.start();
                let excerpt_end = excerpt_start + excerpt.text_summary.len;
                let excerpt_buffer_start = excerpt.range.context.start.to_offset(buffer_snapshot);
                let excerpt_buffer_end = excerpt_buffer_start + excerpt.text_summary.len;
                let edit_buffer_start =
                    excerpt_buffer_start + edit.new.start.saturating_sub(excerpt_start);
                let edit_buffer_end =
                    excerpt_buffer_start + edit.new.end.saturating_sub(excerpt_start);
                let edit_buffer_end = edit_buffer_end.min(excerpt_buffer_end);

                if let Some(main_buffer) = &diff.main_buffer {
                    for hunk in diff.hunks_intersecting_base_text_range(
                        edit_buffer_start..edit_buffer_end,
                        main_buffer,
                    ) {
                        did_expand_hunks = true;
                        let hunk_buffer_range = hunk.diff_base_byte_range.clone();
                        if hunk_buffer_range.start < excerpt_buffer_start {
                            log::trace!("skipping hunk that starts before excerpt");
                            continue;
                        }
                        let hunk_excerpt_start = excerpt_start
                            + hunk_buffer_range.start.saturating_sub(excerpt_buffer_start);
                        let hunk_excerpt_end = excerpt_end
                            .min(excerpt_start + (hunk_buffer_range.end - excerpt_buffer_start));
                        Self::push_buffer_content_transform(
                            snapshot,
                            new_diff_transforms,
                            hunk_excerpt_start,
                            *end_of_current_insert,
                        );
                        if !hunk_buffer_range.is_empty() {
                            let hunk_info = DiffTransformHunkInfo {
                                buffer_id: buffer_snapshot.remote_id(),
                                hunk_start_anchor: hunk.buffer_range.start,
                                hunk_secondary_status: hunk.secondary_status,
                                excerpt_end: excerpt.end_anchor(),
                                is_logically_deleted: true,
                            };
                            *end_of_current_insert =
                                Some((hunk_excerpt_end.min(excerpt_end), hunk_info));
                        }
                    }
                } else {
                    let edit_anchor_range = buffer_snapshot.anchor_before(edit_buffer_start)
                        ..buffer_snapshot.anchor_after(edit_buffer_end);
                    for hunk in diff.hunks_intersecting_range(edit_anchor_range, buffer_snapshot) {
                        if hunk.is_created_file() && !all_diff_hunks_expanded {
                            continue;
                        }

                        let hunk_buffer_range = hunk.buffer_range.to_offset(buffer_snapshot);
                        if hunk_buffer_range.start < excerpt_buffer_start {
                            log::trace!("skipping hunk that starts before excerpt");
                            continue;
                        }

                        let hunk_info = DiffTransformHunkInfo {
                            buffer_id: buffer_snapshot.remote_id(),
                            hunk_start_anchor: hunk.buffer_range.start,
                            hunk_secondary_status: hunk.secondary_status,
                            excerpt_end: excerpt.end_anchor(),
                            is_logically_deleted: false,
                        };

                        let hunk_excerpt_start = excerpt_start
                            + hunk_buffer_range.start.saturating_sub(excerpt_buffer_start);
                        let hunk_excerpt_end = excerpt_end
                            .min(excerpt_start + (hunk_buffer_range.end - excerpt_buffer_start));

                        Self::push_buffer_content_transform(
                            snapshot,
                            new_diff_transforms,
                            hunk_excerpt_start,
                            *end_of_current_insert,
                        );

                        // For every existing hunk, determine if it was previously expanded
                        // and if it should currently be expanded.
                        let was_previously_expanded = old_expanded_hunks.contains(&hunk_info);
                        let should_expand_hunk = match &change_kind {
                            DiffChangeKind::DiffUpdated { base_changed: true } => {
                                was_previously_expanded || all_diff_hunks_expanded
                            }
                            DiffChangeKind::ExpandOrCollapseHunks { expand } => {
                                let intersects = hunk_buffer_range.is_empty()
                                    || (hunk_buffer_range.end > edit_buffer_start);
                                if *expand {
                                    intersects || was_previously_expanded || all_diff_hunks_expanded
                                } else {
                                    !intersects
                                        && (was_previously_expanded || all_diff_hunks_expanded)
                                }
                            }
                            _ => was_previously_expanded || all_diff_hunks_expanded,
                        };

                        if should_expand_hunk {
                            did_expand_hunks = true;
                            log::trace!(
                                "expanding hunk {:?}",
                                hunk_excerpt_start..hunk_excerpt_end,
                            );

                            if !hunk.diff_base_byte_range.is_empty()
                                && hunk_buffer_range.start >= edit_buffer_start
                                && hunk_buffer_range.start <= excerpt_buffer_end
                                && snapshot.show_deleted_hunks
                            {
                                let base_text = diff.base_text();
                                let mut text_cursor =
                                    base_text.as_rope().cursor(hunk.diff_base_byte_range.start);
                                let mut base_text_summary = text_cursor
                                    .summary::<TextSummary>(hunk.diff_base_byte_range.end);

                                let mut has_trailing_newline = false;
                                if base_text_summary.last_line_chars > 0 {
                                    base_text_summary += TextSummary::newline();
                                    has_trailing_newline = true;
                                }

                                new_diff_transforms.push(
                                    DiffTransform::DeletedHunk {
                                        base_text_byte_range: hunk.diff_base_byte_range.clone(),
                                        summary: base_text_summary,
                                        buffer_id: buffer_snapshot.remote_id(),
                                        hunk_info,
                                        has_trailing_newline,
                                    },
                                    (),
                                );
                            }

                            if !hunk_buffer_range.is_empty() {
                                *end_of_current_insert =
                                    Some((hunk_excerpt_end.min(excerpt_end), hunk_info));
                            }
                        }
                    }
                }
            }

            if excerpts.end() <= edit.new.end {
                excerpts.next();
            } else {
                break;
            }
        }

        did_expand_hunks || !old_expanded_hunks.is_empty()
    }

    fn append_diff_transforms(
        new_transforms: &mut SumTree<DiffTransform>,
        subtree: SumTree<DiffTransform>,
    ) {
        if let Some(DiffTransform::BufferContent {
            inserted_hunk_info,
            summary,
        }) = subtree.first()
            && Self::extend_last_buffer_content_transform(
                new_transforms,
                *inserted_hunk_info,
                *summary,
            )
        {
            let mut cursor = subtree.cursor::<()>(());
            cursor.next();
            cursor.next();
            new_transforms.append(cursor.suffix(), ());
            return;
        }
        new_transforms.append(subtree, ());
    }

    fn push_diff_transform(new_transforms: &mut SumTree<DiffTransform>, transform: DiffTransform) {
        if let DiffTransform::BufferContent {
            inserted_hunk_info: inserted_hunk_anchor,
            summary,
        } = transform
            && Self::extend_last_buffer_content_transform(
                new_transforms,
                inserted_hunk_anchor,
                summary,
            )
        {
            return;
        }
        new_transforms.push(transform, ());
    }

    fn push_buffer_content_transform(
        old_snapshot: &MultiBufferSnapshot,
        new_transforms: &mut SumTree<DiffTransform>,
        end_offset: ExcerptOffset,
        current_inserted_hunk: Option<(ExcerptOffset, DiffTransformHunkInfo)>,
    ) {
        let inserted_region = current_inserted_hunk.map(|(insertion_end_offset, hunk_info)| {
            (end_offset.min(insertion_end_offset), Some(hunk_info))
        });
        let unchanged_region = [(end_offset, None)];

        for (end_offset, inserted_hunk_info) in inserted_region.into_iter().chain(unchanged_region)
        {
            let start_offset = new_transforms.summary().excerpt_len();
            if end_offset <= start_offset {
                continue;
            }
            let summary_to_add = old_snapshot
                .text_summary_for_excerpt_offset_range::<MBTextSummary>(start_offset..end_offset);

            if !Self::extend_last_buffer_content_transform(
                new_transforms,
                inserted_hunk_info,
                summary_to_add,
            ) {
                new_transforms.push(
                    DiffTransform::BufferContent {
                        summary: summary_to_add,
                        inserted_hunk_info,
                    },
                    (),
                )
            }
        }
    }

    fn extend_last_buffer_content_transform(
        new_transforms: &mut SumTree<DiffTransform>,
        new_inserted_hunk_info: Option<DiffTransformHunkInfo>,
        summary_to_add: MBTextSummary,
    ) -> bool {
        let mut did_extend = false;
        new_transforms.update_last(
            |last_transform| {
                if let DiffTransform::BufferContent {
                    summary,
                    inserted_hunk_info: inserted_hunk_anchor,
                } = last_transform
                    && *inserted_hunk_anchor == new_inserted_hunk_info
                {
                    *summary += summary_to_add;
                    did_extend = true;
                }
            },
            (),
        );
        did_extend
    }

    pub fn toggle_single_diff_hunk(&mut self, range: Range<Anchor>, cx: &mut Context<Self>) {
        let snapshot = self.snapshot(cx);
        let excerpt_end = snapshot
            .excerpt_containing(range.end..range.end)
            .and_then(|(_, excerpt_range)| snapshot.anchor_in_excerpt(excerpt_range.context.end));
        let point_range = range.to_point(&snapshot);
        let expand = !self.single_hunk_is_expanded(range, cx);
        let edits =
            self.expand_or_collapse_diff_hunks_inner([(point_range, excerpt_end)], expand, cx);
        if !edits.is_empty() {
            self.subscriptions.publish(edits);
        }
        cx.emit(Event::DiffHunksToggled);
        cx.emit(Event::Edited {
            edited_buffer: None,
            source: BufferEditSource::User,
        });
    }
}

impl EventEmitter<Event> for MultiBuffer {}

impl MultiBufferSnapshot {
    pub fn diff_hunk_before<T: ToOffset>(&self, position: T) -> Option<MultiBufferRow> {
        let offset = position.to_offset(self);

        let mut cursor = self
            .cursor::<DimensionPair<MultiBufferOffset, Point>, DimensionPair<BufferOffset, Point>>(
            );
        cursor.seek(&DimensionPair {
            key: offset,
            value: None,
        });
        cursor.seek_to_start_of_current_excerpt();
        let excerpt = cursor.excerpt()?;

        let buffer = excerpt.buffer_snapshot(self);
        let excerpt_start = excerpt.range.context.start.to_offset(buffer);
        let excerpt_end = excerpt.range.context.end.to_offset(buffer);
        let current_position = match self.anchor_before(offset) {
            Anchor::Min => 0,
            Anchor::Excerpt(excerpt_anchor) => excerpt_anchor.text_anchor().to_offset(buffer),
            Anchor::Max => unreachable!(),
        };

        if let Some(diff) = self.diff_state(excerpt.buffer_id) {
            if let Some(main_buffer) = &diff.main_buffer {
                for hunk in diff
                    .hunks_intersecting_base_text_range_rev(excerpt_start..excerpt_end, main_buffer)
                {
                    if hunk.diff_base_byte_range.end >= current_position {
                        continue;
                    }
                    let hunk_start = buffer.anchor_after(hunk.diff_base_byte_range.start);
                    let start =
                        Anchor::in_buffer(excerpt.path_key_index, hunk_start).to_point(self);
                    return Some(MultiBufferRow(start.row));
                }
            } else {
                let excerpt_end = buffer.anchor_before(excerpt_end.min(current_position));
                for hunk in diff
                    .hunks_intersecting_range_rev(excerpt.range.context.start..excerpt_end, buffer)
                {
                    let hunk_end = hunk.buffer_range.end.to_offset(buffer);
                    if hunk_end >= current_position {
                        continue;
                    }
                    let start = Anchor::in_buffer(excerpt.path_key_index, hunk.buffer_range.start)
                        .to_point(self);
                    return Some(MultiBufferRow(start.row));
                }
            }
        }

        loop {
            cursor.prev_excerpt();
            let excerpt = cursor.excerpt()?;
            let buffer = excerpt.buffer_snapshot(self);

            let Some(diff) = self.diff_state(excerpt.buffer_id) else {
                continue;
            };
            if let Some(main_buffer) = &diff.main_buffer {
                let Some(hunk) = diff
                    .hunks_intersecting_base_text_range_rev(
                        excerpt.range.context.to_offset(buffer),
                        main_buffer,
                    )
                    .next()
                else {
                    continue;
                };
                let hunk_start = buffer.anchor_after(hunk.diff_base_byte_range.start);
                let start = Anchor::in_buffer(excerpt.path_key_index, hunk_start).to_point(self);
                return Some(MultiBufferRow(start.row));
            } else {
                let Some(hunk) = diff
                    .hunks_intersecting_range_rev(excerpt.range.context.clone(), buffer)
                    .next()
                else {
                    continue;
                };
                let start = Anchor::in_buffer(excerpt.path_key_index, hunk.buffer_range.start)
                    .to_point(self);
                return Some(MultiBufferRow(start.row));
            }
        }
    }

    pub fn has_diff_hunks(&self) -> bool {
        self.diffs.iter().any(|diff| !diff.is_empty())
    }

    pub fn is_inside_word<T: ToOffset>(
        &self,
        position: T,
        scope_context: Option<CharScopeContext>,
    ) -> bool {
        let position = position.to_offset(self);
        let classifier = self
            .char_classifier_at(position)
            .scope_context(scope_context);
        let next_char_kind = self.chars_at(position).next().map(|c| classifier.kind(c));
        let prev_char_kind = self
            .reversed_chars_at(position)
            .next()
            .map(|c| classifier.kind(c));
        prev_char_kind.zip(next_char_kind) == Some((CharKind::Word, CharKind::Word))
    }

    pub fn surrounding_word<T: ToOffset>(
        &self,
        start: T,
        scope_context: Option<CharScopeContext>,
    ) -> (Range<MultiBufferOffset>, Option<CharKind>) {
        let mut start = start.to_offset(self);
        let mut end = start;
        let mut next_chars = self.chars_at(start).peekable();
        let mut prev_chars = self.reversed_chars_at(start).peekable();

        let classifier = self.char_classifier_at(start).scope_context(scope_context);

        let word_kind = cmp::max(
            prev_chars.peek().copied().map(|c| classifier.kind(c)),
            next_chars.peek().copied().map(|c| classifier.kind(c)),
        );

        for ch in prev_chars {
            if Some(classifier.kind(ch)) == word_kind && ch != '\n' {
                start -= ch.len_utf8();
            } else {
                break;
            }
        }

        for ch in next_chars {
            if Some(classifier.kind(ch)) == word_kind && ch != '\n' {
                end += ch.len_utf8();
            } else {
                break;
            }
        }

        (start..end, word_kind)
    }

    pub fn char_kind_before<T: ToOffset>(
        &self,
        start: T,
        scope_context: Option<CharScopeContext>,
    ) -> Option<CharKind> {
        let start = start.to_offset(self);
        let classifier = self.char_classifier_at(start).scope_context(scope_context);
        self.reversed_chars_at(start)
            .next()
            .map(|ch| classifier.kind(ch))
    }

    pub fn all_buffer_ids(&self) -> impl Iterator<Item = BufferId> + '_ {
        self.buffers.iter().map(|(id, _)| *id)
    }

    pub fn is_singleton(&self) -> bool {
        self.singleton
    }

    pub fn as_singleton(&self) -> Option<&BufferSnapshot> {
        if self.is_singleton() {
            Some(self.excerpts.first()?.buffer_snapshot(&self))
        } else {
            None
        }
    }

    pub fn len(&self) -> MultiBufferOffset {
        self.diff_transforms.summary().output.len
    }

    pub fn max_position<MBD: MultiBufferDimension>(&self) -> MBD {
        MBD::from_summary(&self.text_summary())
    }

    pub fn is_empty(&self) -> bool {
        self.diff_transforms.summary().output.len == MultiBufferOffset(0)
    }

    pub fn widest_line_number(&self) -> u32 {
        // widest_line_number is 0-based, so 1 is added to get the displayed line number.
        self.excerpts.summary().widest_line_number + 1
    }

    pub fn suggested_indents(
        &self,
        rows: impl IntoIterator<Item = u32>,
        cx: &App,
    ) -> BTreeMap<MultiBufferRow, IndentSize> {
        let mut result = BTreeMap::new();
        self.suggested_indents_callback(
            rows,
            &mut |row, indent| {
                result.insert(row, indent);
                ControlFlow::Continue(())
            },
            cx,
        );
        result
    }

    // move this to be a generator once those are a thing
    pub fn suggested_indents_callback(
        &self,
        rows: impl IntoIterator<Item = u32>,
        cb: &mut dyn FnMut(MultiBufferRow, IndentSize) -> ControlFlow<()>,
        cx: &App,
    ) {
        let mut rows_for_excerpt = Vec::new();
        let mut cursor = self.cursor::<Point, Point>();
        let mut rows = rows.into_iter().peekable();
        let mut prev_row = u32::MAX;
        let mut prev_language_indent_size = IndentSize::default();

        while let Some(row) = rows.next() {
            cursor.seek(&Point::new(row, 0));
            let Some(region) = cursor.region() else {
                continue;
            };

            // Retrieve the language and indent size once for each disjoint region being indented.
            let single_indent_size = if row.saturating_sub(1) == prev_row {
                prev_language_indent_size
            } else {
                region
                    .buffer
                    .language_indent_size_at(Point::new(row, 0), cx)
            };
            prev_language_indent_size = single_indent_size;
            prev_row = row;

            let start_buffer_row = region.buffer_range.start.row;
            let start_multibuffer_row = region.range.start.row;
            let end_multibuffer_row = region.range.end.row;

            rows_for_excerpt.push(row);
            while let Some(next_row) = rows.peek().copied() {
                if end_multibuffer_row > next_row {
                    rows_for_excerpt.push(next_row);
                    rows.next();
                } else {
                    break;
                }
            }

            let buffer_rows = rows_for_excerpt
                .drain(..)
                .map(|row| start_buffer_row + row - start_multibuffer_row);
            let buffer_indents = region
                .buffer
                .suggested_indents(buffer_rows, single_indent_size);
            for (row, indent) in buffer_indents {
                if cb(
                    MultiBufferRow(start_multibuffer_row + row - start_buffer_row),
                    indent,
                )
                .is_break()
                {
                    return;
                }
            }
        }
    }

    pub fn indent_size_for_line(&self, row: MultiBufferRow) -> IndentSize {
        if let Some((buffer, range)) = self.buffer_line_for_row(row) {
            let mut size = buffer.indent_size_for_line(range.start.row);
            size.len = size
                .len
                .min(range.end.column)
                .saturating_sub(range.start.column);
            size
        } else {
            IndentSize::spaces(0)
        }
    }

    pub fn line_indent_for_row(&self, row: MultiBufferRow) -> LineIndent {
        if let Some((buffer, range)) = self.buffer_line_for_row(row) {
            LineIndent::from_iter(buffer.text_for_range(range).flat_map(|s| s.chars()))
        } else {
            LineIndent::spaces(0)
        }
    }

    pub fn indent_and_comment_for_line(&self, row: MultiBufferRow, cx: &App) -> String {
        let mut indent = self.indent_size_for_line(row).chars().collect::<String>();

        if self.language_settings(cx).extend_comment_on_newline
            && let Some(language_scope) = self.language_scope_at(Point::new(row.0, 0))
        {
            let delimiters = language_scope.line_comment_prefixes();
            for delimiter in delimiters {
                if *self
                    .chars_at(Point::new(row.0, indent.len() as u32))
                    .take(delimiter.chars().count())
                    .collect::<String>()
                    .as_str()
                    == **delimiter
                {
                    indent.push_str(delimiter);
                    break;
                }
            }
        }

        indent
    }

    pub fn is_line_whitespace_upto<T>(&self, position: T) -> bool
    where
        T: ToOffset,
    {
        for char in self.reversed_chars_at(position) {
            if !char.is_whitespace() {
                return false;
            }
            if char == '\n' {
                return true;
            }
        }
        true
    }

    pub fn prev_non_blank_row(&self, mut row: MultiBufferRow) -> Option<MultiBufferRow> {
        while row.0 > 0 {
            row.0 -= 1;
            if !self.is_line_blank(row) {
                return Some(row);
            }
        }
        None
    }

    pub fn line_len(&self, row: MultiBufferRow) -> u32 {
        if let Some((_, range)) = self.buffer_line_for_row(row) {
            range.end.column - range.start.column
        } else {
            0
        }
    }

    pub fn line_len_utf16(&self, row: MultiBufferRow) -> u32 {
        self.clip_point_utf16(Unclipped(PointUtf16::new(row.0, u32::MAX)), Bias::Left)
            .column
    }

    pub fn buffer_line_for_row(
        &self,
        row: MultiBufferRow,
    ) -> Option<(&BufferSnapshot, Range<Point>)> {
        let mut cursor = self.cursor::<Point, Point>();
        let point = Point::new(row.0, 0);
        cursor.seek(&point);
        let region = cursor.region()?;
        let overshoot = point.min(region.range.end) - region.range.start;
        let buffer_point = region.buffer_range.start + overshoot;
        if buffer_point.row > region.buffer_range.end.row {
            return None;
        }
        let line_start = Point::new(buffer_point.row, 0).max(region.buffer_range.start);
        let line_end = Point::new(buffer_point.row, region.buffer.line_len(buffer_point.row))
            .min(region.buffer_range.end);
        Some((region.buffer, line_start..line_end))
    }

    pub fn max_point(&self) -> Point {
        self.text_summary().lines
    }

    pub fn max_row(&self) -> MultiBufferRow {
        MultiBufferRow(self.text_summary().lines.row)
    }

    pub fn text_summary(&self) -> MBTextSummary {
        self.diff_transforms.summary().output
    }

    pub fn text_summary_for_range<MBD, O>(&self, range: Range<O>) -> MBD
    where
        MBD: MultiBufferDimension + AddAssign,
        O: ToOffset,
    {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut cursor = self
            .diff_transforms
            .cursor::<Dimensions<MultiBufferOffset, ExcerptOffset>>(());
        cursor.seek(&range.start, Bias::Right);

        let Some(first_transform) = cursor.item() else {
            return MBD::from_summary(&MBTextSummary::default());
        };

        let diff_transform_start = cursor.start().0;
        let diff_transform_end = cursor.end().0;
        let diff_start = range.start;
        let start_overshoot = diff_start - diff_transform_start;
        let end_overshoot = std::cmp::min(range.end, diff_transform_end) - diff_transform_start;

        let mut result = match first_transform {
            DiffTransform::BufferContent { .. } => {
                let excerpt_start = cursor.start().1 + start_overshoot;
                let excerpt_end = cursor.start().1 + end_overshoot;
                self.text_summary_for_excerpt_offset_range(excerpt_start..excerpt_end)
            }
            DiffTransform::DeletedHunk {
                buffer_id,
                base_text_byte_range,
                has_trailing_newline,
                ..
            } => {
                let buffer_start = base_text_byte_range.start + start_overshoot;
                let mut buffer_end = base_text_byte_range.start + end_overshoot;
                let Some(base_text) = self.diff_state(*buffer_id).map(|diff| diff.base_text())
                else {
                    panic!("{:?} is in non-existent deleted hunk", range.start)
                };

                let include_trailing_newline =
                    *has_trailing_newline && range.end >= diff_transform_end;
                if include_trailing_newline {
                    buffer_end -= 1;
                }

                let mut summary = base_text
                    .text_summary_for_range::<MBD::TextDimension, _>(buffer_start..buffer_end);

                if include_trailing_newline {
                    summary.add_assign(&<MBD::TextDimension>::from_text_summary(
                        &TextSummary::newline(),
                    ))
                }

                let mut result = MBD::default();
                result.add_text_dim(&summary);
                result
            }
        };
        if range.end < diff_transform_end {
            return result;
        }

        cursor.next();
        result.add_mb_text_summary(
            &cursor
                .summary::<_, OutputDimension<_>>(&range.end, Bias::Right)
                .0,
        );

        let Some(last_transform) = cursor.item() else {
            return result;
        };

        let overshoot = range.end - cursor.start().0;
        let suffix = match last_transform {
            DiffTransform::BufferContent { .. } => {
                let end = cursor.start().1 + overshoot;
                self.text_summary_for_excerpt_offset_range::<MBD>(cursor.start().1..end)
            }
            DiffTransform::DeletedHunk {
                base_text_byte_range,
                buffer_id,
                has_trailing_newline,
                ..
            } => {
                let buffer_end = base_text_byte_range.start + overshoot;
                let Some(base_text) = self.diff_state(*buffer_id).map(|diff| diff.base_text())
                else {
                    panic!("{:?} is in non-existent deleted hunk", range.end)
                };

                let mut suffix = base_text.text_summary_for_range::<MBD::TextDimension, _>(
                    base_text_byte_range.start..buffer_end,
                );
                if *has_trailing_newline && buffer_end == base_text_byte_range.end + 1 {
                    suffix.add_assign(&<MBD::TextDimension>::from_text_summary(
                        &TextSummary::from("\n"),
                    ))
                }

                let mut result = MBD::default();
                result.add_text_dim(&suffix);
                result
            }
        };

        result += suffix;
        result
    }

    fn text_summary_for_excerpt_offset_range<MBD>(&self, mut range: Range<ExcerptOffset>) -> MBD
    where
        MBD: MultiBufferDimension + AddAssign,
    {
        let mut summary = MBD::default();
        let mut cursor = self.excerpts.cursor::<ExcerptOffset>(());
        cursor.seek(&range.start, Bias::Right);
        if let Some(excerpt) = cursor.item() {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            let mut end_before_newline = cursor.end();
            if excerpt.has_trailing_newline {
                end_before_newline -= 1;
            }

            let excerpt_start = excerpt.range.context.start.to_offset(&buffer_snapshot);
            let start_in_excerpt = excerpt_start + (range.start - *cursor.start());
            let end_in_excerpt =
                excerpt_start + (cmp::min(end_before_newline, range.end) - *cursor.start());
            summary.add_text_dim(
                &buffer_snapshot.text_summary_for_range::<MBD::TextDimension, _>(
                    start_in_excerpt..end_in_excerpt,
                ),
            );

            if range.end > end_before_newline {
                summary.add_mb_text_summary(&MBTextSummary::from(TextSummary::newline()));
            }

            cursor.next();
        }

        if range.end > *cursor.start() {
            summary += cursor
                .summary::<_, ExcerptDimension<MBD>>(&range.end, Bias::Right)
                .0;
            if let Some(excerpt) = cursor.item() {
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                range.end = cmp::max(*cursor.start(), range.end);

                let excerpt_start = excerpt.range.context.start.to_offset(&buffer_snapshot);
                let end_in_excerpt = excerpt_start + (range.end - *cursor.start());
                summary.add_text_dim(
                    &buffer_snapshot.text_summary_for_range::<MBD::TextDimension, _>(
                        excerpt_start..end_in_excerpt,
                    ),
                );
            }
        }

        summary
    }

    pub fn summary_for_anchor<MBD>(&self, anchor: &Anchor) -> MBD
    where
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + Sub<MBD::TextDimension, Output = MBD>
            + AddAssign<MBD::TextDimension>
            + Add<MBD::TextDimension, Output = MBD>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension> + Ord,
    {
        let target = anchor.seek_target(self);
        let anchor = match anchor {
            Anchor::Min => {
                return MBD::default();
            }
            Anchor::Excerpt(excerpt_anchor) => excerpt_anchor,
            Anchor::Max => {
                return MBD::from_summary(&self.text_summary());
            }
        };

        let (start, _, item) = self
            .excerpts
            .find::<ExcerptSummary, _>((), &target, Bias::Left);
        let start = MBD::from_summary(&start.text);

        let excerpt_start_position = ExcerptDimension(start);
        if self.diff_transforms.is_empty() {
            if let Some(excerpt) = item {
                if !excerpt.contains(anchor, self) {
                    return excerpt_start_position.0;
                }
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                let excerpt_buffer_start = excerpt
                    .range
                    .context
                    .start
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let excerpt_buffer_end = excerpt
                    .range
                    .context
                    .end
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let buffer_summary = anchor
                    .text_anchor()
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let summary = cmp::min(excerpt_buffer_end, buffer_summary);
                let mut position = excerpt_start_position;
                if summary > excerpt_buffer_start {
                    position += summary - excerpt_buffer_start;
                }

                position.0
            } else {
                excerpt_start_position.0
            }
        } else {
            let mut diff_transforms_cursor = self
                .diff_transforms
                .cursor::<Dimensions<ExcerptDimension<MBD>, OutputDimension<MBD>>>(());

            if let Some(excerpt) = item {
                if !excerpt.contains(anchor, self) {
                    diff_transforms_cursor.seek(&excerpt_start_position, Bias::Left);
                    return self.summary_for_excerpt_position_without_hunks(
                        Bias::Left,
                        excerpt_start_position,
                        &mut diff_transforms_cursor,
                    );
                }
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                let excerpt_buffer_start = excerpt
                    .range
                    .context
                    .start
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let excerpt_buffer_end = excerpt
                    .range
                    .context
                    .end
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let buffer_summary = anchor
                    .text_anchor()
                    .summary::<MBD::TextDimension>(&buffer_snapshot);
                let summary = cmp::min(excerpt_buffer_end, buffer_summary);
                let mut position = excerpt_start_position;
                if summary > excerpt_buffer_start {
                    position += summary - excerpt_buffer_start;
                }

                diff_transforms_cursor.seek(&position, Bias::Left);
                self.summary_for_anchor_with_excerpt_position(
                    *anchor,
                    position,
                    &mut diff_transforms_cursor,
                    &buffer_snapshot,
                )
            } else {
                diff_transforms_cursor.seek(&excerpt_start_position, Bias::Left);
                self.summary_for_excerpt_position_without_hunks(
                    Bias::Right,
                    excerpt_start_position,
                    &mut diff_transforms_cursor,
                )
            }
        }
    }

    /// Maps an anchor's excerpt-space position to its output-space position by
    /// walking the diff transforms. The cursor is shared across consecutive
    /// calls, so it may already be partway through the transform list.
    fn summary_for_anchor_with_excerpt_position<MBD>(
        &self,
        anchor: ExcerptAnchor,
        excerpt_position: ExcerptDimension<MBD>,
        diff_transforms: &mut Cursor<
            DiffTransform,
            Dimensions<ExcerptDimension<MBD>, OutputDimension<MBD>>,
        >,
        excerpt_buffer: &text::BufferSnapshot,
    ) -> MBD
    where
        MBD: MultiBufferDimension + Ord + Sub + AddAssign<<MBD as Sub>::Output>,
    {
        loop {
            let transform_end_position = diff_transforms.end().0;
            let item = diff_transforms.item();
            let at_transform_end = transform_end_position == excerpt_position && item.is_some();

            // A right-biased anchor at a transform boundary belongs to the
            // *next* transform, so advance past the current one.
            if anchor.text_anchor.bias == Bias::Right && at_transform_end {
                diff_transforms.next();
                continue;
            }

            let mut position = diff_transforms.start().1;
            match item {
                Some(DiffTransform::DeletedHunk {
                    buffer_id,
                    base_text_byte_range,
                    hunk_info,
                    ..
                }) => {
                    if let Some(diff_base_anchor) = anchor.diff_base_anchor
                        && let Some(base_text) =
                            self.diff_state(*buffer_id).map(|diff| diff.base_text())
                        && diff_base_anchor.is_valid(&base_text)
                    {
                        // The anchor carries a diff-base position — resolve it
                        // to a location inside the deleted hunk.
                        let base_text_offset = diff_base_anchor.to_offset(base_text);
                        if base_text_offset >= base_text_byte_range.start
                            && base_text_offset <= base_text_byte_range.end
                        {
                            let position_in_hunk = base_text
                                .text_summary_for_range::<MBD::TextDimension, _>(
                                    base_text_byte_range.start..base_text_offset,
                                );
                            position.0.add_text_dim(&position_in_hunk);
                        } else if at_transform_end {
                            // diff_base offset falls outside this hunk's range;
                            // advance to see if the next transform is a better fit.
                            diff_transforms.next();
                            continue;
                        }
                    } else if at_transform_end
                        && anchor
                            .text_anchor()
                            .cmp(&hunk_info.hunk_start_anchor, excerpt_buffer)
                            .is_gt()
                    {
                        // The anchor has no (valid) diff-base position, so it
                        // belongs in the buffer content, not in the deleted
                        // hunk. However, after an edit deletes the text between
                        // the hunk boundary and this anchor, both resolve to
                        // the same excerpt_position—landing us here on the
                        // DeletedHunk left behind by the shared cursor. Use the
                        // CRDT ordering to detect that the anchor is strictly
                        // *past* the hunk boundary and skip to the following
                        // BufferContent.
                        diff_transforms.next();
                        continue;
                    }
                }
                _ => {
                    // On a BufferContent (or no transform). If the anchor
                    // carries a diff_base_anchor it needs a DeletedHunk, so
                    // advance to find one.
                    if at_transform_end && anchor.diff_base_anchor.is_some() {
                        diff_transforms.next();
                        continue;
                    }
                    let overshoot = excerpt_position - diff_transforms.start().0;
                    position += overshoot;
                }
            }

            return position.0;
        }
    }

    /// Like `resolve_summary_for_anchor` but optimized for min/max anchors.
    fn summary_for_excerpt_position_without_hunks<MBD>(
        &self,
        bias: Bias,
        excerpt_position: ExcerptDimension<MBD>,
        diff_transforms: &mut Cursor<
            DiffTransform,
            Dimensions<ExcerptDimension<MBD>, OutputDimension<MBD>>,
        >,
    ) -> MBD
    where
        MBD: MultiBufferDimension + Ord + Sub + AddAssign<<MBD as Sub>::Output>,
    {
        loop {
            let transform_end_position = diff_transforms.end().0;
            let item = diff_transforms.item();
            let at_transform_end = transform_end_position == excerpt_position && item.is_some();

            // A right-biased anchor at a transform boundary belongs to the
            // *next* transform, so advance past the current one.
            if bias == Bias::Right && at_transform_end {
                diff_transforms.next();
                continue;
            }

            let mut position = diff_transforms.start().1;
            if let Some(DiffTransform::BufferContent { .. }) | None = item {
                let overshoot = excerpt_position - diff_transforms.start().0;
                position += overshoot;
            }

            return position.0;
        }
    }

    fn excerpt_offset_for_anchor(&self, anchor: &Anchor) -> ExcerptOffset {
        let anchor = match anchor {
            Anchor::Min => return ExcerptOffset::default(),
            Anchor::Excerpt(excerpt_anchor) => excerpt_anchor,
            Anchor::Max => return self.excerpts.summary().len(),
        };
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        let target = anchor.seek_target(self);

        cursor.seek(&target, Bias::Left);

        let mut position = cursor.start().len();
        if let Some(excerpt) = cursor.item()
            && excerpt.contains(anchor, self)
        {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            let excerpt_buffer_start =
                buffer_snapshot.offset_for_anchor(&excerpt.range.context.start);
            let excerpt_buffer_end = buffer_snapshot.offset_for_anchor(&excerpt.range.context.end);
            let buffer_position = cmp::min(
                excerpt_buffer_end,
                buffer_snapshot.offset_for_anchor(&anchor.text_anchor()),
            );
            if buffer_position > excerpt_buffer_start {
                position += buffer_position - excerpt_buffer_start;
            }
        }
        position
    }

    pub fn summaries_for_anchors<'a, MBD, I>(&'a self, anchors: I) -> Vec<MBD>
    where
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + AddAssign<MBD::TextDimension>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension> + Ord,
        I: 'a + IntoIterator<Item = &'a Anchor>,
    {
        let mut summaries = Vec::new();
        self.summaries_for_anchors_cb(anchors, |summary| summaries.push(summary));
        summaries
    }

    pub fn summaries_for_anchors_cb<'a, MBD, I>(&'a self, anchors: I, mut cb: impl FnMut(MBD))
    where
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + AddAssign<MBD::TextDimension>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension> + Ord,
        I: 'a + IntoIterator<Item = &'a Anchor>,
    {
        let mut anchors = anchors.into_iter().peekable();
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        let mut diff_transforms_cursor = self
            .diff_transforms
            .cursor::<Dimensions<ExcerptDimension<MBD>, OutputDimension<MBD>>>(());
        diff_transforms_cursor.next();

        while let Some(anchor) = anchors.peek() {
            let target = anchor.seek_target(self);
            let excerpt_anchor = match anchor {
                Anchor::Min => {
                    cb(MBD::default());
                    anchors.next();
                    continue;
                }
                Anchor::Excerpt(excerpt_anchor) => excerpt_anchor,
                Anchor::Max => {
                    cb(MBD::from_summary(&self.text_summary()));
                    anchors.next();
                    continue;
                }
            };

            cursor.seek_forward(&target, Bias::Left);

            let excerpt_start_position = ExcerptDimension(MBD::from_summary(&cursor.start().text));
            if let Some(excerpt) = cursor.item() {
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                if !excerpt.contains(&excerpt_anchor, self) {
                    diff_transforms_cursor.seek_forward(&excerpt_start_position, Bias::Left);
                    let position = self.summary_for_excerpt_position_without_hunks(
                        Bias::Left,
                        excerpt_start_position,
                        &mut diff_transforms_cursor,
                    );
                    cb(position);
                    anchors.next();
                    continue;
                }
                let excerpt_buffer_start = excerpt
                    .range
                    .context
                    .start
                    .summary::<MBD::TextDimension>(buffer_snapshot);
                let excerpt_buffer_end = excerpt
                    .range
                    .context
                    .end
                    .summary::<MBD::TextDimension>(buffer_snapshot);
                for (buffer_summary, excerpt_anchor) in buffer_snapshot
                    .summaries_for_anchors_with_payload::<MBD::TextDimension, _, _>(
                        std::iter::from_fn(|| {
                            let excerpt_anchor = anchors.peek()?.excerpt_anchor()?;
                            if !excerpt.contains(&excerpt_anchor, self) {
                                return None;
                            }
                            anchors.next();
                            Some((excerpt_anchor.text_anchor(), excerpt_anchor))
                        }),
                    )
                {
                    let summary = cmp::min(excerpt_buffer_end, buffer_summary);
                    let mut position = excerpt_start_position;
                    if summary > excerpt_buffer_start {
                        position += summary - excerpt_buffer_start;
                    }

                    if diff_transforms_cursor.start().0 < position {
                        diff_transforms_cursor.seek_forward(&position, Bias::Left);
                    }

                    cb(self.summary_for_anchor_with_excerpt_position(
                        excerpt_anchor,
                        position,
                        &mut diff_transforms_cursor,
                        &buffer_snapshot,
                    ));
                }
            } else {
                diff_transforms_cursor.seek_forward(&excerpt_start_position, Bias::Left);
                let position = self.summary_for_excerpt_position_without_hunks(
                    Bias::Right,
                    excerpt_start_position,
                    &mut diff_transforms_cursor,
                );
                cb(position);
                anchors.next();
            }
        }
    }

    pub fn dimensions_from_points<'a, MBD>(
        &'a self,
        points: impl 'a + IntoIterator<Item = Point>,
    ) -> impl 'a + Iterator<Item = MBD>
    where
        MBD: MultiBufferDimension + Sub + AddAssign<<MBD as Sub>::Output>,
    {
        let mut cursor = self.cursor::<DimensionPair<Point, MBD>, Point>();
        cursor.seek(&DimensionPair {
            key: Point::default(),
            value: None,
        });
        let mut points = points.into_iter();
        iter::from_fn(move || {
            let point = points.next()?;

            cursor.seek_forward(&DimensionPair {
                key: point,
                value: None,
            });

            if let Some(region) = cursor.region() {
                let overshoot = point - region.range.start.key;
                let buffer_point = region.buffer_range.start + overshoot;
                let mut position = region.range.start.value.unwrap();
                position.add_text_dim(
                    &region
                        .buffer
                        .text_summary_for_range(region.buffer_range.start..buffer_point),
                );
                if point == region.range.end.key && region.has_trailing_newline {
                    position.add_mb_text_summary(&MBTextSummary::from(TextSummary::newline()));
                }
                Some(position)
            } else {
                Some(MBD::from_summary(&self.text_summary()))
            }
        })
    }

    pub fn excerpts_for_buffer(
        &self,
        buffer_id: BufferId,
    ) -> impl Iterator<Item = ExcerptRange<text::Anchor>> {
        if let Some(buffer_state) = self.buffers.get(&buffer_id) {
            let path_key = buffer_state.path_key.clone();
            let mut cursor = self.excerpts.cursor::<PathKey>(());
            cursor.seek_forward(&path_key, Bias::Left);
            Some(iter::from_fn(move || {
                let excerpt = cursor.item()?;
                if excerpt.path_key != path_key {
                    return None;
                }
                cursor.next();
                Some(excerpt.range.clone())
            }))
        } else {
            None
        }
        .into_iter()
        .flatten()
    }

    pub fn anchor_before<T: ToOffset>(&self, position: T) -> Anchor {
        self.anchor_at(position, Bias::Left)
    }

    pub fn anchor_after<T: ToOffset>(&self, position: T) -> Anchor {
        self.anchor_at(position, Bias::Right)
    }

    pub fn anchor_at<T: ToOffset>(&self, position: T, mut bias: Bias) -> Anchor {
        let offset = position.to_offset(self);

        // Find the given position in the diff transforms. Determine the corresponding
        // offset in the excerpts, and whether the position is within a deleted hunk.
        let mut diff_transforms = self
            .diff_transforms
            .cursor::<Dimensions<MultiBufferOffset, ExcerptOffset>>(());
        diff_transforms.seek(&offset, Bias::Right);

        if offset == diff_transforms.start().0
            && bias == Bias::Left
            && let Some(prev_item) = diff_transforms.prev_item()
            && let DiffTransform::DeletedHunk { .. } = prev_item
        {
            diff_transforms.prev();
        }
        let offset_in_transform = offset - diff_transforms.start().0;
        let mut excerpt_offset = diff_transforms.start().1;
        let mut diff_base_anchor = None;
        if let Some(DiffTransform::DeletedHunk {
            buffer_id,
            base_text_byte_range,
            has_trailing_newline,
            ..
        }) = diff_transforms.item()
        {
            let diff = self.diff_state(*buffer_id).expect("missing diff");
            if offset_in_transform > base_text_byte_range.len() {
                debug_assert!(*has_trailing_newline);
                bias = Bias::Right;
            } else {
                diff_base_anchor = Some(
                    diff.base_text()
                        .anchor_at(base_text_byte_range.start + offset_in_transform, bias),
                );
                bias = Bias::Left;
            }
        } else {
            excerpt_offset += MultiBufferOffset(offset_in_transform);
        };

        let mut excerpts = self
            .excerpts
            .cursor::<Dimensions<ExcerptOffset, ExcerptSummary>>(());
        excerpts.seek(&excerpt_offset, Bias::Right);
        if excerpts.item().is_none() && excerpt_offset == excerpts.start().0 && bias == Bias::Left {
            excerpts.prev();
        }
        if let Some(excerpt) = excerpts.item() {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            let mut overshoot = excerpt_offset.saturating_sub(excerpts.start().0);
            if excerpt.has_trailing_newline && excerpt_offset == excerpts.end().0 {
                overshoot -= 1;
                bias = Bias::Right;
            }

            let buffer_start = excerpt.range.context.start.to_offset(&buffer_snapshot);
            let text_anchor = excerpt.clip_anchor(
                buffer_snapshot.anchor_at(buffer_start + overshoot, bias),
                self,
            );
            let anchor = ExcerptAnchor::in_buffer(excerpt.path_key_index, text_anchor);
            let anchor = match diff_base_anchor {
                Some(diff_base_anchor) => anchor.with_diff_base_anchor(diff_base_anchor),
                None => anchor,
            };
            anchor.into()
        } else if excerpt_offset == ExcerptDimension(MultiBufferOffset::ZERO) && bias == Bias::Left
        {
            Anchor::Min
        } else {
            Anchor::Max
        }
    }

    /// Lifts a buffer anchor to a multibuffer anchor without checking against excerpt boundaries. Returns `None` if there are no excerpts for the buffer
    pub fn anchor_in_buffer(&self, anchor: text::Anchor) -> Option<Anchor> {
        let path_key_index = self.path_key_index_for_buffer(anchor.buffer_id)?;
        Some(Anchor::in_buffer(path_key_index, anchor))
    }

    /// Lifts a buffer anchor range to a multibuffer anchor range without checking against excerpt boundaries. Returns `None` if there are no excerpts for the buffer.
    pub fn anchor_range_in_buffer(&self, range: Range<text::Anchor>) -> Option<Range<Anchor>> {
        if range.start.buffer_id != range.end.buffer_id {
            return None;
        }

        let path_key_index = self.path_key_index_for_buffer(range.start.buffer_id)?;
        Some(Anchor::range_in_buffer(path_key_index, range))
    }

    /// Creates a multibuffer anchor for the given buffer anchor, if it is contained in any excerpt.
    pub fn anchor_in_excerpt(&self, text_anchor: text::Anchor) -> Option<Anchor> {
        let excerpts = {
            let buffer_id = text_anchor.buffer_id;
            if let Some(buffer_state) = self.buffers.get(&buffer_id) {
                let path_key = buffer_state.path_key.clone();
                let mut cursor = self.excerpts.cursor::<PathKey>(());
                cursor.seek_forward(&path_key, Bias::Left);
                Some(iter::from_fn(move || {
                    let excerpt = cursor.item()?;
                    if excerpt.path_key != path_key {
                        return None;
                    }
                    cursor.next();
                    Some(excerpt)
                }))
            } else {
                None
            }
            .into_iter()
            .flatten()
        };
        for excerpt in excerpts {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            if excerpt.range.contains(&text_anchor, &buffer_snapshot) {
                return Some(Anchor::in_buffer(excerpt.path_key_index, text_anchor));
            }
        }

        None
    }

    /// Creates a multibuffer anchor for the given buffer anchor, if it is contained in any excerpt.
    pub fn buffer_anchor_range_to_anchor_range(
        &self,
        text_anchor: Range<text::Anchor>,
    ) -> Option<Range<Anchor>> {
        if self.is_singleton() {
            let excerpt = self.excerpts.first()?;
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            if excerpt.range.contains(&text_anchor.start, &buffer_snapshot)
                && excerpt.range.contains(&text_anchor.end, &buffer_snapshot)
            {
                return Some(Anchor::range_in_buffer(excerpt.path_key_index, text_anchor));
            }
        }

        // for each search match

        let mut buffer_snapshot = None;
        for excerpt in {
            let this = &self;
            let buffer_id = text_anchor.start.buffer_id;
            if let Some(buffer_state) = this.buffers.get(&buffer_id) {
                let path_key = buffer_state.path_key.clone();
                let mut cursor = this.excerpts.cursor::<PathKey>(());
                cursor.seek_forward(&path_key, Bias::Left);
                Some(iter::from_fn(move || {
                    let excerpt = cursor.item()?;
                    if excerpt.path_key != path_key {
                        return None;
                    }
                    cursor.next();
                    Some(excerpt)
                }))
            } else {
                None
            }
            .into_iter()
            .flatten()
        } {
            let buffer_snapshot =
                buffer_snapshot.get_or_insert_with(|| excerpt.buffer_snapshot(self));
            if excerpt.range.contains(&text_anchor.start, &buffer_snapshot)
                && excerpt.range.contains(&text_anchor.end, &buffer_snapshot)
            {
                return Some(Anchor::range_in_buffer(excerpt.path_key_index, text_anchor));
            }
        }

        None
    }

    /// Returns a buffer anchor and its buffer snapshot for the given anchor, if it is in the multibuffer.
    pub fn anchor_to_buffer_anchor(
        &self,
        anchor: Anchor,
    ) -> Option<(text::Anchor, &BufferSnapshot)> {
        match anchor {
            Anchor::Min => {
                let excerpt = self.excerpts.first()?;
                let buffer = excerpt.buffer_snapshot(self);
                Some((excerpt.range.context.start, buffer))
            }
            Anchor::Excerpt(excerpt_anchor) => {
                let buffer = self.buffer_for_id(excerpt_anchor.buffer_id())?;
                Some((excerpt_anchor.text_anchor, buffer))
            }
            Anchor::Max => {
                let excerpt = self.excerpts.last()?;
                let buffer = excerpt.buffer_snapshot(self);
                Some((excerpt.range.context.end, buffer))
            }
        }
    }

    pub fn can_resolve(&self, anchor: &Anchor) -> bool {
        match anchor {
            // todo(lw): should be `!self.is_empty()`
            Anchor::Min | Anchor::Max => true,
            Anchor::Excerpt(excerpt_anchor) => {
                let Some(target) = excerpt_anchor.try_seek_target(self) else {
                    return false;
                };
                let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
                cursor.seek(&target, Bias::Left);
                let Some(excerpt) = cursor.item() else {
                    return false;
                };
                excerpt
                    .buffer_snapshot(self)
                    .can_resolve(&excerpt_anchor.text_anchor())
            }
        }
    }

    pub fn excerpts(&self) -> impl Iterator<Item = ExcerptRange<text::Anchor>> {
        self.excerpts.iter().map(|excerpt| excerpt.range.clone())
    }

    fn cursor<'a, MBD, BD>(&'a self) -> MultiBufferCursor<'a, MBD, BD>
    where
        MBD: MultiBufferDimension + Ord + Sub + ops::AddAssign<<MBD as Sub>::Output>,
        BD: TextDimension + AddAssign<<MBD as Sub>::Output>,
    {
        let excerpts = self.excerpts.cursor(());
        let diff_transforms = self.diff_transforms.cursor(());
        MultiBufferCursor {
            excerpts,
            diff_transforms,
            cached_region: OnceCell::new(),
            snapshot: self,
        }
    }

    pub fn excerpt_before(&self, anchor: Anchor) -> Option<ExcerptRange<text::Anchor>> {
        let target = anchor.try_seek_target(&self)?;
        let mut excerpts = self.excerpts.cursor::<ExcerptSummary>(());
        excerpts.seek(&target, Bias::Left);
        excerpts.prev();
        Some(excerpts.item()?.range.clone())
    }

    pub fn excerpt_boundaries_in_range<R, T>(
        &self,
        range: R,
    ) -> impl Iterator<Item = ExcerptBoundary> + '_
    where
        R: RangeBounds<T>,
        T: ToOffset,
    {
        let start_offset;
        let start = match range.start_bound() {
            Bound::Included(start) => {
                start_offset = start.to_offset(self);
                Bound::Included(start_offset)
            }
            Bound::Excluded(_) => {
                panic!("not supported")
            }
            Bound::Unbounded => {
                start_offset = MultiBufferOffset::ZERO;
                Bound::Unbounded
            }
        };
        let end = match range.end_bound() {
            Bound::Included(end) => Bound::Included(end.to_offset(self)),
            Bound::Excluded(end) => Bound::Excluded(end.to_offset(self)),
            Bound::Unbounded => Bound::Unbounded,
        };
        let bounds = (start, end);
        let mut cursor = self.cursor::<DimensionPair<MultiBufferOffset, Point>, BufferOffset>();
        cursor.seek(&DimensionPair {
            key: start_offset,
            value: None,
        });

        if cursor
            .fetch_excerpt_with_range()
            .is_some_and(|(_, range)| bounds.contains(&range.start.key))
        {
            cursor.prev_excerpt();
        } else {
            cursor.seek_to_start_of_current_excerpt();
        }
        let mut prev_excerpt = cursor
            .fetch_excerpt_with_range()
            .map(|(excerpt, _)| excerpt);

        cursor.next_excerpt_forwards();

        iter::from_fn(move || {
            loop {
                if self.singleton {
                    return None;
                }

                let (next_excerpt, next_range) = cursor.fetch_excerpt_with_range()?;
                cursor.next_excerpt_forwards();
                if !bounds.contains(&next_range.start.key) {
                    prev_excerpt = Some(next_excerpt);
                    continue;
                }

                let next_region_start = next_range.start.value.unwrap();
                let next_region_end = if let Some((_, range)) = cursor.fetch_excerpt_with_range() {
                    range.start.value.unwrap()
                } else {
                    self.max_point()
                };

                let prev = prev_excerpt.as_ref().map(|excerpt| ExcerptBoundaryInfo {
                    start_anchor: Anchor::in_buffer(
                        excerpt.path_key_index,
                        excerpt.range.context.start,
                    ),
                    range: excerpt.range.clone(),
                    end_row: MultiBufferRow(next_region_start.row),
                });

                let next = ExcerptBoundaryInfo {
                    start_anchor: Anchor::in_buffer(
                        next_excerpt.path_key_index,
                        next_excerpt.range.context.start,
                    ),
                    range: next_excerpt.range.clone(),
                    end_row: if next_excerpt.has_trailing_newline {
                        MultiBufferRow(next_region_end.row - 1)
                    } else {
                        MultiBufferRow(next_region_end.row)
                    },
                };

                let row = MultiBufferRow(next_region_start.row);

                prev_excerpt = Some(next_excerpt);

                return Some(ExcerptBoundary { row, prev, next });
            }
        })
    }

    pub fn edit_count(&self) -> usize {
        self.edit_count
    }

    pub fn non_text_state_update_count(&self) -> usize {
        self.non_text_state_update_count
    }

    /// Allows converting several ranges within the same excerpt between buffer offsets and multibuffer offsets.
    ///
    /// If the input range is contained in a single excerpt, invokes the callback with the full range of that excerpt
    /// and the input range both converted to buffer coordinates. The buffer ranges returned by the callback are lifted back
    /// to multibuffer offsets and returned.
    ///
    /// Returns `None` if the input range spans multiple excerpts.
    pub fn map_excerpt_ranges<'a, T>(
        &'a self,
        position: Range<MultiBufferOffset>,
        f: impl FnOnce(
            &'a BufferSnapshot,
            ExcerptRange<BufferOffset>,
            Range<BufferOffset>,
        ) -> Vec<(Range<BufferOffset>, T)>,
    ) -> Option<Vec<(Range<MultiBufferOffset>, T)>> {
        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        cursor.seek(&position.start);

        let region = cursor.region()?;
        if !region.is_main_buffer {
            return None;
        }
        let excerpt = cursor.excerpt()?;
        let excerpt_start = *cursor.excerpts.start();
        let input_buffer_start = cursor.buffer_position_at(&position.start)?;

        cursor.seek_forward(&position.end);
        if cursor.excerpt()? != excerpt {
            return None;
        }
        let region = cursor.region()?;
        if !region.is_main_buffer {
            return None;
        }
        let input_buffer_end = cursor.buffer_position_at(&position.end)?;
        let input_buffer_range = input_buffer_start..input_buffer_end;
        let buffer = excerpt.buffer_snapshot(self);
        let excerpt_context_range = excerpt.range.context.to_offset(buffer);
        let excerpt_context_range =
            BufferOffset(excerpt_context_range.start)..BufferOffset(excerpt_context_range.end);
        let excerpt_primary_range = excerpt.range.primary.to_offset(buffer);
        let excerpt_primary_range =
            BufferOffset(excerpt_primary_range.start)..BufferOffset(excerpt_primary_range.end);
        let results = f(
            buffer,
            ExcerptRange {
                context: excerpt_context_range.clone(),
                primary: excerpt_primary_range,
            },
            input_buffer_range,
        );
        let mut diff_transforms = cursor.diff_transforms;
        Some(
            results
                .into_iter()
                .map(|(buffer_range, metadata)| {
                    let clamped_start = buffer_range
                        .start
                        .max(excerpt_context_range.start)
                        .min(excerpt_context_range.end);
                    let clamped_end = buffer_range
                        .end
                        .max(clamped_start)
                        .min(excerpt_context_range.end);
                    let excerpt_offset_start =
                        excerpt_start + (clamped_start.0 - excerpt_context_range.start.0);
                    let excerpt_offset_end =
                        excerpt_start + (clamped_end.0 - excerpt_context_range.start.0);

                    diff_transforms.seek(&excerpt_offset_start, Bias::Right);
                    let mut output_start = diff_transforms.start().output_dimension;
                    output_start +=
                        excerpt_offset_start - diff_transforms.start().excerpt_dimension;

                    diff_transforms.seek_forward(&excerpt_offset_end, Bias::Right);
                    let mut output_end = diff_transforms.start().output_dimension;
                    output_end += excerpt_offset_end - diff_transforms.start().excerpt_dimension;

                    (output_start.0..output_end.0, metadata)
                })
                .collect(),
        )
    }

    /// Returns the smallest enclosing bracket ranges containing the given range or
    /// None if no brackets contain range or the range is not contained in a single
    /// excerpt
    ///
    /// Can optionally pass a range_filter to filter the ranges of brackets to consider
    #[ztracing::instrument(skip_all)]
    pub fn innermost_enclosing_bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
        range_filter: Option<
            &dyn Fn(&BufferSnapshot, Range<BufferOffset>, Range<BufferOffset>) -> bool,
        >,
    ) -> Option<(Range<MultiBufferOffset>, Range<MultiBufferOffset>)> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let results =
            self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
                let filter = |open: Range<usize>, close: Range<usize>| -> bool {
                    excerpt_range.context.start.0 <= open.start
                        && close.end <= excerpt_range.context.end.0
                        && range_filter.is_none_or(|filter| {
                            filter(
                                buffer,
                                BufferOffset(open.start)..BufferOffset(close.end),
                                BufferOffset(close.start)..BufferOffset(close.end),
                            )
                        })
                };
                let Some((open, close)) =
                    buffer.innermost_enclosing_bracket_ranges(input_buffer_range, Some(&filter))
                else {
                    return Vec::new();
                };
                vec![
                    (BufferOffset(open.start)..BufferOffset(open.end), ()),
                    (BufferOffset(close.start)..BufferOffset(close.end), ()),
                ]
            })?;
        let [(open, _), (close, _)] = results.try_into().ok()?;
        Some((open, close))
    }

    /// Returns enclosing bracket ranges containing the given range or returns None if the range is
    /// not contained in a single excerpt
    pub fn enclosing_bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Option<impl Iterator<Item = (Range<MultiBufferOffset>, Range<MultiBufferOffset>)>> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let results =
            self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
                buffer
                    .enclosing_bracket_ranges(input_buffer_range)
                    .filter(|pair| {
                        excerpt_range.context.start.0 <= pair.open_range.start
                            && pair.close_range.end <= excerpt_range.context.end.0
                    })
                    .flat_map(|pair| {
                        [
                            (
                                BufferOffset(pair.open_range.start)
                                    ..BufferOffset(pair.open_range.end),
                                (),
                            ),
                            (
                                BufferOffset(pair.close_range.start)
                                    ..BufferOffset(pair.close_range.end),
                                (),
                            ),
                        ]
                    })
                    .collect()
            })?;
        Some(results.into_iter().map(|(range, _)| range).tuples())
    }

    /// Returns enclosing bracket ranges containing the given range or returns None if the range is
    /// not contained in a single excerpt
    pub fn text_object_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
        options: TreeSitterOptions,
    ) -> impl Iterator<Item = (Range<MultiBufferOffset>, TextObject)> + '_ {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
            buffer
                .text_object_ranges(input_buffer_range, options)
                .filter(|(range, _)| {
                    excerpt_range.context.start.0 <= range.start
                        && range.end <= excerpt_range.context.end.0
                })
                .map(|(range, text_object)| {
                    (
                        BufferOffset(range.start)..BufferOffset(range.end),
                        text_object,
                    )
                })
                .collect()
        })
        .into_iter()
        .flatten()
    }

    pub fn bracket_ranges<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Option<impl Iterator<Item = (Range<MultiBufferOffset>, Range<MultiBufferOffset>)>> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let results =
            self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
                buffer
                    .bracket_ranges(input_buffer_range)
                    .filter(|pair| {
                        excerpt_range.context.start.0 <= pair.open_range.start
                            && pair.close_range.end <= excerpt_range.context.end.0
                    })
                    .flat_map(|pair| {
                        [
                            (
                                BufferOffset(pair.open_range.start)
                                    ..BufferOffset(pair.open_range.end),
                                (),
                            ),
                            (
                                BufferOffset(pair.close_range.start)
                                    ..BufferOffset(pair.close_range.end),
                                (),
                            ),
                        ]
                    })
                    .collect()
            })?;
        Some(results.into_iter().map(|(range, _)| range).tuples())
    }

    pub fn redacted_ranges<'a, T: ToOffset>(
        &'a self,
        range: Range<T>,
        redaction_enabled: impl Fn(Option<&Arc<dyn File>>) -> bool + 'a,
    ) -> impl Iterator<Item = Range<MultiBufferOffset>> + 'a {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        self.lift_buffer_metadata(range, move |buffer, range| {
            if redaction_enabled(buffer.file()) {
                Some(buffer.redacted_ranges(range).map(|range| (range, ())))
            } else {
                None
            }
        })
        .map(|(range, _, _)| range)
    }

    pub fn runnable_ranges(
        &self,
        range: Range<Anchor>,
    ) -> impl Iterator<Item = (Range<Anchor>, language::RunnableRange)> + '_ {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        self.lift_buffer_metadata(range, move |buffer, range| {
            Some(
                buffer
                    .runnable_ranges(range.clone())
                    .filter(move |runnable| {
                        runnable.run_range.start >= range.start
                            && runnable.run_range.end < range.end
                    })
                    .map(|runnable| (runnable.run_range.clone(), runnable)),
            )
        })
        .map(|(run_range, runnable, _)| {
            (
                self.anchor_after(run_range.start)..self.anchor_before(run_range.end),
                runnable,
            )
        })
    }

    pub fn line_indents(
        &self,
        start_row: MultiBufferRow,
        buffer_filter: impl Fn(&BufferSnapshot) -> bool,
    ) -> impl Iterator<Item = (MultiBufferRow, LineIndent, &BufferSnapshot)> {
        let max_point = self.max_point();
        let mut cursor = self.cursor::<Point, Point>();
        cursor.seek(&Point::new(start_row.0, 0));
        iter::from_fn(move || {
            let mut region = cursor.region()?;
            while !buffer_filter(&region.excerpt.buffer_snapshot(self)) {
                cursor.next();
                region = cursor.region()?;
            }
            let region = cursor.region()?;
            let overshoot = start_row.0.saturating_sub(region.range.start.row);
            let buffer_start_row =
                (region.buffer_range.start.row + overshoot).min(region.buffer_range.end.row);

            let buffer_end_row = if region.is_main_buffer
                && (region.has_trailing_newline || region.range.end == max_point)
            {
                region.buffer_range.end.row
            } else {
                region.buffer_range.end.row.saturating_sub(1)
            };

            let line_indents = region
                .buffer
                .line_indents_in_row_range(buffer_start_row..buffer_end_row);
            let region_buffer_row = region.buffer_range.start.row;
            let region_row = region.range.start.row;
            let region_buffer = region.excerpt.buffer_snapshot(self);
            cursor.next();
            Some(line_indents.map(move |(buffer_row, indent)| {
                let row = region_row + (buffer_row - region_buffer_row);
                (MultiBufferRow(row), indent, region_buffer)
            }))
        })
        .flatten()
    }

    pub fn reversed_line_indents(
        &self,
        end_row: MultiBufferRow,
        buffer_filter: impl Fn(&BufferSnapshot) -> bool,
    ) -> impl Iterator<Item = (MultiBufferRow, LineIndent, &BufferSnapshot)> {
        let max_point = self.max_point();
        let mut cursor = self.cursor::<Point, Point>();
        cursor.seek(&Point::new(end_row.0, 0));
        iter::from_fn(move || {
            let mut region = cursor.region()?;
            while !buffer_filter(&region.excerpt.buffer_snapshot(self)) {
                cursor.prev();
                region = cursor.region()?;
            }
            let region = cursor.region()?;

            let buffer_start_row = region.buffer_range.start.row;
            let buffer_end_row = if region.is_main_buffer
                && (region.has_trailing_newline || region.range.end == max_point)
            {
                region.buffer_range.end.row + 1
            } else {
                region.buffer_range.end.row
            };

            let overshoot = end_row.0 - region.range.start.row;
            let buffer_end_row =
                (region.buffer_range.start.row + overshoot + 1).min(buffer_end_row);

            let line_indents = region
                .buffer
                .reversed_line_indents_in_row_range(buffer_start_row..buffer_end_row);
            let region_buffer_row = region.buffer_range.start.row;
            let region_row = region.range.start.row;
            let region_buffer = region.excerpt.buffer_snapshot(self);
            cursor.prev();
            Some(line_indents.map(move |(buffer_row, indent)| {
                let row = region_row + (buffer_row - region_buffer_row);
                (MultiBufferRow(row), indent, region_buffer)
            }))
        })
        .flatten()
    }

    pub async fn enclosing_indent(
        &self,
        mut target_row: MultiBufferRow,
    ) -> Option<(Range<MultiBufferRow>, LineIndent)> {
        let max_row = MultiBufferRow(self.max_point().row);
        if target_row >= max_row {
            return None;
        }

        let mut target_indent = self.line_indent_for_row(target_row);

        // If the current row is at the start of an indented block, we want to return this
        // block as the enclosing indent.
        if !target_indent.is_line_empty() && target_row < max_row {
            let next_line_indent = self.line_indent_for_row(MultiBufferRow(target_row.0 + 1));
            if !next_line_indent.is_line_empty()
                && target_indent.raw_len() < next_line_indent.raw_len()
            {
                target_indent = next_line_indent;
                target_row.0 += 1;
            }
        }

        const SEARCH_ROW_LIMIT: u32 = 25000;
        const SEARCH_WHITESPACE_ROW_LIMIT: u32 = 2500;
        const YIELD_INTERVAL: u32 = 100;

        let mut accessed_row_counter = 0;

        // If there is a blank line at the current row, search for the next non indented lines
        if target_indent.is_line_empty() {
            let start = MultiBufferRow(target_row.0.saturating_sub(SEARCH_WHITESPACE_ROW_LIMIT));
            let end =
                MultiBufferRow((max_row.0 + 1).min(target_row.0 + SEARCH_WHITESPACE_ROW_LIMIT));

            let mut non_empty_line_above = None;
            for (row, indent, _) in self.reversed_line_indents(target_row, |_| true) {
                if row < start {
                    break;
                }
                accessed_row_counter += 1;
                if accessed_row_counter == YIELD_INTERVAL {
                    accessed_row_counter = 0;
                    yield_now().await;
                }
                if !indent.is_line_empty() {
                    non_empty_line_above = Some((row, indent));
                    break;
                }
            }

            let mut non_empty_line_below = None;
            for (row, indent, _) in self.line_indents(target_row, |_| true) {
                if row > end {
                    break;
                }
                accessed_row_counter += 1;
                if accessed_row_counter == YIELD_INTERVAL {
                    accessed_row_counter = 0;
                    yield_now().await;
                }
                if !indent.is_line_empty() {
                    non_empty_line_below = Some((row, indent));
                    break;
                }
            }

            let (row, indent) = match (non_empty_line_above, non_empty_line_below) {
                (Some((above_row, above_indent)), Some((below_row, below_indent))) => {
                    if above_indent.raw_len() >= below_indent.raw_len() {
                        (above_row, above_indent)
                    } else {
                        (below_row, below_indent)
                    }
                }
                (Some(above), None) => above,
                (None, Some(below)) => below,
                _ => return None,
            };

            target_indent = indent;
            target_row = row;
        }

        let start = MultiBufferRow(target_row.0.saturating_sub(SEARCH_ROW_LIMIT));
        let end = MultiBufferRow((max_row.0 + 1).min(target_row.0 + SEARCH_ROW_LIMIT));

        let mut start_indent = None;
        for (row, indent, _) in self.reversed_line_indents(target_row, |_| true) {
            if row < start {
                break;
            }
            accessed_row_counter += 1;
            if accessed_row_counter == YIELD_INTERVAL {
                accessed_row_counter = 0;
                yield_now().await;
            }
            if !indent.is_line_empty() && indent.raw_len() < target_indent.raw_len() {
                start_indent = Some((row, indent));
                break;
            }
        }
        let (start_row, start_indent_size) = start_indent?;

        let mut end_indent = (end, None);
        for (row, indent, _) in self.line_indents(target_row, |_| true) {
            if row > end {
                break;
            }
            accessed_row_counter += 1;
            if accessed_row_counter == YIELD_INTERVAL {
                accessed_row_counter = 0;
                yield_now().await;
            }
            if !indent.is_line_empty() && indent.raw_len() < target_indent.raw_len() {
                end_indent = (MultiBufferRow(row.0.saturating_sub(1)), Some(indent));
                break;
            }
        }
        let (end_row, end_indent_size) = end_indent;

        let indent = if let Some(end_indent_size) = end_indent_size {
            if start_indent_size.raw_len() > end_indent_size.raw_len() {
                start_indent_size
            } else {
                end_indent_size
            }
        } else {
            start_indent_size
        };

        Some((start_row..end_row, indent))
    }

    pub fn indent_guides_in_range<T: ToPoint>(
        &self,
        range: Range<T>,
        ignore_disabled_for_language: bool,
        cx: &App,
    ) -> impl Iterator<Item = IndentGuide> {
        let range = range.start.to_point(self)..range.end.to_point(self);
        let start_row = MultiBufferRow(range.start.row);
        let end_row = MultiBufferRow(range.end.row);

        let mut row_indents = self.line_indents(start_row, |buffer| {
            let settings = LanguageSettings::for_buffer_snapshot(buffer, None, cx);
            settings.indent_guides.enabled || ignore_disabled_for_language
        });

        let mut result = Vec::new();
        let mut indent_stack = SmallVec::<[IndentGuide; 8]>::new();

        let mut prev_settings = None;
        while let Some((first_row, mut line_indent, buffer)) = row_indents.next() {
            if first_row > end_row {
                break;
            }
            let current_depth = indent_stack.len() as u32;

            // Avoid retrieving the language settings repeatedly for every buffer row.
            if let Some((prev_buffer_id, _)) = &prev_settings
                && prev_buffer_id != &buffer.remote_id()
            {
                prev_settings.take();
            }
            let settings = &prev_settings
                .get_or_insert_with(|| {
                    (
                        buffer.remote_id(),
                        LanguageSettings::for_buffer_snapshot(buffer, None, cx),
                    )
                })
                .1;
            let tab_size = settings.tab_size.get();

            // When encountering empty, continue until found useful line indent
            // then add to the indent stack with the depth found
            let mut found_indent = false;
            let mut last_row = first_row;
            if line_indent.is_line_blank() {
                while !found_indent {
                    let Some((target_row, new_line_indent, _)) = row_indents.next() else {
                        break;
                    };
                    const TRAILING_ROW_SEARCH_LIMIT: u32 = 25;
                    if target_row > MultiBufferRow(end_row.0 + TRAILING_ROW_SEARCH_LIMIT) {
                        break;
                    }

                    if new_line_indent.is_line_blank() {
                        continue;
                    }
                    last_row = target_row.min(end_row);
                    line_indent = new_line_indent;
                    found_indent = true;
                    break;
                }
            } else {
                found_indent = true
            }

            let depth = if found_indent {
                line_indent.len(tab_size) / tab_size
            } else {
                0
            };

            match depth.cmp(&current_depth) {
                cmp::Ordering::Less => {
                    for _ in 0..(current_depth - depth) {
                        let mut indent = indent_stack.pop().unwrap();
                        if last_row != first_row {
                            // In this case, we landed on an empty row, had to seek forward,
                            // and discovered that the indent we where on is ending.
                            // This means that the last display row must
                            // be on line that ends this indent range, so we
                            // should display the range up to the first non-empty line
                            indent.end_row = MultiBufferRow(first_row.0.saturating_sub(1));
                        }

                        result.push(indent)
                    }
                }
                cmp::Ordering::Greater => {
                    for next_depth in current_depth..depth {
                        indent_stack.push(IndentGuide {
                            buffer_id: buffer.remote_id(),
                            start_row: first_row,
                            end_row: last_row,
                            depth: next_depth,
                            tab_size,
                            settings: settings.indent_guides.clone(),
                        });
                    }
                }
                _ => {}
            }

            for indent in indent_stack.iter_mut() {
                indent.end_row = last_row;
            }
        }

        result.extend(indent_stack);
        result.into_iter()
    }

    pub fn trailing_excerpt_update_count(&self) -> usize {
        self.trailing_excerpt_update_count
    }

    pub fn file_at<T: ToOffset>(&self, point: T) -> Option<&Arc<dyn File>> {
        self.point_to_buffer_offset(point)
            .and_then(|(buffer, _)| buffer.file())
    }

    pub fn language_at<T: ToOffset>(&self, offset: T) -> Option<&Arc<Language>> {
        self.point_to_buffer_offset(offset)
            .and_then(|(buffer, offset)| buffer.language_at(offset))
    }

    fn language_settings<'a>(&'a self, cx: &'a App) -> Cow<'a, LanguageSettings> {
        self.excerpts
            .first()
            .map(|excerpt| excerpt.buffer_snapshot(self))
            .map(|buffer| LanguageSettings::for_buffer_snapshot(buffer, None, cx))
            .unwrap_or_else(move || self.language_settings_at(MultiBufferOffset::ZERO, cx))
    }

    pub fn language_settings_at<'a, T: ToOffset>(
        &'a self,
        point: T,
        cx: &'a App,
    ) -> Cow<'a, LanguageSettings> {
        if let Some((buffer, offset)) = self.point_to_buffer_offset(point) {
            buffer.settings_at(offset, cx)
        } else {
            Cow::Borrowed(&AllLanguageSettings::get_global(cx).defaults)
        }
    }

    pub fn language_scope_at<T: ToOffset>(&self, point: T) -> Option<LanguageScope> {
        self.point_to_buffer_offset(point)
            .and_then(|(buffer, offset)| buffer.language_scope_at(offset))
    }

    pub fn char_classifier_at<T: ToOffset>(&self, point: T) -> CharClassifier {
        self.point_to_buffer_offset(point)
            .map(|(buffer, offset)| buffer.char_classifier_at(offset))
            .unwrap_or_default()
    }

    pub fn language_indent_size_at<T: ToOffset>(
        &self,
        position: T,
        cx: &App,
    ) -> Option<IndentSize> {
        let (buffer_snapshot, offset) = self.point_to_buffer_offset(position)?;
        Some(buffer_snapshot.language_indent_size_at(offset, cx))
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn has_deleted_file(&self) -> bool {
        self.has_deleted_file
    }

    pub fn has_conflict(&self) -> bool {
        self.has_conflict
    }

    pub fn has_diagnostics(&self) -> bool {
        self.excerpts
            .iter()
            .any(|excerpt| excerpt.buffer_snapshot(self).has_diagnostics())
    }

    pub fn diagnostic_group(
        &self,
        buffer_id: BufferId,
        group_id: usize,
    ) -> impl Iterator<Item = DiagnosticEntryRef<'_, Point>> + '_ {
        self.lift_buffer_metadata::<Point, _, _>(
            Point::zero()..self.max_point(),
            move |buffer, range| {
                if buffer.remote_id() != buffer_id {
                    return None;
                };
                Some(
                    buffer
                        .diagnostics_in_range(range, false)
                        .filter(move |diagnostic| diagnostic.diagnostic.group_id == group_id)
                        .map(move |DiagnosticEntryRef { diagnostic, range }| (range, diagnostic)),
                )
            },
        )
        .map(|(range, diagnostic, _)| DiagnosticEntryRef { diagnostic, range })
    }

    pub fn diagnostics_in_range<'a, MBD>(
        &'a self,
        range: Range<MBD>,
    ) -> impl Iterator<Item = DiagnosticEntryRef<'a, MBD>> + 'a
    where
        MBD::TextDimension: 'a
            + text::ToOffset
            + text::FromAnchor
            + Sub<Output = MBD::TextDimension>
            + fmt::Debug
            + ops::Add<Output = MBD::TextDimension>
            + ops::AddAssign
            + Ord,
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + ops::Add<MBD::TextDimension, Output = MBD>
            + ops::AddAssign<MBD::TextDimension>
            + 'a,
    {
        self.lift_buffer_metadata::<MBD, _, _>(range, move |buffer, buffer_range| {
            Some(
                buffer
                    .diagnostics_in_range(buffer_range.start..buffer_range.end, false)
                    .map(|entry| (entry.range, entry.diagnostic)),
            )
        })
        .map(|(range, diagnostic, _)| DiagnosticEntryRef { diagnostic, range })
    }

    pub fn diagnostics_with_buffer_ids_in_range<'a, MBD>(
        &'a self,
        range: Range<MBD>,
    ) -> impl Iterator<Item = (BufferId, DiagnosticEntryRef<'a, MBD>)> + 'a
    where
        MBD: MultiBufferDimension
            + Ord
            + Sub<Output = MBD::TextDimension>
            + ops::Add<MBD::TextDimension, Output = MBD>
            + ops::AddAssign<MBD::TextDimension>,
        MBD::TextDimension: Sub<Output = MBD::TextDimension>
            + ops::Add<Output = MBD::TextDimension>
            + text::ToOffset
            + text::FromAnchor
            + AddAssign<MBD::TextDimension>
            + Ord,
    {
        self.lift_buffer_metadata::<MBD, _, _>(range, move |buffer, buffer_range| {
            Some(
                buffer
                    .diagnostics_in_range(buffer_range.start..buffer_range.end, false)
                    .map(|entry| (entry.range, entry.diagnostic)),
            )
        })
        .map(|(range, diagnostic, excerpt)| {
            (
                excerpt.buffer_snapshot(self).remote_id(),
                DiagnosticEntryRef { diagnostic, range },
            )
        })
    }

    pub fn syntax_ancestor<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Option<(tree_sitter::Node<'_>, Range<MultiBufferOffset>)> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let results =
            self.map_excerpt_ranges(range, |buffer, excerpt_range, input_buffer_range| {
                let Some(node) = buffer.syntax_ancestor(input_buffer_range) else {
                    return vec![];
                };
                let node_range = node.byte_range();
                if excerpt_range.context.start.0 <= node_range.start
                    && node_range.end <= excerpt_range.context.end.0
                {
                    vec![(
                        BufferOffset(node_range.start)..BufferOffset(node_range.end),
                        node,
                    )]
                } else {
                    vec![]
                }
            })?;
        let (output_range, node) = results.into_iter().next()?;
        Some((node, output_range))
    }

    pub fn outline(&self, theme: Option<&SyntaxTheme>) -> Option<Outline<Anchor>> {
        let buffer_snapshot = self.as_singleton()?;
        let excerpt = self.excerpts.first()?;
        let path_key_index = excerpt.path_key_index;
        let outline = buffer_snapshot.outline(theme);
        Some(Outline::new(
            outline
                .items
                .into_iter()
                .map(|item| OutlineItem {
                    depth: item.depth,
                    range: Anchor::range_in_buffer(path_key_index, item.range),
                    selection_range: Anchor::range_in_buffer(path_key_index, item.selection_range),
                    source_range_for_text: Anchor::range_in_buffer(
                        path_key_index,
                        item.source_range_for_text,
                    ),
                    text: item.text,
                    highlight_ranges: item.highlight_ranges,
                    name_ranges: item.name_ranges,
                    body_range: item
                        .body_range
                        .map(|body_range| Anchor::range_in_buffer(path_key_index, body_range)),
                    annotation_range: item.annotation_range.map(|annotation_range| {
                        Anchor::range_in_buffer(path_key_index, annotation_range)
                    }),
                })
                .collect(),
        ))
    }

    pub fn symbols_containing<T: ToOffset>(
        &self,
        offset: T,
        theme: Option<&SyntaxTheme>,
    ) -> Option<(BufferId, Vec<OutlineItem<Anchor>>)> {
        let anchor = self.anchor_before(offset);
        let target = anchor.try_seek_target(&self)?;
        let (_, _, excerpt) = self.excerpts.find((), &target, Bias::Left);
        let excerpt = excerpt?;
        let buffer_snapshot = excerpt.buffer_snapshot(self);
        Some((
            buffer_snapshot.remote_id(),
            buffer_snapshot
                .symbols_containing(
                    anchor
                        .excerpt_anchor()
                        .map(|anchor| anchor.text_anchor())
                        .unwrap_or(text::Anchor::min_for_buffer(buffer_snapshot.remote_id())),
                    theme,
                )
                .into_iter()
                .flat_map(|item| {
                    Some(OutlineItem {
                        depth: item.depth,
                        selection_range: Anchor::range_in_buffer(
                            excerpt.path_key_index,
                            item.selection_range,
                        ),
                        source_range_for_text: Anchor::range_in_buffer(
                            excerpt.path_key_index,
                            item.source_range_for_text,
                        ),
                        range: Anchor::range_in_buffer(excerpt.path_key_index, item.range),
                        text: item.text,
                        highlight_ranges: item.highlight_ranges,
                        name_ranges: item.name_ranges,
                        body_range: item.body_range.map(|body_range| {
                            Anchor::range_in_buffer(excerpt.path_key_index, body_range)
                        }),
                        annotation_range: item.annotation_range.map(|body_range| {
                            Anchor::range_in_buffer(excerpt.path_key_index, body_range)
                        }),
                    })
                })
                .collect(),
        ))
    }

    pub fn path_for_buffer(&self, buffer_id: BufferId) -> Option<&PathKey> {
        Some(&self.buffers.get(&buffer_id)?.path_key)
    }

    pub(crate) fn path_key_index_for_buffer(&self, buffer_id: BufferId) -> Option<PathKeyIndex> {
        let snapshot = self.buffers.get(&buffer_id)?;
        Some(snapshot.path_key_index)
    }

    fn first_excerpt_for_buffer(&self, buffer_id: BufferId) -> Option<&Excerpt> {
        let path_key = &self.buffers.get(&buffer_id)?.path_key;
        self.first_excerpt_for_path(path_key)
    }

    fn first_excerpt_for_path(&self, path_key: &PathKey) -> Option<&Excerpt> {
        let (_, _, first_excerpt) = self.excerpts.find::<PathKey, _>((), path_key, Bias::Left);
        first_excerpt
    }

    pub fn buffer_for_id(&self, id: BufferId) -> Option<&BufferSnapshot> {
        self.buffers.get(&id).map(|state| &state.buffer_snapshot)
    }

    fn try_path_for_anchor(&self, anchor: ExcerptAnchor) -> Option<&PathKey> {
        self.path_keys.get_index(anchor.path.0 as usize)
    }

    pub fn path_for_anchor(&self, anchor: ExcerptAnchor) -> &PathKey {
        self.try_path_for_anchor(anchor)
            .expect("invalid anchor: path was never added to multibuffer")
    }

    /// Returns the excerpt containing range and its offset start within the multibuffer or none if `range` spans multiple excerpts
    pub fn excerpt_containing<T: ToOffset>(
        &self,
        range: Range<T>,
    ) -> Option<(&BufferSnapshot, ExcerptRange<text::Anchor>)> {
        let range = range.start.to_offset(self)..range.end.to_offset(self);
        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        cursor.seek(&range.start);

        let start_excerpt = cursor.excerpt()?;
        if range.end != range.start {
            cursor.seek_forward(&range.end);
            if cursor.excerpt()? != start_excerpt {
                return None;
            }
        }

        Some((
            start_excerpt.buffer_snapshot(self),
            start_excerpt.range.clone(),
        ))
    }

    pub fn selections_in_range<'a>(
        &'a self,
        range: &'a Range<Anchor>,
        include_local: bool,
    ) -> impl 'a + Iterator<Item = (ReplicaId, bool, CursorShape, Selection<Anchor>)> {
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        cursor.seek(&range.start.seek_target(self), Bias::Left);
        cursor
            .take_while(move |excerpt| {
                let excerpt_start =
                    Anchor::in_buffer(excerpt.path_key_index, excerpt.range.context.start);
                excerpt_start.cmp(&range.end, self).is_le()
            })
            .flat_map(move |excerpt| {
                let buffer_snapshot = excerpt.buffer_snapshot(self);
                let mut query_range = excerpt.range.context.start..excerpt.range.context.end;
                if let Some(excerpt_anchor) = range.start.excerpt_anchor()
                    && excerpt.contains(&excerpt_anchor, self)
                {
                    query_range.start = excerpt_anchor.text_anchor();
                }
                if let Some(excerpt_anchor) = range.end.excerpt_anchor()
                    && excerpt.contains(&excerpt_anchor, self)
                {
                    query_range.end = excerpt_anchor.text_anchor();
                }

                buffer_snapshot
                    .selections_in_range(query_range, include_local)
                    .flat_map(move |(replica_id, line_mode, cursor_shape, selections)| {
                        selections.map(move |selection| {
                            let mut start =
                                Anchor::in_buffer(excerpt.path_key_index, selection.start);
                            let mut end = Anchor::in_buffer(excerpt.path_key_index, selection.end);
                            if range.start.cmp(&start, self).is_gt() {
                                start = range.start;
                            }
                            if range.end.cmp(&end, self).is_lt() {
                                end = range.end;
                            }

                            (
                                replica_id,
                                line_mode,
                                cursor_shape,
                                Selection {
                                    id: selection.id,
                                    start,
                                    end,
                                    reversed: selection.reversed,
                                    goal: selection.goal,
                                },
                            )
                        })
                    })
            })
    }

    pub fn show_headers(&self) -> bool {
        self.show_headers
    }

    pub fn diff_for_buffer_id(&self, buffer_id: BufferId) -> Option<&BufferDiffSnapshot> {
        self.diff_state(buffer_id).map(|diff| &diff.diff)
    }

    fn diff_state(&self, buffer_id: BufferId) -> Option<&DiffStateSnapshot> {
        find_diff_state(&self.diffs, buffer_id)
    }

    pub fn total_changed_lines(&self) -> (u32, u32) {
        let summary = self.diffs.summary();
        (summary.added_rows, summary.removed_rows)
    }

    pub fn all_diff_hunks_expanded(&self) -> bool {
        self.all_diff_hunks_expanded
    }

    /// Visually annotates a position or range with the `Debug` representation of a value. The
    /// callsite of this function is used as a key - previous annotations will be removed.
    #[cfg(debug_assertions)]
    #[track_caller]
    pub fn debug<V, R>(&self, ranges: &R, value: V)
    where
        R: debug::ToMultiBufferDebugRanges,
        V: std::fmt::Debug,
    {
        self.debug_with_key(std::panic::Location::caller(), ranges, value);
    }

    /// Visually annotates a position or range with the `Debug` representation of a value. Previous
    /// debug annotations with the same key will be removed. The key is also used to determine the
    /// annotation's color.
    #[cfg(debug_assertions)]
    #[track_caller]
    pub fn debug_with_key<K, R, V>(&self, key: &K, ranges: &R, value: V)
    where
        K: std::hash::Hash + 'static,
        R: debug::ToMultiBufferDebugRanges,
        V: std::fmt::Debug,
    {
        let text_ranges = ranges
            .to_multi_buffer_debug_ranges(self)
            .into_iter()
            .flat_map(|range| {
                self.range_to_buffer_ranges(range)
                    .into_iter()
                    .map(|(buffer_snapshot, range, _)| {
                        buffer_snapshot.anchor_after(range.start)
                            ..buffer_snapshot.anchor_before(range.end)
                    })
            })
            .collect();
        text::debug::GlobalDebugRanges::with_locked(|debug_ranges| {
            debug_ranges.insert(key, text_ranges, format!("{value:?}").into())
        });
    }

    fn excerpt_edits_for_diff_change(
        &self,
        path: &PathKey,
        diff_change_range: Range<usize>,
    ) -> Vec<Edit<ExcerptDimension<MultiBufferOffset>>> {
        let mut excerpt_edits = Vec::new();
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        cursor.seek(path, Bias::Left);
        while let Some(excerpt) = cursor.item()
            && &excerpt.path_key == path
        {
            let buffer_snapshot = excerpt.buffer_snapshot(self);
            let excerpt_buffer_range = excerpt.range.context.to_offset(buffer_snapshot);
            let excerpt_start = cursor.start().clone();
            let excerpt_len = excerpt.text_summary.len;
            cursor.next();
            if diff_change_range.end < excerpt_buffer_range.start
                || diff_change_range.start > excerpt_buffer_range.end
            {
                continue;
            }
            let diff_change_start_in_excerpt = diff_change_range
                .start
                .saturating_sub(excerpt_buffer_range.start);
            let diff_change_end_in_excerpt = diff_change_range
                .end
                .saturating_sub(excerpt_buffer_range.start);
            let edit_start = excerpt_start.len() + diff_change_start_in_excerpt.min(excerpt_len);
            let edit_end = excerpt_start.len() + diff_change_end_in_excerpt.min(excerpt_len);
            excerpt_edits.push(Edit {
                old: edit_start..edit_end,
                new: edit_start..edit_end,
            });
        }
        excerpt_edits
    }

    fn excerpts_for_path<'a>(
        &'a self,
        path_key: &'a PathKey,
    ) -> impl Iterator<Item = ExcerptRange<text::Anchor>> + 'a {
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        cursor.seek(path_key, Bias::Left);
        cursor
            .take_while(move |item| &item.path_key == path_key)
            .map(|excerpt| excerpt.range.clone())
    }

    /// If the given multibuffer range is contained in a single excerpt and contains no deleted hunks,
    /// returns the corresponding buffer range.
    ///
    /// Otherwise, returns None.
    pub fn range_to_buffer_range<MBD>(
        &self,
        range: Range<MBD>,
    ) -> Option<(&BufferSnapshot, Range<MBD::TextDimension>)>
    where
        MBD: MultiBufferDimension + Ord + Sub + ops::AddAssign<<MBD as Sub>::Output>,
        MBD::TextDimension: AddAssign<<MBD as Sub>::Output>,
    {
        let mut cursor = self.cursor::<MBD, MBD::TextDimension>();
        cursor.seek(&range.start);

        let start_region = cursor.region()?.clone();

        while let Some(region) = cursor.region()
            && region.range.end < range.end
        {
            if !region.is_main_buffer {
                return None;
            }
            cursor.next();
        }

        let end_region = cursor.region()?;
        if end_region.buffer.remote_id() != start_region.buffer.remote_id() {
            return None;
        }

        let mut buffer_start = start_region.buffer_range.start;
        buffer_start += range.start - start_region.range.start;
        let mut buffer_end = end_region.buffer_range.start;
        buffer_end += range.end - end_region.range.start;

        Some((start_region.buffer, buffer_start..buffer_end))
    }

    /// If the two endpoints of the range lie in the same excerpt, return the corresponding
    /// buffer range. Intervening deleted hunks are allowed.
    pub fn anchor_range_to_buffer_anchor_range(
        &self,
        range: Range<Anchor>,
    ) -> Option<(&BufferSnapshot, Range<text::Anchor>)> {
        let mut cursor = self.excerpts.cursor::<ExcerptSummary>(());
        cursor.seek(&range.start.seek_target(&self), Bias::Left);

        let start_excerpt = cursor.item()?;

        let snapshot = start_excerpt.buffer_snapshot(&self);

        cursor.seek(&range.end.seek_target(&self), Bias::Left);

        let end_excerpt = cursor.item()?;

        if start_excerpt != end_excerpt {
            return None;
        }

        if let Anchor::Excerpt(excerpt_anchor) = range.start
            && (excerpt_anchor.path != start_excerpt.path_key_index
                || excerpt_anchor.buffer_id() != snapshot.remote_id())
        {
            return None;
        }
        if let Anchor::Excerpt(excerpt_anchor) = range.end
            && (excerpt_anchor.path != end_excerpt.path_key_index
                || excerpt_anchor.buffer_id() != snapshot.remote_id())
        {
            return None;
        }

        Some((
            snapshot,
            range.start.text_anchor_in(snapshot)..range.end.text_anchor_in(snapshot),
        ))
    }

    /// Returns all nonempty intersections of the given buffer range with excerpts in the multibuffer in order.
    ///
    /// The multibuffer ranges are split to not intersect deleted hunks.
    pub fn buffer_range_to_excerpt_ranges(
        &self,
        range: Range<text::Anchor>,
    ) -> impl Iterator<Item = Range<Anchor>> {
        assert!(range.start.buffer_id == range.end.buffer_id);

        let buffer_id = range.start.buffer_id;
        self.buffers
            .get(&buffer_id)
            .map(|buffer_state_snapshot| {
                let path_key_index = buffer_state_snapshot.path_key_index;
                let buffer_snapshot = &buffer_state_snapshot.buffer_snapshot;
                let buffer_range = range.to_offset(buffer_snapshot);

                let start = Anchor::in_buffer(path_key_index, range.start).to_offset(self);
                let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
                cursor.seek(&start);
                std::iter::from_fn(move || {
                    while let Some(region) = cursor.region()
                        && !region.is_main_buffer
                    {
                        cursor.next();
                    }

                    let region = cursor.region()?;
                    if region.buffer.remote_id() != buffer_id
                        || region.buffer_range.start > BufferOffset(buffer_range.end)
                    {
                        return None;
                    }

                    let start = region
                        .buffer_range
                        .start
                        .max(BufferOffset(buffer_range.start));
                    let mut end = region.buffer_range.end.min(BufferOffset(buffer_range.end));

                    cursor.next();
                    while let Some(region) = cursor.region()
                        && region.is_main_buffer
                        && region.buffer.remote_id() == buffer_id
                        && region.buffer_range.start <= end
                    {
                        end = end
                            .max(region.buffer_range.end)
                            .min(BufferOffset(buffer_range.end));
                        cursor.next();
                    }

                    let multibuffer_range = Anchor::range_in_buffer(
                        path_key_index,
                        buffer_snapshot.anchor_range_inside(start..end),
                    );
                    Some(multibuffer_range)
                })
            })
            .into_iter()
            .flatten()
    }

    pub fn buffers_with_paths<'a>(
        &'a self,
    ) -> impl 'a + Iterator<Item = (&'a BufferSnapshot, &'a PathKey)> {
        self.buffers
            .values()
            .map(|buffer| (&buffer.buffer_snapshot, &buffer.path_key))
    }

    /// Returns the number of graphemes in `range`.
    ///
    /// This counts user-visible characters like `e\u{301}` as one.
    pub fn grapheme_count_for_range(&self, range: &Range<MultiBufferOffset>) -> usize {
        self.text_for_range(range.clone())
            .collect::<String>()
            .graphemes(true)
            .count()
    }

    pub fn range_for_buffer(&self, buffer_id: BufferId) -> Option<Range<Point>> {
        let path_key = self.path_key_index_for_buffer(buffer_id)?;
        let start = Anchor::in_buffer(path_key, text::Anchor::min_for_buffer(buffer_id));
        let end = Anchor::in_buffer(path_key, text::Anchor::max_for_buffer(buffer_id));
        Some((start..end).to_point(self))
    }
}

#[cfg(any(test, feature = "test-support"))]
impl MultiBufferSnapshot {
    pub fn random_byte_range(
        &self,
        start_offset: MultiBufferOffset,
        rng: &mut impl rand::Rng,
    ) -> Range<MultiBufferOffset> {
        let end = self.clip_offset(rng.random_range(start_offset..=self.len()), Bias::Right);
        let start = self.clip_offset(rng.random_range(start_offset..=end), Bias::Right);
        start..end
    }

    #[cfg(any(test, feature = "test-support"))]
    fn check_invariants(&self) {
        let excerpts = self.excerpts.items(());

        let mut all_buffer_path_keys = HashSet::default();
        for buffer in self.buffers.values() {
            let path_key = buffer.path_key.clone();
            assert!(
                all_buffer_path_keys.insert(path_key),
                "path key reused for multiple buffers: {:#?}",
                self.buffers
            );
        }

        let all_excerpt_path_keys = HashSet::from_iter(excerpts.iter().map(|e| e.path_key.clone()));

        for (ix, excerpt) in excerpts.iter().enumerate() {
            if ix > 0 {
                let prev = &excerpts[ix - 1];

                if excerpt.path_key < prev.path_key {
                    panic!("excerpt path_keys are out-of-order: {:#?}", excerpts);
                } else if excerpt.path_key == prev.path_key {
                    assert_eq!(
                        excerpt.buffer_id, prev.buffer_id,
                        "excerpts with same path_key have different buffer_ids: {:#?}",
                        excerpts
                    );
                    if excerpt
                        .start_anchor()
                        .cmp(&prev.end_anchor(), &self)
                        .is_le()
                    {
                        panic!("excerpt anchors are out-of-order: {:#?}", excerpts);
                    }
                    if excerpt
                        .start_anchor()
                        .cmp(&excerpt.end_anchor(), &self)
                        .is_ge()
                    {
                        panic!("excerpt with backward range: {:#?}", excerpts);
                    }
                }
            }

            if ix < excerpts.len() - 1 {
                assert!(
                    excerpt.has_trailing_newline,
                    "non-trailing excerpt has no trailing newline: {:#?}",
                    excerpts
                );
            } else {
                assert!(
                    !excerpt.has_trailing_newline,
                    "trailing excerpt has trailing newline: {:#?}",
                    excerpts
                );
            }
            assert!(
                all_buffer_path_keys.contains(&excerpt.path_key),
                "excerpt path key not found in active path keys: {:#?}",
                excerpt.path_key
            );
            assert_eq!(
                self.path_keys.get_index(excerpt.path_key_index.0 as usize),
                Some(&excerpt.path_key),
                "excerpt path key index does not match path key: {:#?}",
                excerpt.path_key,
            );
        }
        assert_eq!(all_buffer_path_keys, all_excerpt_path_keys);

        if self.diff_transforms.summary().input != self.excerpts.summary().text {
            panic!(
                "incorrect input summary. expected {:?}, got {:?}. transforms: {:+?}",
                self.excerpts.summary().text,
                self.diff_transforms.summary().input,
                self.diff_transforms.items(()),
            );
        }

        let mut prev_transform: Option<&DiffTransform> = None;
        for item in self.diff_transforms.iter() {
            if let DiffTransform::BufferContent {
                summary,
                inserted_hunk_info,
            } = item
            {
                if let Some(DiffTransform::BufferContent {
                    inserted_hunk_info: prev_inserted_hunk_info,
                    ..
                }) = prev_transform
                    && *inserted_hunk_info == *prev_inserted_hunk_info
                {
                    panic!(
                        "multiple adjacent buffer content transforms with is_inserted_hunk = {inserted_hunk_info:?}. transforms: {:+?}",
                        self.diff_transforms.items(())
                    );
                }
                if summary.len == MultiBufferOffset(0) && !self.is_empty() {
                    panic!("empty buffer content transform");
                }
            }
            prev_transform = Some(item);
        }
    }
}

#[cfg(debug_assertions)]
pub mod debug {
    use super::*;

    pub trait ToMultiBufferDebugRanges {
        fn to_multi_buffer_debug_ranges(
            &self,
            snapshot: &MultiBufferSnapshot,
        ) -> Vec<Range<MultiBufferOffset>>;
    }

    impl<T: ToOffset> ToMultiBufferDebugRanges for T {
        fn to_multi_buffer_debug_ranges(
            &self,
            snapshot: &MultiBufferSnapshot,
        ) -> Vec<Range<MultiBufferOffset>> {
            [self.to_offset(snapshot)].to_multi_buffer_debug_ranges(snapshot)
        }
    }

    impl<T: ToOffset> ToMultiBufferDebugRanges for Range<T> {
        fn to_multi_buffer_debug_ranges(
            &self,
            snapshot: &MultiBufferSnapshot,
        ) -> Vec<Range<MultiBufferOffset>> {
            [self.start.to_offset(snapshot)..self.end.to_offset(snapshot)]
                .to_multi_buffer_debug_ranges(snapshot)
        }
    }

    impl<T: ToOffset> ToMultiBufferDebugRanges for Vec<T> {
        fn to_multi_buffer_debug_ranges(
            &self,
            snapshot: &MultiBufferSnapshot,
        ) -> Vec<Range<MultiBufferOffset>> {
            self.as_slice().to_multi_buffer_debug_ranges(snapshot)
        }
    }

    impl<T: ToOffset> ToMultiBufferDebugRanges for Vec<Range<T>> {
        fn to_multi_buffer_debug_ranges(
            &self,
            snapshot: &MultiBufferSnapshot,
        ) -> Vec<Range<MultiBufferOffset>> {
            self.as_slice().to_multi_buffer_debug_ranges(snapshot)
        }
    }

    impl<T: ToOffset> ToMultiBufferDebugRanges for [T] {
        fn to_multi_buffer_debug_ranges(
            &self,
            snapshot: &MultiBufferSnapshot,
        ) -> Vec<Range<MultiBufferOffset>> {
            self.iter()
                .map(|item| {
                    let offset = item.to_offset(snapshot);
                    offset..offset
                })
                .collect()
        }
    }

    impl<T: ToOffset> ToMultiBufferDebugRanges for [Range<T>] {
        fn to_multi_buffer_debug_ranges(
            &self,
            snapshot: &MultiBufferSnapshot,
        ) -> Vec<Range<MultiBufferOffset>> {
            self.iter()
                .map(|range| range.start.to_offset(snapshot)..range.end.to_offset(snapshot))
                .collect()
        }
    }
}
