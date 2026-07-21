use super::*;

impl MultiBufferSnapshot {
    pub fn diff_for_buffer_id(&self, buffer_id: BufferId) -> Option<&BufferDiffSnapshot> {
        self.diff_state(buffer_id).map(|diff| &diff.diff)
    }

    pub(super) fn diff_state(&self, buffer_id: BufferId) -> Option<&DiffStateSnapshot> {
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

    pub(super) fn excerpt_edits_for_diff_change(
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
}
