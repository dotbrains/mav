use super::*;

impl Editor {
    pub fn diff_hunks_in_ranges<'a>(
        &'a self,
        ranges: &'a [Range<Anchor>],
        buffer: &'a MultiBufferSnapshot,
    ) -> impl 'a + Iterator<Item = MultiBufferDiffHunk> {
        ranges.iter().flat_map(move |range| {
            let end_excerpt = buffer.excerpt_containing(range.end..range.end);
            let range = range.to_point(buffer);
            let mut peek_end = range.end;
            if range.end.row < buffer.max_row().0 {
                peek_end = Point::new(range.end.row + 1, 0);
            }
            buffer
                .diff_hunks_in_range(range.start..peek_end)
                .filter(move |hunk| {
                    if let Some((_, excerpt_range)) = &end_excerpt
                        && let Some(end_anchor) =
                            buffer.anchor_in_excerpt(excerpt_range.context.end)
                        && let Some(hunk_end_anchor) =
                            buffer.anchor_in_excerpt(hunk.excerpt_range.context.end)
                        && hunk_end_anchor.cmp(&end_anchor, buffer).is_gt()
                    {
                        false
                    } else {
                        true
                    }
                })
        })
    }
    pub fn set_render_diff_hunk_controls(
        &mut self,
        render_diff_hunk_controls: RenderDiffHunkControlsFn,
        cx: &mut Context<Self>,
    ) {
        self.render_diff_hunk_controls = render_diff_hunk_controls;
        cx.notify();
    }

    /// Make all diff hunks render with the "unstaged" appearance, regardless
    /// of whether they have a secondary hunk. Intended for views whose diffs
    /// aren't related to the git index (e.g. agent diffs).
    pub fn set_render_diff_hunks_as_unstaged(
        &mut self,
        render_as_unstaged: bool,
        cx: &mut Context<Self>,
    ) {
        self.render_diff_hunks_as_unstaged = render_as_unstaged;
        cx.notify();
    }

    pub fn git_blame_inline_enabled(&self) -> bool {
        self.git_blame_inline_enabled
    }

    pub fn blame(&self) -> Option<&Entity<GitBlame>> {
        self.blame.as_ref()
    }

    pub fn active_git_blame_entry(&self, cx: &mut App) -> Option<BlameEntry> {
        if !self.show_git_blame_inline
            || self.newest_selection_head_on_empty_line(cx)
            || !self.has_blame_entries(cx)
        {
            return None;
        }

        let blame = self.blame.as_ref()?;
        let snapshot = self.display_snapshot(cx);
        let cursor = self.selections.newest::<Point>(&snapshot).head();
        let (buffer, point) = snapshot.buffer_snapshot().point_to_buffer_point(cursor)?;

        blame
            .update(cx, |blame, cx| {
                blame
                    .blame_for_rows(
                        &[RowInfo {
                            buffer_id: Some(buffer.remote_id()),
                            buffer_row: Some(point.row),
                            ..Default::default()
                        }],
                        cx,
                    )
                    .next()
            })
            .flatten()
            .map(|(_, entry)| entry)
    }

    pub fn show_git_blame_gutter(&self) -> bool {
        self.show_git_blame_gutter
    }

    pub fn expand_selected_diff_hunks(&mut self, cx: &mut Context<Self>) {
        let ranges: Vec<_> = self
            .selections
            .disjoint_anchors()
            .iter()
            .map(|s| s.range())
            .collect();
        self.buffer
            .update(cx, |buffer, cx| buffer.expand_diff_hunks(ranges, cx))
    }

    pub fn toggle_git_blame(
        &mut self,
        _: &::git::Blame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_git_blame_gutter = !self.show_git_blame_gutter;

        if self.show_git_blame_gutter && !self.has_blame_entries(cx) {
            self.start_git_blame(true, window, cx);
        }

        cx.notify();
    }

    pub fn toggle_git_blame_inline(
        &mut self,
        _: &ToggleGitBlameInline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_git_blame_inline_internal(true, window, cx);
        cx.notify();
    }

    pub fn start_temporary_diff_override(&mut self) {
        self.load_diff_task.take();
        self.temporary_diff_override = true;
    }

    pub fn end_temporary_diff_override(&mut self, cx: &mut Context<Self>) {
        self.temporary_diff_override = false;
        self.render_diff_hunks_as_unstaged = false;
        self.set_render_diff_hunk_controls(Arc::new(render_diff_hunk_controls), cx);
        self.buffer.update(cx, |buffer, cx| {
            buffer.set_all_diff_hunks_collapsed(cx);
        });

        if let Some(project) = self.project.clone() {
            self.load_diff_task = Some(
                update_uncommitted_diff_for_buffer(
                    cx.entity(),
                    &project,
                    self.buffer.read(cx).all_buffers(),
                    self.buffer.clone(),
                    cx,
                )
                .shared(),
            );
        }
    }

    /// Hides the inline blame popover element, in case it's already visible, or
    /// interrupts the task meant to show it, in case the task is running.
    ///
    /// When `ignore_timeout` is set to `true`, the popover is hidden
    /// immediately, otherwise it'll be hidden after a short delay.
    ///
    /// Returns `true` if the popover was visible and was hidden, `false`
    /// otherwise.
    pub fn hide_blame_popover(&mut self, ignore_timeout: bool, cx: &mut Context<Self>) -> bool {
        self.inline_blame_popover_show_task.take();

        if let Some(state) = &mut self.inline_blame_popover {
            if ignore_timeout {
                self.inline_blame_popover.take();
                cx.notify();
            } else {
                state.hide_task = Some(cx.spawn(async move |editor, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(100))
                        .await;

                    editor
                        .update(cx, |editor, cx| {
                            editor.inline_blame_popover.take();
                            cx.notify();
                        })
                        .ok();
                }));
            }

            true
        } else {
            false
        }
    }

    pub fn git_restore(&mut self, _: &Restore, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        let selections = self
            .selections
            .all(&self.display_snapshot(cx))
            .into_iter()
            .map(|s| s.range())
            .collect();
        self.restore_hunks_in_ranges(selections, window, cx);
    }

    pub fn status_for_buffer_id(&self, buffer_id: BufferId, cx: &App) -> Option<FileStatus> {
        if let Some(status) = self
            .addons
            .iter()
            .find_map(|(_, addon)| addon.override_status_for_buffer_id(buffer_id, cx))
        {
            return Some(status);
        }
        self.project
            .as_ref()?
            .read(cx)
            .status_for_buffer_id(buffer_id, cx)
    }

    pub fn go_to_hunk_before_or_after_position(
        &mut self,
        snapshot: &EditorSnapshot,
        position: Point,
        direction: Direction,
        wrap_around: bool,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let row = if direction == Direction::Next {
            self.hunk_after_position(snapshot, position, wrap_around)
                .map(|hunk| hunk.row_range.start)
        } else {
            self.hunk_before_position(snapshot, position, wrap_around)
        };

        if let Some(row) = row {
            let destination = Point::new(row.0, 0);
            let autoscroll = Autoscroll::center();

            self.unfold_ranges(&[destination..destination], false, false, cx);
            self.change_selections(SelectionEffects::scroll(autoscroll), window, cx, |s| {
                s.select_ranges([destination..destination]);
            });
        }
    }

    pub fn set_expand_all_diff_hunks(&mut self, cx: &mut App) {
        self.buffer.update(cx, |buffer, cx| {
            buffer.set_all_diff_hunks_expanded(cx);
        });
    }

    pub fn expand_all_diff_hunks(
        &mut self,
        _: &ExpandAllDiffHunks,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.buffer.update(cx, |buffer, cx| {
            buffer.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx)
        });
    }
}
