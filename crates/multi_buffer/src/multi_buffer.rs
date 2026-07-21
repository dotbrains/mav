mod anchor;
mod byte_iterators;
mod chunk_iterators;
mod cursor;
mod diff_state;
mod diff_transform;
mod dimensions;
mod edit_operations;
mod edit_state;
mod events;
mod excerpt_ranges;
mod excerpt_summary;
mod expansion;
mod lifecycle;
#[cfg(test)]
mod multi_buffer_tests;
mod multibuffer_buffer_metadata;
mod multibuffer_selection_state;
mod path_key;
mod row_info;
mod row_iterators;
mod snapshot_anchor_mapping;
mod snapshot_anchor_summary;
mod snapshot_diff_hunks;
mod snapshot_diff_state;
mod snapshot_dimensions;
mod snapshot_excerpt_boundaries;
mod snapshot_indent_guides;
mod snapshot_language;
mod snapshot_lines;
mod snapshot_paths;
mod snapshot_range_conversions;
mod snapshot_ranges;
mod snapshot_syntax_ranges;
mod snapshot_text;
mod snapshot_text_ranges;
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

    pub fn trailing_excerpt_update_count(&self) -> usize {
        self.trailing_excerpt_update_count
    }

    pub fn show_headers(&self) -> bool {
        self.show_headers
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
