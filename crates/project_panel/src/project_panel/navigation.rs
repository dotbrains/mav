use super::*;

impl ProjectPanel {
    pub(super) fn select_previous(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(edit_state) = &self.state.edit_state
            && edit_state.processing_filename.is_none()
        {
            self.filename_editor.update(cx, |editor, cx| {
                editor.move_to_beginning_of_line(
                    &editor::actions::MoveToBeginningOfLine {
                        stop_at_soft_wraps: false,
                        stop_at_indent: false,
                    },
                    window,
                    cx,
                );
            });
            return;
        }
        if let Some(selection) = self.selection {
            let (mut worktree_ix, mut entry_ix, _) =
                self.index_for_selection(selection).unwrap_or_default();
            if entry_ix > 0 {
                entry_ix -= 1;
            } else if worktree_ix > 0 {
                worktree_ix -= 1;
                entry_ix = self.state.visible_entries[worktree_ix].entries.len() - 1;
            } else {
                return;
            }

            let VisibleEntriesForWorktree {
                worktree_id,
                entries,
                ..
            } = &self.state.visible_entries[worktree_ix];
            let selection = SelectedEntry {
                worktree_id: *worktree_id,
                entry_id: entries[entry_ix].id,
            };
            self.selection = Some(selection);
            if window.modifiers().shift {
                self.marked_entries.push(selection);
            }
            self.autoscroll(cx);
            cx.notify();
        } else {
            self.select_first(&SelectFirst {}, window, cx);
        }
    }

    pub(super) fn unfold_directory(
        &mut self,
        _: &UnfoldDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_entry(cx) {
            self.state.unfolded_dir_ids.insert(entry.id);

            let snapshot = worktree.snapshot();
            let mut parent_path = entry.path.parent();
            while let Some(path) = parent_path {
                if let Some(parent_entry) = worktree.entry_for_path(path) {
                    let mut children_iter = snapshot.child_entries(path);

                    if children_iter.by_ref().take(2).count() > 1 {
                        break;
                    }

                    self.state.unfolded_dir_ids.insert(parent_entry.id);
                    parent_path = path.parent();
                } else {
                    break;
                }
            }

            self.update_visible_entries(None, false, true, window, cx);
            cx.notify();
        }
    }

