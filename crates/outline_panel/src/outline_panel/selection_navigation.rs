use super::*;

impl OutlinePanel {
    pub(super) fn unfold_directory(
        &mut self,
        _: &UnfoldDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(PanelEntry::FoldedDirs(FoldedDirsEntry {
            worktree_id,
            entries,
            ..
        })) = self.selected_entry().cloned()
        {
            self.unfolded_dirs
                .entry(worktree_id)
                .or_default()
                .extend(entries.iter().map(|entry| entry.id));
            self.update_cached_entries(None, window, cx);
        }
    }

    pub(super) fn fold_directory(
        &mut self,
        _: &FoldDirectory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (worktree_id, entry) = match self.selected_entry().cloned() {
            Some(PanelEntry::Fs(FsEntry::Directory(directory))) => {
                (directory.worktree_id, Some(directory.entry))
            }
            Some(PanelEntry::FoldedDirs(folded_dirs)) => {
                (folded_dirs.worktree_id, folded_dirs.entries.last().cloned())
            }
            _ => return,
        };
        let Some(entry) = entry else {
            return;
        };
        let unfolded_dirs = self.unfolded_dirs.get_mut(&worktree_id);
        let worktree = self
            .project
            .read(cx)
            .worktree_for_id(worktree_id, cx)
            .map(|w| w.read(cx).snapshot());
        let Some((_, unfolded_dirs)) = worktree.zip(unfolded_dirs) else {
            return;
        };

        unfolded_dirs.remove(&entry.id);
        self.update_cached_entries(None, window, cx);
    }

    pub(super) fn open_selected_entry(
        &mut self,
        _: &OpenSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.filter_editor.focus_handle(cx).is_focused(window) {
            cx.propagate()
        } else if let Some(selected_entry) = self.selected_entry().cloned() {
            self.scroll_editor_to_entry(&selected_entry, true, true, window, cx);
        }
    }

    pub(super) fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter_editor.focus_handle(cx).is_focused(window) {
            self.focus_handle.focus(window, cx);
        } else {
            self.filter_editor.focus_handle(cx).focus(window, cx);
        }

