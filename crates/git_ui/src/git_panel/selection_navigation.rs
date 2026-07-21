use super::*;

impl GitPanel {
    pub(super) fn close_panel(&mut self, _: &Close, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PanelEvent::Close);
    }

    pub(super) fn focus_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus_handle.contains_focused(window, cx) {
            cx.emit(Event::Focus);
        }
        if self.active_tab == GitPanelTab::History && self.focused_history_entry.is_some() {
            self.history_keyboard_nav = true;
            cx.notify();
        }
    }

    pub(super) fn scroll_to_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(selected_entry) = self.selected_entry else {
            cx.notify();
            return;
        };

        let visible_index = match &self.view_mode {
            GitPanelViewMode::Flat => Some(selected_entry),
            GitPanelViewMode::Tree(state) => state
                .logical_indices
                .iter()
                .position(|&ix| ix == selected_entry),
        };

        if let Some(visible_index) = visible_index {
            self.scroll_handle
                .scroll_to_item(visible_index, ScrollStrategy::Center);
        }

        cx.notify();
    }

    pub(super) fn expand_selected_entry(
        &mut self,
        _: &ExpandSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.get_selected_entry().cloned() else {
            return;
        };

        if let GitListEntry::Directory(dir_entry) = entry {
            if dir_entry.expanded {
                self.select_next(&menu::SelectNext, window, cx);
            } else {
                self.toggle_directory(&dir_entry.key, window, cx);
            }
        } else {
            self.select_next(&menu::SelectNext, window, cx);
        }
    }

    pub(super) fn collapse_selected_entry(
        &mut self,
        _: &CollapseSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.get_selected_entry().cloned() else {
            return;
        };

        if let GitListEntry::Directory(dir_entry) = entry {
            if dir_entry.expanded {
                self.toggle_directory(&dir_entry.key, window, cx);
            } else {
                self.select_previous(&menu::SelectPrevious, window, cx);
            }
        } else {
            self.select_previous(&menu::SelectPrevious, window, cx);
        }
    }

    pub(super) fn select_first(
        &mut self,
        _: &menu::SelectFirst,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let first_entry = match &self.view_mode {
            GitPanelViewMode::Flat => self
                .entries
                .iter()
                .position(|entry| entry.status_entry().is_some()),
            GitPanelViewMode::Tree(state) => {
                let index = self.entries.iter().position(|entry| {
                    entry.status_entry().is_some() || entry.directory_entry().is_some()
                });

                index.map(|index| state.logical_indices[index])
            }
        };

        if let Some(first_entry) = first_entry {
            self.selected_entry = Some(first_entry);
            self.scroll_to_selected_entry(cx);
        }
    }

    pub(super) fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab == GitPanelTab::History {
            self.select_previous_history_entry(cx);
            return;
        }

        let item_count = self.entries.len();
        if item_count == 0 {
            return;
        }

        let Some(selected_entry) = self.selected_entry else {
            return;
        };

        let new_index = match &self.view_mode {
            GitPanelViewMode::Flat => selected_entry.saturating_sub(1),
            GitPanelViewMode::Tree(state) => {
                let Some(current_logical_index) = state
                    .logical_indices
                    .iter()
                    .position(|&i| i == selected_entry)
                else {
                    return;
                };

                state.logical_indices[current_logical_index.saturating_sub(1)]
            }
        };

        if selected_entry == 0 && new_index == 0 {
            return;
        }

        if matches!(
            self.entries.get(new_index.saturating_sub(1)),
            Some(GitListEntry::Header(..))
        ) && new_index == 0
        {
            return;
        }

        if matches!(self.entries.get(new_index), Some(GitListEntry::Header(..))) {
            self.selected_entry = match &self.view_mode {
                GitPanelViewMode::Flat => Some(new_index.saturating_sub(1)),
                GitPanelViewMode::Tree(tree_view_state) => {
                    maybe!({
                        let current_logical_index = tree_view_state
                            .logical_indices
                            .iter()
                            .position(|&i| i == new_index)?;

                        tree_view_state
                            .logical_indices
                            .get(current_logical_index.saturating_sub(1))
                            .copied()
                    })
                }
            };
        } else {
            self.selected_entry = Some(new_index);
        }

        self.scroll_to_selected_entry(cx);
    }

    pub(super) fn select_next(
        &mut self,
        _: &menu::SelectNext,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_tab == GitPanelTab::History {
            self.select_next_history_entry(cx);
            return;
        }

        let item_count = self.entries.len();
        if item_count == 0 {
            return;
        }

        let Some(selected_entry) = self.selected_entry else {
            return;
        };

        let new_index = match &self.view_mode {
            GitPanelViewMode::Flat => {
                if selected_entry >= item_count.saturating_sub(1) {
                    return;
                }

                selected_entry.saturating_add(1)
            }
            GitPanelViewMode::Tree(state) => {
                let Some(current_logical_index) = state
                    .logical_indices
                    .iter()
                    .position(|&i| i == selected_entry)
                else {
                    return;
                };

                let Some(new_index) = state
                    .logical_indices
                    .get(current_logical_index.saturating_add(1))
                    .copied()
                else {
                    return;
                };

                new_index
            }
        };

        if matches!(self.entries.get(new_index), Some(GitListEntry::Header(..))) {
            self.selected_entry = Some(new_index.saturating_add(1));
        } else {
            self.selected_entry = Some(new_index);
        }

        self.scroll_to_selected_entry(cx);
    }

    pub(super) fn select_last(
        &mut self,
        _: &menu::SelectLast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.entries.last().is_some() {
            self.selected_entry = Some(self.entries.len() - 1);
            self.scroll_to_selected_entry(cx);
        }
    }

    /// Show diff view at selected entry, only if the diff view is open
    pub(super) fn move_diff_to_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        maybe!({
            let workspace = self.workspace.upgrade()?;

            if let Some(project_diff) = workspace.read(cx).item_of_type::<ProjectDiff>(cx) {
                let entry = self.entries.get(self.selected_entry?)?.status_entry()?;

                project_diff.update(cx, |project_diff, cx| {
                    project_diff.move_to_entry(entry.clone(), window, cx);
                });
            }

            Some(())
        });
    }

    pub(super) fn first_entry(
        &mut self,
        _: &FirstEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_first(&menu::SelectFirst, window, cx);
        self.move_diff_to_entry(window, cx);
    }

    pub(super) fn last_entry(
        &mut self,
        _: &LastEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_last(&menu::SelectLast, window, cx);
        self.move_diff_to_entry(window, cx);
    }

    pub(super) fn next_entry(
        &mut self,
        _: &NextEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_next(&menu::SelectNext, window, cx);
        self.move_diff_to_entry(window, cx);
    }

    pub(super) fn previous_entry(
        &mut self,
        _: &PreviousEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_previous(&menu::SelectPrevious, window, cx);
        self.move_diff_to_entry(window, cx);
    }

    pub(super) fn focus_editor(
        &mut self,
        _: &FocusEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_editor.update(cx, |editor, cx| {
            window.focus(&editor.focus_handle(cx), cx);
        });
        cx.notify();
    }

    pub(super) fn select_first_entry_if_none(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let have_entries = self
            .active_repository
            .as_ref()
            .is_some_and(|active_repository| active_repository.read(cx).status_summary().count > 0);
        if have_entries && self.selected_entry.is_none() {
            self.select_first(&menu::SelectFirst, window, cx);
        }
    }

    pub(super) fn select_last_entry_if_out_of_bounds(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(idx) = self.selected_entry
            && idx >= self.entries.len()
        {
            self.select_last(&menu::SelectLast, window, cx);
        }
    }

    pub(super) fn focus_changes_list(
        &mut self,
        _: &FocusChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.select_first_entry_if_none(window, cx);
    }

    pub(super) fn get_selected_entry(&self) -> Option<&GitListEntry> {
        self.selected_entry.and_then(|i| self.entries.get(i))
    }
}
