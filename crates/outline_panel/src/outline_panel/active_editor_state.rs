use super::*;

impl OutlinePanel {
    pub(super) fn replace_active_editor(
        &mut self,
        new_active_item: Box<dyn ItemHandle>,
        new_active_editor: Entity<Editor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_previous(window, cx);

        let default_expansion_depth =
            OutlinePanelSettings::get_global(cx).expand_outlines_with_depth;
        // We'll apply the expansion depth after outlines are loaded
        self.pending_default_expansion_depth = Some(default_expansion_depth);

        let buffer_search_subscription = cx.subscribe_in(
            &new_active_editor,
            window,
            |outline_panel: &mut Self,
             _,
             e: &SearchEvent,
             window: &mut Window,
             cx: &mut Context<Self>| {
                if matches!(e, SearchEvent::MatchesInvalidated)
                    && outline_panel.update_search_matches(window, cx)
                {
                    outline_panel.selected_entry.invalidate();
                    outline_panel.update_cached_entries(Some(UPDATE_DEBOUNCE), window, cx);
                }
            },
        );
        self.active_item = Some(ActiveItem {
            _buffer_search_subscription: buffer_search_subscription,
            _editor_subscription: subscribe_for_editor_events(&new_active_editor, window, cx),
            item_handle: new_active_item.downgrade_item(),
            active_editor: new_active_editor.downgrade(),
        });
        self.new_entries_for_fs_update.extend(
            new_active_editor
                .read(cx)
                .buffer()
                .read(cx)
                .snapshot(cx)
                .excerpts()
                .map(|excerpt| excerpt.context.start.buffer_id),
        );
        self.selected_entry.invalidate();
        self.update_fs_entries(new_active_editor, None, window, cx);
    }

    pub(super) fn clear_previous(&mut self, window: &mut Window, cx: &mut App) {
        self.fs_entries_update_task = Task::ready(());
        self.fs_entries_update_pending = false;
        self.outline_fetch_tasks.clear();
        self.cached_entries_update_task = Task::ready(());
        self.cached_entries_update_pending = false;
        self.reveal_selection_task = Task::ready(Ok(()));
        self.filter_editor
            .update(cx, |editor, cx| editor.clear(window, cx));
        self.collapsed_entries.clear();
        self.unfolded_dirs.clear();
        self.active_item = None;
        self.fs_entries.clear();
        self.fs_entries_depth.clear();
        self.fs_children_count.clear();
        self.buffers.clear();
        self.cached_entries = Vec::new();
        self.selected_entry = SelectedEntry::None;
        self.pinned = false;
        self.mode = ItemsDisplayMode::Outline;
        self.pending_default_expansion_depth = None;
    }

    pub(super) fn active_editor(&self) -> Option<Entity<Editor>> {
        self.active_item.as_ref()?.active_editor.upgrade()
    }

    pub(super) fn active_item(&self) -> Option<Box<dyn ItemHandle>> {
        self.active_item.as_ref()?.item_handle.upgrade()
    }

    pub(super) fn should_replace_active_item(&self, new_active_item: &dyn ItemHandle) -> bool {
        self.active_item().is_none_or(|active_item| {
            !self.pinned && active_item.item_id() != new_active_item.item_id()
        })
    }

    pub(super) fn toggle_active_editor_pin(
        &mut self,
        _: &ToggleActiveEditorPin,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pinned = !self.pinned;
        if !self.pinned
            && let Some((active_item, active_editor)) = self
                .workspace
                .upgrade()
                .and_then(|workspace| workspace_active_editor(workspace.read(cx), cx))
            && self.should_replace_active_item(active_item.as_ref())
        {
            self.replace_active_editor(active_item, active_editor, window, cx);
        }

        cx.notify();
    }
}
