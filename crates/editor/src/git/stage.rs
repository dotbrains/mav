use super::*;

impl Editor {
    pub(super) fn toggle_staged_selected_diff_hunks(
        &mut self,
        _: &::git::ToggleStaged,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer.read(cx).snapshot(cx);
        let ranges: Vec<_> = self
            .selections
            .disjoint_anchors()
            .iter()
            .map(|s| s.range())
            .collect();
        let stage = self.has_stageable_diff_hunks_in_ranges(&ranges, &snapshot);
        self.stage_or_unstage_diff_hunks(stage, ranges, cx);
    }
    pub(super) fn stage_and_next(
        &mut self,
        _: &::git::StageAndNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_stage_or_unstage_and_next(true, window, cx);
    }

    pub(super) fn unstage_and_next(
        &mut self,
        _: &::git::UnstageAndNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_stage_or_unstage_and_next(false, window, cx);
    }

    pub(super) fn do_stage_or_unstage(
        &self,
        stage: bool,
        buffer_id: BufferId,
        hunks: impl Iterator<Item = MultiBufferDiffHunk>,
        cx: &mut App,
    ) -> Option<()> {
        let project = self.project()?;
        let buffer = project.read(cx).buffer_for_id(buffer_id, cx)?;
        let diff = self.buffer.read(cx).diff_for(buffer_id)?;
        let buffer_snapshot = buffer.read(cx).snapshot();
        let file_exists = buffer_snapshot
            .file()
            .is_some_and(|file| file.disk_state().exists());
        diff.update(cx, |diff, cx| {
            diff.stage_or_unstage_hunks(
                stage,
                &hunks
                    .map(|hunk| buffer_diff::DiffHunk {
                        buffer_range: hunk.buffer_range,
                        // We don't need to pass in word diffs here because they're only used for rendering and
                        // this function changes internal state
                        base_word_diffs: Vec::default(),
                        buffer_word_diffs: Vec::default(),
                        diff_base_byte_range: hunk.diff_base_byte_range.start.0
                            ..hunk.diff_base_byte_range.end.0,
                        secondary_status: hunk.status.secondary,
                        range: Point::zero()..Point::zero(), // unused
                    })
                    .collect::<Vec<_>>(),
                &buffer_snapshot,
                file_exists,
                cx,
            )
        });
        None
    }

    pub(super) fn clear_expanded_diff_hunks(&mut self, cx: &mut Context<Self>) -> bool {
        self.buffer.update(cx, |buffer, cx| {
            let ranges = vec![Anchor::Min..Anchor::Max];
            if !buffer.all_diff_hunks_expanded()
                && buffer.has_expanded_diff_hunks_in_ranges(&ranges, cx)
            {
                buffer.collapse_diff_hunks(ranges, cx);
                true
            } else {
                false
            }
        })
    }

    pub(super) fn has_any_expanded_diff_hunks(&self, cx: &App) -> bool {
        if self.buffer.read(cx).all_diff_hunks_expanded() {
            return true;
        }
        let ranges = vec![Anchor::Min..Anchor::Max];
        self.buffer
            .read(cx)
            .has_expanded_diff_hunks_in_ranges(&ranges, cx)
    }

    pub(super) fn toggle_single_diff_hunk(&mut self, range: Range<Anchor>, cx: &mut Context<Self>) {
        self.buffer.update(cx, |buffer, cx| {
            buffer.toggle_single_diff_hunk(range, cx);
        })
    }
}
