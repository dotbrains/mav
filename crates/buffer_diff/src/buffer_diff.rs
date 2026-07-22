#[path = "buffer_diff/diff_compute.rs"]
mod diff_compute;
#[path = "buffer_diff/diff_lifecycle.rs"]
mod diff_lifecycle;
#[path = "buffer_diff/diff_operations.rs"]
mod diff_operations;
#[path = "buffer_diff/hunks.rs"]
mod hunks;
#[path = "buffer_diff/patch_mapping.rs"]
mod patch_mapping;
#[path = "buffer_diff/snapshot_queries.rs"]
mod snapshot_queries;
#[path = "buffer_diff/snapshot_staging.rs"]
mod snapshot_staging;
#[cfg(test)]
mod test_base_text;
#[cfg(test)]
mod test_basic;
#[cfg(test)]
mod test_compare;
#[cfg(test)]
mod test_patch_ranges;
#[cfg(test)]
mod test_ranges;
#[cfg(test)]
mod test_staging;
use diff_compute::*;
use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Task};
use hunks::*;
pub use hunks::{DiffHunk, DiffHunkSecondaryStatus, DiffHunkStatus, DiffHunkStatusKind};
use imara_diff::{Algorithm, Sink, intern::InternedInput, sources::lines_with_terminator};
use language::{
    Capability, DiffOptions, Language, LanguageName, LanguageRegistry,
    language_settings::LanguageSettings, word_diff_ranges,
};
use rope::Rope;
use std::{
    iter,
    ops::{Range, RangeInclusive},
    sync::Arc,
};
use sum_tree::SumTree;
use text::{
    Anchor, Bias, BufferId, Edit, OffsetRangeExt, Patch, Point, ToOffset as _, ToPoint as _,
};
use util::{ResultExt, debug_panic};

pub const MAX_WORD_DIFF_LINE_COUNT: usize = 5;

pub struct BufferDiff {
    pub buffer_id: BufferId,
    base_text_buffer: Entity<language::Buffer>,
    diff_snapshot: Option<BufferDiffSnapshot>,
    secondary_diff: Option<Entity<BufferDiff>>,
    buffer_snapshot: text::BufferSnapshot,
}

#[derive(Clone)]
pub struct BufferDiffSnapshot {
    hunks: SumTree<InternalDiffHunk>,
    pending_hunks: SumTree<PendingHunk>,
    base_text: language::BufferSnapshot,
    base_text_exists: bool,
    buffer_snapshot: text::BufferSnapshot,
    secondary_diff: Option<Arc<BufferDiffSnapshot>>,
}

impl std::fmt::Debug for BufferDiffSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferDiffSnapshot")
            .field("hunks", &self.hunks)
            .field("remote_id", &self.base_text.remote_id())
            .field("secondary_diff", &self.secondary_diff)
            .finish()
    }
}

#[derive(Clone)]
pub struct BufferDiffUpdate {
    hunks: SumTree<InternalDiffHunk>,
    base_text: language::BufferSnapshot,
    base_text_exists: bool,
    buffer_snapshot: text::BufferSnapshot,
}

impl BufferDiffUpdate {
    pub fn set_base_text_snapshot(
        &mut self,
        base_text: language::BufferSnapshot,
        base_text_exists: bool,
    ) {
        self.base_text = base_text;
        self.base_text_exists = base_text_exists;
    }
}

impl std::fmt::Debug for BufferDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferChangeSet")
            .field("buffer_id", &self.buffer_id)
            .finish()
    }
}

#[derive(Clone, Debug, Default)]
pub struct DiffChanged {
    pub changed_range: Option<Range<text::Anchor>>,
    pub base_text_changed_range: Option<Range<usize>>,
    pub extended_range: Option<Range<text::Anchor>>,
    pub base_text_changed: bool,
}

#[derive(Clone, Debug)]
pub enum BufferDiffEvent {
    BaseTextChanged,
    DiffChanged(DiffChanged),
    HunksStagedOrUnstaged(Option<Rope>),
}

impl EventEmitter<BufferDiffEvent> for BufferDiff {}

#[cfg(any(test, feature = "test-support"))]
#[track_caller]
pub fn assert_hunks<ExpectedText, HunkIter>(
    diff_hunks: HunkIter,
    buffer: &text::BufferSnapshot,
    diff_base: &str,
    // Line range, deleted, added, status
    expected_hunks: &[(Range<u32>, ExpectedText, ExpectedText, DiffHunkStatus)],
) where
    HunkIter: Iterator<Item = DiffHunk>,
    ExpectedText: AsRef<str>,
{
    let actual_hunks = diff_hunks
        .map(|hunk| {
            (
                hunk.range.clone(),
                &diff_base[hunk.diff_base_byte_range.clone()],
                buffer
                    .text_for_range(hunk.range.clone())
                    .collect::<String>(),
                hunk.status(),
            )
        })
        .collect::<Vec<_>>();

    let expected_hunks: Vec<_> = expected_hunks
        .iter()
        .map(|(line_range, deleted_text, added_text, status)| {
            (
                Point::new(line_range.start, 0)..Point::new(line_range.end, 0),
                deleted_text.as_ref(),
                added_text.as_ref().to_string(),
                *status,
            )
        })
        .collect();

    pretty_assertions::assert_eq!(actual_hunks, expected_hunks);
}
