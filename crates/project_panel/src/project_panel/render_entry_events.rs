use super::*;

impl ProjectPanel {
    pub(super) fn handle_rendered_entry_click(
        &mut self,
        event: &gpui::ClickEvent,
        entry_id: ProjectEntryId,
        worktree_id: WorktreeId,
        kind: EntryKind,
        is_sticky: bool,
        sticky_index: Option<usize>,
        show_editor: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_right_click() || show_editor {
            return;
        }
        if event.standard_click() {
            self.mouse_down = false;
        }
        cx.stop_propagation();

        if let Some(selection) = self.selection.filter(|_| event.modifiers().shift) {
            let current_selection = self.index_for_selection(selection);
            let clicked_entry = SelectedEntry {
                entry_id,
                worktree_id,
            };
            let target_selection = self.index_for_selection(clicked_entry);
            if let Some(((_, _, source_index), (_, _, target_index))) =
                current_selection.zip(target_selection)
            {
                let range_start = source_index.min(target_index);
                let range_end = source_index.max(target_index) + 1;
                let mut new_selections = Vec::new();
                self.for_each_visible_entry(
                    range_start..range_end,
                    window,
                    cx,
                    &mut |entry_id, details, _, _| {
                        new_selections.push(SelectedEntry {
                            entry_id,
                            worktree_id: details.worktree_id,
                        });
                    },
                );

                for selection in &new_selections {
                    if !self.marked_entries.contains(selection) {
                        self.marked_entries.push(*selection);
                    }
                }

                self.selection = Some(clicked_entry);
                if !self.marked_entries.contains(&clicked_entry) {
                    self.marked_entries.push(clicked_entry);
                }
            }
        } else if event.modifiers().secondary() {
            if event.click_count() > 1 {
                self.split_entry(entry_id, false, None, cx);
            } else {
                let selection = SelectedEntry {
                    entry_id,
                    worktree_id,
                };
                self.selection = Some(selection);
                if let Some(position) = self.marked_entries.iter().position(|e| *e == selection) {
                    self.marked_entries.remove(position);
                } else {
                    self.marked_entries.push(selection);
                }
            }
        } else if kind.is_dir() {
            self.marked_entries.clear();
            if is_sticky && let Some((_, _, index)) = self.index_for_entry(entry_id, worktree_id) {
                self.scroll_handle.scroll_to_item_strict_with_offset(
                    index,
                    ScrollStrategy::Top,
                    sticky_index.unwrap_or(0),
                );
                cx.notify();
                // move down by 1px so that clicked item
                // don't count as sticky anymore
                cx.on_next_frame(window, |_, window, cx| {
                    cx.on_next_frame(window, |this, _, cx| {
                        let mut offset = this.scroll_handle.offset();
                        offset.y += px(1.);
                        this.scroll_handle.set_offset(offset);
                        cx.notify();
                    });
                });
                return;
            }
            if event.modifiers().alt {
                self.toggle_expand_all(entry_id, window, cx);
            } else {
                self.toggle_expanded(entry_id, window, cx);
            }
        } else {
            let preview_tabs_enabled =
                PreviewTabsSettings::get_global(cx).enable_preview_from_project_panel;
            let click_count = event.click_count();
            let focus_opened_item = click_count > 1;
            let allow_preview = preview_tabs_enabled && click_count == 1;
            self.open_entry(entry_id, focus_opened_item, allow_preview, cx);
        }
    }
}
