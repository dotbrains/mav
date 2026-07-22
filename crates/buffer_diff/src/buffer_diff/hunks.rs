use super::*;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiffHunkStatus {
    pub kind: DiffHunkStatusKind,
    pub secondary: DiffHunkSecondaryStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffHunkStatusKind {
    Added,
    Modified,
    Deleted,
}

/// Diff of Working Copy vs Index
/// aka 'is this hunk staged or not'
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffHunkSecondaryStatus {
    /// Unstaged
    HasSecondaryHunk,
    /// Partially staged
    OverlapsWithSecondaryHunk,
    /// Staged
    NoSecondaryHunk,
    /// We are unstaging
    SecondaryHunkAdditionPending,
    /// We are stagind
    SecondaryHunkRemovalPending,
}

/// A diff hunk resolved to rows in the buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// The buffer range as points.
    pub range: Range<Point>,
    /// The range in the buffer to which this hunk corresponds.
    pub buffer_range: Range<Anchor>,
    /// The range in the buffer's diff base text to which this hunk corresponds.
    pub diff_base_byte_range: Range<usize>,
    pub secondary_status: DiffHunkSecondaryStatus,
    // Anchors representing the word diff locations in the active buffer
    pub buffer_word_diffs: Vec<Range<Anchor>>,
    // Offsets relative to the start of the deleted diff that represent word diff locations
    pub base_word_diffs: Vec<Range<usize>>,
}

/// We store [`InternalDiffHunk`]s internally so we don't need to store the additional row range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InternalDiffHunk {
    pub(super) buffer_range: Range<Anchor>,
    pub(super) diff_base_byte_range: Range<usize>,
    pub(super) diff_base_point_range: Range<Point>,
    pub(super) base_word_diffs: Vec<Range<usize>>,
    pub(super) buffer_word_diffs: Vec<Range<Anchor>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingHunk {
    pub(super) buffer_range: Range<Anchor>,
    pub(super) diff_base_byte_range: Range<usize>,
    pub(super) buffer_version: clock::Global,
    pub(super) new_status: DiffHunkSecondaryStatus,
}

#[derive(Debug, Clone)]
pub struct DiffHunkSummary {
    pub(super) buffer_range: Range<Anchor>,
    pub(super) diff_base_byte_range: Range<usize>,
    pub(super) added_rows: u32,
    pub(super) removed_rows: u32,
}

impl sum_tree::Item for InternalDiffHunk {
    type Summary = DiffHunkSummary;

    fn summary(&self, buffer: &text::BufferSnapshot) -> Self::Summary {
        let buffer_start = self.buffer_range.start.to_point(buffer);
        let buffer_end = self.buffer_range.end.to_point(buffer);
        DiffHunkSummary {
            buffer_range: self.buffer_range.clone(),
            diff_base_byte_range: self.diff_base_byte_range.clone(),
            added_rows: buffer_end.row.saturating_sub(buffer_start.row),
            removed_rows: self
                .diff_base_point_range
                .end
                .row
                .saturating_sub(self.diff_base_point_range.start.row),
        }
    }
}

impl sum_tree::Item for PendingHunk {
    type Summary = DiffHunkSummary;

    fn summary(&self, _cx: &text::BufferSnapshot) -> Self::Summary {
        DiffHunkSummary {
            buffer_range: self.buffer_range.clone(),
            diff_base_byte_range: self.diff_base_byte_range.clone(),
            added_rows: 0,
            removed_rows: 0,
        }
    }
}

impl sum_tree::Summary for DiffHunkSummary {
    type Context<'a> = &'a text::BufferSnapshot;

    fn zero(buffer: &text::BufferSnapshot) -> Self {
        DiffHunkSummary {
            buffer_range: Anchor::min_min_range_for_buffer(buffer.remote_id()),
            diff_base_byte_range: 0..0,
            added_rows: 0,
            removed_rows: 0,
        }
    }

