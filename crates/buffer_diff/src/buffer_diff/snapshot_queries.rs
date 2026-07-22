use super::*;

impl BufferDiffSnapshot {
    #[cfg(test)]
    fn new_sync(
        buffer: &text::BufferSnapshot,
        diff_base: String,
        cx: &mut gpui::TestAppContext,
    ) -> BufferDiffSnapshot {
        let buffer_diff = cx.new(|cx| BufferDiff::new_with_base_text(&diff_base, buffer, cx));
        buffer_diff.update(cx, |buffer_diff, cx| buffer_diff.snapshot(cx))
    }

    pub fn buffer_id(&self) -> BufferId {
        self.buffer_snapshot.remote_id()
    }

    pub fn buffer_snapshot(&self) -> &text::BufferSnapshot {
        &self.buffer_snapshot
    }

    pub fn is_empty(&self) -> bool {
        self.hunks.is_empty()
    }

    pub fn changed_row_counts(&self) -> (u32, u32) {
        let summary = self.hunks.summary();
        (summary.added_rows, summary.removed_rows)
    }

    pub fn base_text_string(&self) -> Option<String> {
        self.base_text_exists.then(|| self.base_text.text())
    }

    pub fn base_text_exists(&self) -> bool {
        self.base_text_exists
    }

    pub fn secondary_diff(&self) -> Option<&BufferDiffSnapshot> {
        self.secondary_diff.as_deref()
    }

    pub fn buffer_version(&self) -> &clock::Global {
        self.buffer_snapshot.version()
    }

    pub(super) fn original_buffer_snapshot(&self) -> &text::BufferSnapshot {
        &self.buffer_snapshot
    }

    #[ztracing::instrument(skip_all)]
    pub fn hunks_intersecting_range<'a>(
        &'a self,
        range: Range<Anchor>,
        buffer: &'a text::BufferSnapshot,
    ) -> impl 'a + Iterator<Item = DiffHunk> {
        let unstaged_counterpart = self.secondary_diff.as_deref();
        let range = range.to_offset(buffer);
        let filter = move |summary: &DiffHunkSummary| {
            let summary_range = summary.buffer_range.to_offset(buffer);
            let before_start = summary_range.end < range.start;
            let after_end = summary_range.start > range.end;
            !before_start && !after_end
        };
        self.hunks_intersecting_range_impl(filter, buffer, unstaged_counterpart)
    }

    pub fn hunks_intersecting_range_rev<'a>(
        &'a self,
        range: Range<Anchor>,
        buffer: &'a text::BufferSnapshot,
    ) -> impl 'a + Iterator<Item = DiffHunk> {
        let filter = move |summary: &DiffHunkSummary| {
            let before_start = summary.buffer_range.end.cmp(&range.start, buffer).is_lt();
            let after_end = summary.buffer_range.start.cmp(&range.end, buffer).is_gt();
            !before_start && !after_end
        };
        self.hunks_intersecting_range_rev_impl(filter, buffer)
    }

    pub fn hunks_intersecting_base_text_range<'a>(
        &'a self,
        range: Range<usize>,
        main_buffer: &'a text::BufferSnapshot,
    ) -> impl 'a + Iterator<Item = DiffHunk> {
        let unstaged_counterpart = self.secondary_diff.as_deref();
        let filter = move |summary: &DiffHunkSummary| {
            let before_start = summary.diff_base_byte_range.end < range.start;
            let after_end = summary.diff_base_byte_range.start > range.end;
            !before_start && !after_end
        };
        self.hunks_intersecting_range_impl(filter, main_buffer, unstaged_counterpart)
    }

    pub fn hunks_intersecting_base_text_range_rev<'a>(
        &'a self,
        range: Range<usize>,
        main_buffer: &'a text::BufferSnapshot,
    ) -> impl 'a + Iterator<Item = DiffHunk> {
        let filter = move |summary: &DiffHunkSummary| {
            let before_start = summary.diff_base_byte_range.end.cmp(&range.start).is_lt();
            let after_end = summary.diff_base_byte_range.start.cmp(&range.end).is_gt();
            !before_start && !after_end
        };
        self.hunks_intersecting_range_rev_impl(filter, main_buffer)
    }

    pub fn hunks<'a>(
        &'a self,
        buffer_snapshot: &'a text::BufferSnapshot,
    ) -> impl 'a + Iterator<Item = DiffHunk> {
        self.hunks_intersecting_range(
            Anchor::min_max_range_for_buffer(buffer_snapshot.remote_id()),
            buffer_snapshot,
        )
    }

    pub fn hunks_in_row_range<'a>(
        &'a self,
        range: Range<u32>,
        buffer: &'a text::BufferSnapshot,
    ) -> impl 'a + Iterator<Item = DiffHunk> {
        let start = buffer.anchor_before(Point::new(range.start, 0));
        let end = buffer.anchor_after(Point::new(range.end, 0));
        self.hunks_intersecting_range(start..end, buffer)
    }

    pub fn range_to_hunk_range(
        &self,
        range: Range<Anchor>,
        buffer: &text::BufferSnapshot,
    ) -> (Option<Range<Anchor>>, Option<Range<usize>>) {
        let first_hunk = self.hunks_intersecting_range(range.clone(), buffer).next();
        let last_hunk = self.hunks_intersecting_range_rev(range, buffer).next();
        let range = first_hunk
            .as_ref()
            .zip(last_hunk.as_ref())
            .map(|(first, last)| first.buffer_range.start..last.buffer_range.end);
        let base_text_range = first_hunk
            .zip(last_hunk)
            .map(|(first, last)| first.diff_base_byte_range.start..last.diff_base_byte_range.end);
        (range, base_text_range)
    }

    pub fn base_text(&self) -> &language::BufferSnapshot {
        &self.base_text
    }

    /// If this function returns `true`, the base texts are equal. If this
    /// function returns `false`, they might be equal, but might not. This
    /// result is used to avoid recalculating diffs in situations where we know
    /// nothing has changed.
    pub fn base_texts_definitely_eq(&self, other: &Self) -> bool {
        if self.base_text_exists != other.base_text_exists {
            return false;
        }
        let left = &self.base_text;
        let right = &other.base_text;
        let (old_id, old_version, old_empty) = (left.remote_id(), left.version(), left.is_empty());
        let (new_id, new_version, new_empty) =
            (right.remote_id(), right.version(), right.is_empty());
        (new_id == old_id && new_version == old_version) || (new_empty && old_empty)
    }
}