    pub(super) fn fold_directory(
        &mut self,
        _: &FoldDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_entry(cx) {
            self.state.unfolded_dir_ids.remove(&entry.id);

            let snapshot = worktree.snapshot();
            let mut path = &*entry.path;
            loop {
                let mut child_entries_iter = snapshot.child_entries(path);
                if let Some(child) = child_entries_iter.next() {
                    if child_entries_iter.next().is_none() && child.is_dir() {
                        self.state.unfolded_dir_ids.remove(&child.id);
                        path = &*child.path;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            self.update_visible_entries(None, false, true, window, cx);
            cx.notify();
        }
    }

    pub(super) fn scroll_up(&mut self, _: &ScrollUp, window: &mut Window, cx: &mut Context<Self>) {
        for _ in 0..self.rendered_entries_len / 2 {
            window.dispatch_action(SelectPrevious.boxed_clone(), cx);
        }
    }

    pub(super) fn scroll_down(
        &mut self,
        _: &ScrollDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for _ in 0..self.rendered_entries_len / 2 {
            window.dispatch_action(SelectNext.boxed_clone(), cx);
        }
    }

    pub(super) fn scroll_cursor_center(
        &mut self,
        _: &ScrollCursorCenter,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, _, index)) = self.selection.and_then(|s| self.index_for_selection(s)) {
            self.scroll_handle
                .scroll_to_item_strict(index, ScrollStrategy::Center);
            cx.notify();
        }
    }

    pub(super) fn scroll_cursor_top(
        &mut self,
        _: &ScrollCursorTop,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, _, index)) = self.selection.and_then(|s| self.index_for_selection(s)) {
            self.scroll_handle
                .scroll_to_item_strict(index, ScrollStrategy::Top);
            cx.notify();
        }
    }

    pub(super) fn scroll_cursor_bottom(
        &mut self,
        _: &ScrollCursorBottom,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, _, index)) = self.selection.and_then(|s| self.index_for_selection(s)) {
            self.scroll_handle
                .scroll_to_item_strict(index, ScrollStrategy::Bottom);
            cx.notify();
        }
    }

    pub(super) fn select_next(
        &mut self,
        _: &SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(edit_state) = &self.state.edit_state
            && edit_state.processing_filename.is_none()
        {
            self.filename_editor.update(cx, |editor, cx| {
                editor.move_to_end_of_line(
                    &editor::actions::MoveToEndOfLine {
                        stop_at_soft_wraps: false,
                    },
                    window,
                    cx,
                );
            });
            return;
        }
        if let Some(selection) = self.selection {
            let (mut worktree_ix, mut entry_ix, _) =
                self.index_for_selection(selection).unwrap_or_default();
            if let Some(worktree_entries) = self
                .state
                .visible_entries
                .get(worktree_ix)
                .map(|v| &v.entries)
            {
                if entry_ix + 1 < worktree_entries.len() {
                    entry_ix += 1;
                } else {
                    worktree_ix += 1;
                    entry_ix = 0;
                }
            }

            if let Some(VisibleEntriesForWorktree {
                worktree_id,
                entries,
                ..
            }) = self.state.visible_entries.get(worktree_ix)
                && let Some(entry) = entries.get(entry_ix)
            {
                let selection = SelectedEntry {
                    worktree_id: *worktree_id,
                    entry_id: entry.id,
                };
                self.selection = Some(selection);
                if window.modifiers().shift {
                    self.marked_entries.push(selection);
                }

                self.autoscroll(cx);
                cx.notify();
            }
        } else {
            self.select_first(&SelectFirst {}, window, cx);
        }
    }

    pub(super) fn select_parent(
        &mut self,
        _: &SelectParent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_sub_entry(cx) {
            if let Some(parent) = entry.path.parent() {
                let worktree = worktree.read(cx);
                if let Some(parent_entry) = worktree.entry_for_path(parent) {
                    self.selection = Some(SelectedEntry {
                        worktree_id: worktree.id(),
                        entry_id: parent_entry.id,
                    });
                    self.autoscroll(cx);
                    cx.notify();
                }
            }
        } else {
            self.select_first(&SelectFirst {}, window, cx);
        }
    }

    pub(super) fn select_first(
        &mut self,
        _: &SelectFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(VisibleEntriesForWorktree {
            worktree_id,
            entries,
            ..
        }) = self.state.visible_entries.first()
            && let Some(entry) = entries.first()
        {
            let selection = SelectedEntry {
                worktree_id: *worktree_id,
                entry_id: entry.id,
            };
            self.selection = Some(selection);
            if window.modifiers().shift {
                self.marked_entries.push(selection);
            }
            self.autoscroll(cx);
            cx.notify();
        }
    }

    pub(super) fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(VisibleEntriesForWorktree {
            worktree_id,
            entries,
            ..
        }) = self.state.visible_entries.last()
        {
            let worktree = self.project.read(cx).worktree_for_id(*worktree_id, cx);
            if let (Some(worktree), Some(entry)) = (worktree, entries.last()) {
                let worktree = worktree.read(cx);
                if let Some(entry) = worktree.entry_for_id(entry.id) {
                    let selection = SelectedEntry {
                        worktree_id: *worktree_id,
                        entry_id: entry.id,
                    };
                    self.selection = Some(selection);
                    self.autoscroll(cx);
                    cx.notify();
                }
            }
        }
    }

    pub(super) fn autoscroll(&mut self, cx: &mut Context<Self>) {
        if let Some((_, _, index)) = self.selection.and_then(|s| self.index_for_selection(s)) {
            self.scroll_handle.scroll_to_item_with_offset(
                index,
                ScrollStrategy::Center,
                self.sticky_items_count,
            );
            cx.notify();
        }
    }
}
