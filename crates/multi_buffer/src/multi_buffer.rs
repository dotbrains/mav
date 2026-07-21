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
mod multibuffer_diff_hunks;
mod multibuffer_selection_state;
mod multibuffer_sync;
mod multibuffer_transform_sync;
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
mod snapshot_words;
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

impl MultiBuffer {}

impl EventEmitter<Event> for MultiBuffer {}

impl MultiBufferSnapshot {
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
