use super::*;

impl Editor {
    pub(crate) fn stage_or_unstage_diff_hunks(
        &mut self,
        stage: bool,
        ranges: Vec<Range<Anchor>>,
        cx: &mut Context<Self>,
    ) {
        if self.delegate_stage_and_restore {
            let snapshot = self.buffer.read(cx).snapshot(cx);
            let hunks: Vec<_> = self.diff_hunks_in_ranges(&ranges, &snapshot).collect();
            if !hunks.is_empty() {
                cx.emit(EditorEvent::StageOrUnstageRequested { stage, hunks });
            }
            return;
        }
        let task = self.save_buffers_for_ranges_if_needed(&ranges, cx);
        cx.spawn(async move |this, cx| {
            task.await?;
            this.update(cx, |this, cx| {
                let snapshot = this.buffer.read(cx).snapshot(cx);
                let chunk_by = this
                    .diff_hunks_in_ranges(&ranges, &snapshot)
                    .chunk_by(|hunk| hunk.buffer_id);
                for (buffer_id, hunks) in &chunk_by {
                    this.do_stage_or_unstage(stage, buffer_id, hunks, cx);
                }
            })
        })
        .detach_and_log_err(cx);
    }
    pub(crate) fn toggle_diff_hunks_in_ranges(
        &mut self,
        ranges: Vec<Range<Anchor>>,
        cx: &mut Context<Editor>,
    ) {
        self.buffer.update(cx, |buffer, cx| {
            let expand = !buffer.has_expanded_diff_hunks_in_ranges(&ranges, cx);
            buffer.expand_or_collapse_diff_hunks(ranges, expand, cx);
        })
    }

    pub(crate) fn start_git_blame(
        &mut self,
        user_triggered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(project) = self.project() {
            if let Some(buffer) = self.buffer().read(cx).as_singleton()
                && buffer.read(cx).file().is_none()
            {
                return;
            }

            let focused = self.focus_handle(cx).contains_focused(window, cx);

            let project = project.clone();
            let blame = cx
                .new(|cx| GitBlame::new(self.buffer.clone(), project, user_triggered, focused, cx));
            self.blame_subscription =
                Some(cx.observe_in(&blame, window, |_, _, _, cx| cx.notify()));
            self.blame = Some(blame);
        }
    }

    pub(crate) fn restore_hunks_in_ranges(
        &mut self,
        ranges: Vec<Range<Point>>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if self.delegate_stage_and_restore {
            let hunks = self.snapshot(window, cx).hunks_for_ranges(ranges);
            if !hunks.is_empty() {
                cx.emit(EditorEvent::RestoreRequested { hunks });
            }
            return;
        }
        let hunks = self.snapshot(window, cx).hunks_for_ranges(ranges);
        self.transact(window, cx, |editor, window, cx| {
            editor.restore_diff_hunks(hunks, cx);
            let selections = editor
                .selections
                .all::<MultiBufferOffset>(&editor.display_snapshot(cx));
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select(selections);
            });
        });
    }

    pub(crate) fn has_stageable_diff_hunks_in_ranges(
        &self,
        ranges: &[Range<Anchor>],
        snapshot: &MultiBufferSnapshot,
    ) -> bool {
        let mut hunks = self.diff_hunks_in_ranges(ranges, snapshot);
        hunks.any(|hunk| hunk.status().has_secondary_hunk())
    }

    pub(crate) fn prepare_restore_change(
        &self,
        revert_changes: &mut HashMap<BufferId, Vec<(Range<text::Anchor>, Rope)>>,
        hunk: &MultiBufferDiffHunk,
        cx: &mut App,
    ) -> Option<()> {
        if hunk.is_created_file() {
            return None;
        }
        let multi_buffer = self.buffer.read(cx);
        let multi_buffer_snapshot = multi_buffer.snapshot(cx);
        let diff_snapshot = multi_buffer_snapshot.diff_for_buffer_id(hunk.buffer_id)?;
        let original_text = diff_snapshot
            .base_text()
            .as_rope()
            .slice(hunk.diff_base_byte_range.start.0..hunk.diff_base_byte_range.end.0);
        let buffer = multi_buffer.buffer(hunk.buffer_id)?;
        let buffer = buffer.read(cx);
        let buffer_snapshot = buffer.snapshot();
        let buffer_revert_changes = revert_changes.entry(buffer.remote_id()).or_default();
        if let Err(i) = buffer_revert_changes.binary_search_by(|probe| {
            probe
                .0
                .start
                .cmp(&hunk.buffer_range.start, &buffer_snapshot)
                .then(probe.0.end.cmp(&hunk.buffer_range.end, &buffer_snapshot))
        }) {
            buffer_revert_changes.insert(i, (hunk.buffer_range.clone(), original_text));
            Some(())
        } else {
            None
        }
    }

    pub(crate) fn save_buffers_for_ranges_if_needed(
        &mut self,
        ranges: &[Range<Anchor>],
        cx: &mut Context<Editor>,
    ) -> Task<Result<()>> {
        let multibuffer = self.buffer.read(cx);
        let snapshot = multibuffer.read(cx);
        let buffer_ids: HashSet<_> = ranges
            .iter()
            .flat_map(|range| snapshot.buffer_ids_for_range(range.clone()))
            .collect();
        drop(snapshot);

        let mut buffers = HashSet::default();
        for buffer_id in buffer_ids {
            if let Some(buffer_entity) = multibuffer.buffer(buffer_id) {
                let buffer = buffer_entity.read(cx);
                if buffer.file().is_some_and(|file| file.disk_state().exists()) && buffer.is_dirty()
                {
                    buffers.insert(buffer_entity);
                }
            }
        }

        if let Some(project) = &self.project {
            project.update(cx, |project, cx| project.save_buffers(buffers, cx))
        } else {
            Task::ready(Ok(()))
        }
    }

    pub(crate) fn do_stage_or_unstage_and_next(
        &mut self,
        stage: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ranges = self.selections.disjoint_anchor_ranges().collect::<Vec<_>>();

        if ranges.iter().any(|range| range.start != range.end) {
            self.stage_or_unstage_diff_hunks(stage, ranges, cx);
            return;
        }

        self.stage_or_unstage_diff_hunks(stage, ranges, cx);

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
}
