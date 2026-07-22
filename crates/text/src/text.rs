mod anchor;
#[path = "text/buffer_core.rs"]
mod buffer_core;
#[path = "text/buffer_history.rs"]
mod buffer_history;
#[path = "text/buffer_remote.rs"]
mod buffer_remote;
#[path = "text/debug_ranges.rs"]
mod debug_ranges;
#[path = "text/fragment_builders.rs"]
mod fragment_builders;
#[path = "text/fragment_insertions.rs"]
mod fragment_insertions;
#[path = "text/fragment_summaries.rs"]
mod fragment_summaries;
mod history;
mod line_ending;
mod line_indent;
pub mod locator;
#[cfg(any(test, feature = "test-support"))]
pub mod network;
pub mod operation_queue;
#[path = "text/operation_traits.rs"]
mod operation_traits;
mod patch;
mod selection;
#[path = "text/snapshot_access.rs"]
mod snapshot_access;
#[path = "text/snapshot_anchors.rs"]
mod snapshot_anchors;
#[path = "text/snapshot_apply.rs"]
mod snapshot_apply;
#[path = "text/snapshot_edits.rs"]
mod snapshot_edits;
pub mod subscription;
#[cfg(any(test, feature = "test-support"))]
#[path = "text/test_support.rs"]
mod test_support;
#[cfg(test)]
mod tests;
mod types;
mod undo_map;

pub use anchor::*;
use anyhow::Result;
use clock::Lamport;
pub use clock::ReplicaId;
use collections::{HashMap, HashSet};
#[cfg(debug_assertions)]
pub use debug_ranges::debug;
use fragment_builders::{FragmentBuilder, RopeBuilder};
use fragment_insertions::push_fragments_for_insertion;
pub use fragment_summaries::FullOffset;
use fragment_summaries::VersionedFullOffset;
pub use history::Transaction;
use history::*;
pub use line_ending::{LineEnding, chunks_with_line_ending};
pub use line_indent::LineIndent;
use locator::Locator;
use operation_queue::OperationQueue;
pub use operation_traits::*;
pub use patch::Patch;
use postage::{oneshot, prelude::*};

pub use rope::*;
pub use selection::*;
use smallvec::SmallVec;
use std::{
    cmp::{self, Ordering, Reverse},
    future::Future,
    iter::Iterator,
    ops::{self, Deref, Range},
    str,
    sync::Arc,
    time::{Duration, Instant},
};
pub use subscription::*;
pub use sum_tree::Bias;
use sum_tree::{Dimensions, FilterCursor, SumTree, Summary, TreeMap, TreeSet};
pub use types::*;
use undo_map::UndoMap;
use util::debug_panic;

#[cfg(any(test, feature = "test-support"))]
use util::RandomCharIter;

/// The maximum length of a single insertion operation.
/// Fragments larger than this will be split into multiple smaller
/// fragments. This allows us to use relative `u32` offsets instead of `usize`,
/// reducing memory usage.
const MAX_INSERTION_LEN: usize = if cfg!(test) { 16 } else { u32::MAX as usize };

pub type TransactionId = clock::Lamport;

pub struct Buffer {
    snapshot: BufferSnapshot,
    history: History,
    deferred_ops: OperationQueue<Operation>,
    deferred_replicas: HashSet<ReplicaId>,
    pub lamport_clock: clock::Lamport,
    subscriptions: Topic<usize>,
    edit_id_resolvers: HashMap<clock::Lamport, Vec<oneshot::Sender<()>>>,
    wait_for_version_txs: Vec<(clock::Global, oneshot::Sender<()>)>,
}

#[derive(Clone)]
pub struct BufferSnapshot {
    visible_text: Rope,
    deleted_text: Rope,
    fragments: SumTree<Fragment>,
    insertions: SumTree<InsertionFragment>,
    insertion_slices: TreeSet<InsertionSlice>,
    undo_map: UndoMap,
    pub version: clock::Global,
    remote_id: BufferId,
    replica_id: ReplicaId,
    line_ending: LineEnding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InsertionSlice {
    // Inline the lamports to allow the replica ids to share the same alignment
    // saving 4 bytes space edit_id: clock::Lamport,
    edit_id_value: clock::Seq,
    edit_id_replica_id: ReplicaId,
    // insertion_id: clock::Lamport,
    insertion_id_value: clock::Seq,
    insertion_id_replica_id: ReplicaId,
    range: Range<u32>,
}

impl Ord for InsertionSlice {
    fn cmp(&self, other: &Self) -> Ordering {
        Lamport {
            value: self.edit_id_value,
            replica_id: self.edit_id_replica_id,
        }
        .cmp(&Lamport {
            value: other.edit_id_value,
            replica_id: other.edit_id_replica_id,
        })
        .then_with(|| {
            Lamport {
                value: self.insertion_id_value,
                replica_id: self.insertion_id_replica_id,
            }
            .cmp(&Lamport {
                value: other.insertion_id_value,
                replica_id: other.insertion_id_replica_id,
            })
        })
        .then_with(|| self.range.start.cmp(&other.range.start))
        .then_with(|| self.range.end.cmp(&other.range.end))
    }
}

impl PartialOrd for InsertionSlice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl InsertionSlice {
    fn from_fragment(edit_id: clock::Lamport, fragment: &Fragment) -> Self {
        Self {
            edit_id_value: edit_id.value,
            edit_id_replica_id: edit_id.replica_id,
            insertion_id_value: fragment.timestamp.value,
            insertion_id_replica_id: fragment.timestamp.replica_id,
            range: fragment.insertion_offset..fragment.insertion_offset + fragment.len,
        }
    }
}

struct Edits<'a, D: TextDimension, F: FnMut(&FragmentSummary) -> bool> {
    visible_cursor: rope::Cursor<'a>,
    deleted_cursor: rope::Cursor<'a>,
    fragments_cursor: Option<FilterCursor<'a, 'static, F, Fragment, FragmentTextSummary>>,
    undos: &'a UndoMap,
    since: &'a clock::Global,
    old_end: D,
    new_end: D,
    range: Range<(&'a Locator, u32)>,
    buffer_id: BufferId,
}

#[derive(Eq, PartialEq, Clone, Debug)]
struct Fragment {
    id: Locator,
    timestamp: clock::Lamport,
    insertion_offset: u32,
    len: u32,
    visible: bool,
    deletions: SmallVec<[clock::Lamport; 2]>,
    max_undos: clock::Global,
}

#[derive(Eq, PartialEq, Clone, Debug)]
struct FragmentSummary {
    text: FragmentTextSummary,
    max_id: Locator,
    max_version: clock::Global,
    min_insertion_version: clock::Global,
    max_insertion_version: clock::Global,
}

#[derive(Copy, Default, Clone, Debug, PartialEq, Eq)]
struct FragmentTextSummary {
    visible: usize,
    deleted: usize,
}

impl<'a> sum_tree::Dimension<'a, FragmentSummary> for FragmentTextSummary {
    fn zero(_: &Option<clock::Global>) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a FragmentSummary, _: &Option<clock::Global>) {
        self.visible += summary.text.visible;
        self.deleted += summary.text.deleted;
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
struct InsertionFragment {
    timestamp: clock::Lamport,
    split_offset: u32,
    fragment_id: Locator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct InsertionFragmentKey {
    timestamp: clock::Lamport,
    split_offset: u32,
}

pub struct EditedBufferSnapshot {
    pub base_version: clock::Global,
    pub snapshot: BufferSnapshot,
    pub did_edit: bool,
}