    fn add_summary(&mut self, other: &Self, buffer: Self::Context<'_>) {
        self.buffer_range.start = *self
            .buffer_range
            .start
            .min(&other.buffer_range.start, buffer);
        self.buffer_range.end = *self.buffer_range.end.max(&other.buffer_range.end, buffer);

        self.diff_base_byte_range.start = self
            .diff_base_byte_range
            .start
            .min(other.diff_base_byte_range.start);
        self.diff_base_byte_range.end = self
            .diff_base_byte_range
            .end
            .max(other.diff_base_byte_range.end);

        self.added_rows += other.added_rows;
        self.removed_rows += other.removed_rows;
    }
}

impl sum_tree::SeekTarget<'_, DiffHunkSummary, DiffHunkSummary> for Anchor {
    fn cmp(&self, cursor_location: &DiffHunkSummary, buffer: &text::BufferSnapshot) -> Ordering {
        if self
            .cmp(&cursor_location.buffer_range.start, buffer)
            .is_lt()
        {
            Ordering::Less
        } else if self.cmp(&cursor_location.buffer_range.end, buffer).is_gt() {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl sum_tree::SeekTarget<'_, DiffHunkSummary, DiffHunkSummary> for usize {
    fn cmp(&self, cursor_location: &DiffHunkSummary, _cx: &text::BufferSnapshot) -> Ordering {
        if *self < cursor_location.diff_base_byte_range.start {
            Ordering::Less
        } else if *self > cursor_location.diff_base_byte_range.end {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    }
}

impl DiffHunk {
    pub fn is_created_file(&self) -> bool {
        self.diff_base_byte_range == (0..0)
            && self.buffer_range.start.is_min()
            && self.buffer_range.end.is_max()
    }

    pub fn status(&self) -> DiffHunkStatus {
        let kind = if self.buffer_range.start == self.buffer_range.end {
            DiffHunkStatusKind::Deleted
        } else if self.diff_base_byte_range.is_empty() {
            DiffHunkStatusKind::Added
        } else {
            DiffHunkStatusKind::Modified
        };
        DiffHunkStatus {
            kind,
            secondary: self.secondary_status,
        }
    }
}

impl DiffHunkStatus {
    pub fn has_secondary_hunk(&self) -> bool {
        matches!(
            self.secondary,
            DiffHunkSecondaryStatus::HasSecondaryHunk
                | DiffHunkSecondaryStatus::SecondaryHunkAdditionPending
                | DiffHunkSecondaryStatus::OverlapsWithSecondaryHunk
        )
    }

    pub fn is_pending(&self) -> bool {
        matches!(
            self.secondary,
            DiffHunkSecondaryStatus::SecondaryHunkAdditionPending
                | DiffHunkSecondaryStatus::SecondaryHunkRemovalPending
        )
    }

    pub fn is_deleted(&self) -> bool {
        self.kind == DiffHunkStatusKind::Deleted
    }

    pub fn is_added(&self) -> bool {
        self.kind == DiffHunkStatusKind::Added
    }

    pub fn is_modified(&self) -> bool {
        self.kind == DiffHunkStatusKind::Modified
    }

    pub fn added(secondary: DiffHunkSecondaryStatus) -> Self {
        Self {
            kind: DiffHunkStatusKind::Added,
            secondary,
        }
    }

    pub fn modified(secondary: DiffHunkSecondaryStatus) -> Self {
        Self {
            kind: DiffHunkStatusKind::Modified,
            secondary,
        }
    }

    pub fn deleted(secondary: DiffHunkSecondaryStatus) -> Self {
        Self {
            kind: DiffHunkStatusKind::Deleted,
            secondary,
        }
    }

    pub fn deleted_none() -> Self {
        Self {
            kind: DiffHunkStatusKind::Deleted,
            secondary: DiffHunkSecondaryStatus::NoSecondaryHunk,
        }
    }

    pub fn added_none() -> Self {
        Self {
            kind: DiffHunkStatusKind::Added,
            secondary: DiffHunkSecondaryStatus::NoSecondaryHunk,
        }
    }

    pub fn modified_none() -> Self {
        Self {
            kind: DiffHunkStatusKind::Modified,
            secondary: DiffHunkSecondaryStatus::NoSecondaryHunk,
        }
    }
}
