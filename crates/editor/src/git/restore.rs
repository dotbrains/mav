use super::*;

impl Editor {
    pub(super) fn restore_file(
        &mut self,
        _: &::git::RestoreFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        let mut buffer_ids = HashSet::default();
        let snapshot = self.buffer().read(cx).snapshot(cx);
        for selection in self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx))
        {
            buffer_ids.extend(snapshot.buffer_ids_for_range(selection.range()))
        }
        let ranges = buffer_ids
            .into_iter()
            .flat_map(|buffer_id| snapshot.range_for_buffer(buffer_id))
            .collect::<Vec<_>>();

        self.restore_hunks_in_ranges(ranges, window, cx);
    }

    /// Restores the diff hunks in the editor's selections and moves the cursor
    /// to the next diff hunk. Wraps around to the beginning of the buffer if
    /// not all diff hunks are expanded.
    pub(super) fn restore_and_next(
        &mut self,
        _: &::git::RestoreAndNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        let selections = self
            .selections
            .all(&self.display_snapshot(cx))
            .into_iter()
            .map(|selection| selection.range())
            .collect();

        self.restore_hunks_in_ranges(selections, window, cx);

        let all_diff_hunks_expanded = self.buffer().read(cx).all_diff_hunks_expanded();
        let wrap_around = !all_diff_hunks_expanded;
        let snapshot = self.snapshot(window, cx);
        let position = self
            .selections
            .newest::<Point>(&snapshot.display_snapshot)
            .head();

        self.go_to_hunk_before_or_after_position(
            &snapshot,
            position,
            Direction::Next,
            wrap_around,
            window,
            cx,
        );
    }

    pub(super) fn restore_diff_hunks(&self, hunks: Vec<MultiBufferDiffHunk>, cx: &mut App) {
        let mut revert_changes = HashMap::default();
        let chunk_by = hunks.into_iter().chunk_by(|hunk| hunk.buffer_id);
        for (buffer_id, hunks) in &chunk_by {
            let hunks = hunks.collect::<Vec<_>>();
            for hunk in &hunks {
                self.prepare_restore_change(&mut revert_changes, hunk, cx);
            }
            self.do_stage_or_unstage(false, buffer_id, hunks.into_iter(), cx);
        }
        if !revert_changes.is_empty() {
            self.buffer().update(cx, |multi_buffer, cx| {
                for (buffer_id, changes) in revert_changes {
                    if let Some(buffer) = multi_buffer.buffer(buffer_id) {
                        buffer.update(cx, |buffer, cx| {
                            buffer.edit(
                                changes
                                    .into_iter()
                                    .map(|(range, text)| (range, text.to_string())),
                                None,
                                cx,
                            );
                        });
                    }
                }
            });
        }
    }
}