        if self.context_menu.is_some() {
            self.context_menu.take();
            cx.notify();
        }
    }

    pub(super) fn open_excerpts(
        &mut self,
        action: &editor::actions::OpenExcerpts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.filter_editor.focus_handle(cx).is_focused(window) {
            cx.propagate()
        } else if let Some((active_editor, selected_entry)) =
            self.active_editor().zip(self.selected_entry().cloned())
        {
            self.scroll_editor_to_entry(&selected_entry, true, true, window, cx);
            active_editor.update(cx, |editor, cx| editor.open_excerpts(action, window, cx));
        }
    }

    pub(super) fn open_excerpts_split(
        &mut self,
        action: &editor::actions::OpenExcerptsSplit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.filter_editor.focus_handle(cx).is_focused(window) {
            cx.propagate()
        } else if let Some((active_editor, selected_entry)) =
            self.active_editor().zip(self.selected_entry().cloned())
        {
            self.scroll_editor_to_entry(&selected_entry, true, true, window, cx);
            active_editor.update(cx, |editor, cx| {
                editor.open_excerpts_in_split(action, window, cx)
            });
        }
    }

    pub(super) fn scroll_editor_to_entry(
        &mut self,
        entry: &PanelEntry,
        prefer_selection_change: bool,
        prefer_focus_change: bool,
        window: &mut Window,
        cx: &mut Context<OutlinePanel>,
    ) {
        let Some(active_editor) = self.active_editor() else {
            return;
        };
        let active_multi_buffer = active_editor.read(cx).buffer().clone();
        let multi_buffer_snapshot = active_multi_buffer.read(cx).snapshot(cx);
        let mut change_selection = prefer_selection_change;
        let mut change_focus = prefer_focus_change;
        let mut scroll_to_buffer = None;
        let scroll_target = match entry {
            PanelEntry::FoldedDirs(..) | PanelEntry::Fs(FsEntry::Directory(..)) => {
                change_focus = false;
                None
            }
            PanelEntry::Fs(FsEntry::ExternalFile(file)) => {
                change_selection = false;
                scroll_to_buffer = Some(file.buffer_id);
                multi_buffer_snapshot.excerpts().find_map(|excerpt_range| {
                    if excerpt_range.context.start.buffer_id == file.buffer_id {
                        multi_buffer_snapshot.anchor_in_excerpt(excerpt_range.context.start)
                    } else {
                        None
                    }
                })
            }

            PanelEntry::Fs(FsEntry::File(file)) => {
                change_selection = false;
                scroll_to_buffer = Some(file.buffer_id);
                self.project
                    .update(cx, |project, cx| {
                        project
                            .path_for_entry(file.entry.id, cx)
                            .and_then(|path| project.get_open_buffer(&path, cx))
                    })
                    .map(|buffer| {
                        multi_buffer_snapshot.excerpts_for_buffer(buffer.read(cx).remote_id())
                    })
                    .and_then(|mut excerpts| {
                        let excerpt_range = excerpts.next()?;
                        multi_buffer_snapshot.anchor_in_excerpt(excerpt_range.context.start)
                    })
            }
            PanelEntry::Outline(OutlineEntry::Outline(outline)) => multi_buffer_snapshot
                .anchor_in_excerpt(outline.range.start)
                .or_else(|| multi_buffer_snapshot.anchor_in_excerpt(outline.range.end)),
            PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => {
                change_selection = false;
                change_focus = false;
                multi_buffer_snapshot.anchor_in_excerpt(excerpt.context.start)
            }
            PanelEntry::Search(search_entry) => Some(search_entry.match_range.start),
        };

        if let Some(anchor) = scroll_target {
            let activate = self
                .workspace
                .update(cx, |workspace, cx| match self.active_item() {
                    Some(active_item) => workspace.activate_item(
                        active_item.as_ref(),
                        true,
                        change_focus,
                        window,
                        cx,
                    ),
                    None => workspace.activate_item(&active_editor, true, change_focus, window, cx),
                });

            if activate.is_ok() {
                self.select_entry(entry.clone(), true, window, cx);
                if change_selection {
                    active_editor.update(cx, |editor, cx| {
                        editor.change_selections(
                            SelectionEffects::scroll(Autoscroll::center()),
                            window,
                            cx,
                            |s| s.select_ranges(Some(anchor..anchor)),
                        );
                    });
                } else {
                    let mut offset = Point::default();
                    if let Some(buffer_id) = scroll_to_buffer
                        && multi_buffer_snapshot.as_singleton().is_none()
                        && !active_editor.read(cx).is_buffer_folded(buffer_id, cx)
                    {
                        offset.y = -(active_editor.read(cx).file_header_size() as f64);
                    }

                    active_editor.update(cx, |editor, cx| {
                        editor.set_scroll_anchor(ScrollAnchor { offset, anchor }, window, cx);
                    });
                }

                if change_focus {
                    active_editor.focus_handle(cx).focus(window, cx);
                } else {
                    self.focus_handle.focus(window, cx);
                }
            }
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
        if let Some(selected_entry) = self.selected_entry() {
            let index = self
                .cached_entries
                .iter()
                .position(|cached_entry| &cached_entry.entry == selected_entry);
            if let Some(index) = index {
                self.scroll_handle
                    .scroll_to_item_strict(index, ScrollStrategy::Center);
                cx.notify();
            }
        }
    }

    pub(super) fn scroll_cursor_top(
        &mut self,
        _: &ScrollCursorTop,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selected_entry) = self.selected_entry() {
            let index = self
                .cached_entries
                .iter()
                .position(|cached_entry| &cached_entry.entry == selected_entry);
            if let Some(index) = index {
                self.scroll_handle
                    .scroll_to_item_strict(index, ScrollStrategy::Top);
                cx.notify();
            }
        }
    }

    pub(super) fn scroll_cursor_bottom(
        &mut self,
        _: &ScrollCursorBottom,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selected_entry) = self.selected_entry() {
            let index = self
                .cached_entries
                .iter()
                .position(|cached_entry| &cached_entry.entry == selected_entry);
            if let Some(index) = index {
                self.scroll_handle
                    .scroll_to_item_strict(index, ScrollStrategy::Bottom);
                cx.notify();
            }
        }
    }

    pub(super) fn select_next(
        &mut self,
        _: &SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry_to_select) = self.selected_entry().and_then(|selected_entry| {
            self.cached_entries
                .iter()
                .map(|cached_entry| &cached_entry.entry)
                .skip_while(|entry| entry != &selected_entry)
                .nth(1)
                .cloned()
        }) {
            self.select_entry(entry_to_select, true, window, cx);
        } else {
            self.select_first(&SelectFirst {}, window, cx)
        }
        if let Some(selected_entry) = self.selected_entry().cloned() {
            self.scroll_editor_to_entry(&selected_entry, true, false, window, cx);
        }
    }

    pub(super) fn select_previous(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry_to_select) = self.selected_entry().and_then(|selected_entry| {
            self.cached_entries
                .iter()
                .rev()
                .map(|cached_entry| &cached_entry.entry)
                .skip_while(|entry| entry != &selected_entry)
                .nth(1)
                .cloned()
        }) {
            self.select_entry(entry_to_select, true, window, cx);
        } else {
            self.select_last(&SelectLast, window, cx)
        }
        if let Some(selected_entry) = self.selected_entry().cloned() {
            self.scroll_editor_to_entry(&selected_entry, true, false, window, cx);
        }
    }

    pub(super) fn select_parent(
        &mut self,
        _: &SelectParent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry_to_select) = self.selected_entry().and_then(|selected_entry| {
            let mut previous_entries = self
                .cached_entries
                .iter()
                .rev()
                .map(|cached_entry| &cached_entry.entry)
                .skip_while(|entry| entry != &selected_entry)
                .skip(1);
            match &selected_entry {
                PanelEntry::Fs(fs_entry) => match fs_entry {
                    FsEntry::ExternalFile(..) => None,
                    FsEntry::File(FsEntryFile {
                        worktree_id, entry, ..
                    })
                    | FsEntry::Directory(FsEntryDirectory {
                        worktree_id, entry, ..
                    }) => entry.path.parent().and_then(|parent_path| {
                        previous_entries.find(|entry| match entry {
                            PanelEntry::Fs(FsEntry::Directory(directory)) => {
                                directory.worktree_id == *worktree_id
                                    && directory.entry.path.as_ref() == parent_path
                            }
                            PanelEntry::FoldedDirs(FoldedDirsEntry {
                                worktree_id: dirs_worktree_id,
                                entries: dirs,
                                ..
                            }) => {
                                dirs_worktree_id == worktree_id
                                    && dirs
                                        .last()
                                        .is_some_and(|dir| dir.path.as_ref() == parent_path)
                            }
                            _ => false,
                        })
                    }),
                },
                PanelEntry::FoldedDirs(folded_dirs) => folded_dirs
                    .entries
                    .first()
                    .and_then(|entry| entry.path.parent())
                    .and_then(|parent_path| {
                        previous_entries.find(|entry| {
                            if let PanelEntry::Fs(FsEntry::Directory(directory)) = entry {
                                directory.worktree_id == folded_dirs.worktree_id
                                    && directory.entry.path.as_ref() == parent_path
                            } else {
                                false
                            }
                        })
                    }),
                PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) => {
                    previous_entries.find(|entry| match entry {
                        PanelEntry::Fs(FsEntry::File(file)) => {
                            file.buffer_id == excerpt.context.start.buffer_id
                                && file.excerpts.contains(&excerpt)
                        }
                        PanelEntry::Fs(FsEntry::ExternalFile(external_file)) => {
                            external_file.buffer_id == excerpt.context.start.buffer_id
                                && external_file.excerpts.contains(&excerpt)
                        }
                        _ => false,
                    })
                }
                PanelEntry::Outline(OutlineEntry::Outline(outline)) => {
                    previous_entries.find(|entry| {
                        if let PanelEntry::Outline(OutlineEntry::Excerpt(excerpt)) = entry {
                            if outline.range.start.buffer_id != excerpt.context.start.buffer_id {
                                return false;
                            }
                            let Some(buffer_snapshot) =
                                self.buffer_snapshot_for_id(outline.range.start.buffer_id, cx)
                            else {
                                return false;
                            };
                            excerpt.contains(&outline.range.start, &buffer_snapshot)
                                || excerpt.contains(&outline.range.end, &buffer_snapshot)
                        } else {
                            false
                        }
                    })
                }
                PanelEntry::Search(_) => {
                    previous_entries.find(|entry| !matches!(entry, PanelEntry::Search(_)))
                }
            }
        }) {
            self.select_entry(entry_to_select.clone(), true, window, cx);
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
        if let Some(first_entry) = self.cached_entries.first() {
            self.select_entry(first_entry.entry.clone(), true, window, cx);
        }
    }

    pub(super) fn select_last(
        &mut self,
        _: &SelectLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(new_selection) = self
            .cached_entries
            .iter()
            .rev()
            .map(|cached_entry| &cached_entry.entry)
            .next()
        {
            self.select_entry(new_selection.clone(), true, window, cx);
        }
    }

    pub(super) fn autoscroll(&mut self, cx: &mut Context<Self>) {
        if let Some(selected_entry) = self.selected_entry() {
            let index = self
                .cached_entries
                .iter()
                .position(|cached_entry| &cached_entry.entry == selected_entry);
            if let Some(index) = index {
                self.scroll_handle
                    .scroll_to_item(index, ScrollStrategy::Center);
                cx.notify();
            }
        }
    }

    pub(super) fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.contains_focused(window, cx) {
            cx.emit(Event::Focus);
        }
    }
}
