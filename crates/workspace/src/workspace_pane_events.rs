use super::*;

impl Workspace {
    pub(crate) fn handle_pane_focused(
        &mut self,
        pane: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.flush_deferred_saves(window, cx);

        // This is explicitly hoisted out of the following check for pane identity as
        // terminal panel panes are not registered as a center panes.
        self.status_bar.update(cx, |status_bar, cx| {
            status_bar.set_active_pane(&pane, window, cx);
        });
        if self.active_pane != pane {
            self.set_active_pane(&pane, window, cx);
        }

        if self.last_active_center_pane.is_none() && self.pane_is_in_center(&pane) {
            self.last_active_center_pane = Some(pane.downgrade());
        }

        // If this pane is in a dock, preserve that dock when dismissing zoomed items.
        // This prevents the dock from closing when focus events fire during window activation.
        // We also preserve any dock whose active panel itself has focus — this covers
        // panels like AgentPanel that don't implement `pane()` but can still be zoomed.
        let dock_to_preserve = self.all_docks().iter().find_map(|dock| {
            let dock_read = dock.read(cx);
            if let Some(panel) = dock_read.active_panel() {
                if panel.pane(cx).is_some_and(|dock_pane| dock_pane == pane)
                    || panel.panel_focus_handle(cx).contains_focused(window, cx)
                {
                    return Some(dock_read.position());
                }
            }
            None
        });

        self.dismiss_zoomed_items_to_reveal(dock_to_preserve, window, cx);
        if pane.read(cx).is_zoomed() {
            self.zoomed = Some(pane.downgrade().into());
        } else {
            self.zoomed = None;
        }
        self.zoomed_position = None;
        cx.emit(Event::ZoomChanged);
        self.update_active_view_for_followers(window, cx);
        pane.update(cx, |pane, _| {
            pane.track_alternate_file_items();
        });

        cx.notify();
    }

    pub(crate) fn set_active_pane(
        &mut self,
        pane: &Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_pane = pane.clone();
        self.active_item_path_changed(true, window, cx);
        if self.pane_is_in_center(pane) {
            self.last_active_center_pane = Some(pane.downgrade());
        }
    }

    pub(crate) fn handle_panel_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_deferred_saves(window, cx);
        self.update_active_view_for_followers(window, cx);
    }

    pub(crate) fn flush_deferred_saves(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let deferred = std::mem::take(&mut self.deferred_save_items);
        for weak_item in deferred {
            let Some(item) = weak_item.upgrade() else {
                continue;
            };
            // Skip if focus returned to this item
            let focus_handle = item.item_focus_handle(cx);
            if focus_handle.contains_focused(window, cx) {
                continue;
            }
            Pane::autosave_item(item.as_ref(), self.project.clone(), window, cx)
                .detach_and_log_err(cx);
        }
    }

    pub(crate) fn handle_pane_event(
        &mut self,
        pane: &Entity<Pane>,
        event: &pane::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut serialize_workspace = true;
        match event {
            pane::Event::AddItem { item } => {
                item.added_to_pane(self, pane.clone(), window, cx);
                cx.emit(Event::ItemAdded {
                    item: item.boxed_clone(),
                });
            }
            pane::Event::Split { direction, mode } => {
                match mode {
                    SplitMode::ClonePane => {
                        self.split_and_clone(pane.clone(), *direction, window, cx)
                            .detach();
                    }
                    SplitMode::EmptyPane => {
                        self.split_pane(pane.clone(), *direction, window, cx);
                    }
                    SplitMode::MovePane => {
                        self.split_and_move(pane.clone(), *direction, window, cx);
                    }
                };
            }
            pane::Event::JoinIntoNext => {
                self.join_pane_into_next(pane.clone(), window, cx);
            }
            pane::Event::JoinAll => {
                self.join_all_panes(window, cx);
            }
            pane::Event::Remove { focus_on_pane } => {
                self.remove_pane(pane.clone(), focus_on_pane.clone(), window, cx);
            }
            pane::Event::ActivateItem {
                local,
                focus_changed,
            } => {
                window.invalidate_character_coordinates();

                pane.update(cx, |pane, _| {
                    pane.track_alternate_file_items();
                });
                if *local {
                    self.unfollow_in_pane(pane, window, cx);
                }
                serialize_workspace = *focus_changed || pane != self.active_pane();
                if pane == self.active_pane() {
                    self.active_item_path_changed(*focus_changed, window, cx);
                    self.update_active_view_for_followers(window, cx);
                } else if *local {
                    self.set_active_pane(pane, window, cx);
                }
            }
            pane::Event::UserSavedItem { item, save_intent } => {
                cx.emit(Event::UserSavedItem {
                    pane: pane.downgrade(),
                    item: item.boxed_clone(),
                    save_intent: *save_intent,
                });
                serialize_workspace = false;
            }
            pane::Event::ChangeItemTitle => {
                if *pane == self.active_pane {
                    self.active_item_path_changed(false, window, cx);
                }
                serialize_workspace = false;
            }
            pane::Event::RemovedItem { item } => {
                cx.emit(Event::ActiveItemChanged);
                self.update_window_edited(window, cx);
                if let hash_map::Entry::Occupied(entry) = self.panes_by_item.entry(item.item_id())
                    && entry.get().entity_id() == pane.entity_id()
                {
                    entry.remove();
                }
                cx.emit(Event::ItemRemoved {
                    item_id: item.item_id(),
                });
            }
            pane::Event::Focus => {
                window.invalidate_character_coordinates();
                self.handle_pane_focused(pane.clone(), window, cx);
            }
            pane::Event::ZoomIn => {
                if *pane == self.active_pane {
                    pane.update(cx, |pane, cx| pane.set_zoomed(true, cx));
                    if pane.read(cx).has_focus(window, cx) {
                        self.zoomed = Some(pane.downgrade().into());
                        self.zoomed_position = None;
                        cx.emit(Event::ZoomChanged);
                    }
                    cx.notify();
                }
            }
            pane::Event::ZoomOut => {
                pane.update(cx, |pane, cx| pane.set_zoomed(false, cx));
                if self.zoomed_position.is_none() {
                    self.zoomed = None;
                    cx.emit(Event::ZoomChanged);
                }
                cx.notify();
            }
            pane::Event::ItemPinned | pane::Event::ItemUnpinned => {}
        }

        if serialize_workspace {
            self.serialize_workspace(window, cx);
        }
    }

    pub(crate) fn active_item_path_changed(
        &mut self,
        focus_changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(Event::ActiveItemChanged);
        let active_entry = self.active_project_path(cx);
        self.project.update(cx, |project, cx| {
            project.set_active_path(active_entry.clone(), cx)
        });

        if focus_changed && let Some(project_path) = &active_entry {
            let git_store_entity = self.project.read(cx).git_store().clone();
            git_store_entity.update(cx, |git_store, cx| {
                git_store.set_active_repo_for_path(project_path, cx);
            });
        }

        self.update_window_title(window, cx);
    }

    pub fn on_window_activation_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.is_window_active() {
            self.update_active_view_for_followers(window, cx);

            if let Some(database_id) = self.database_id {
                let db = WorkspaceDb::global(cx);
                cx.background_spawn(async move { db.update_timestamp(database_id).await })
                    .detach();
            }
        } else {
            // When window is deactivated, flush any deferred saves since focus has left the window
            self.flush_deferred_saves(window, cx);
            for pane in &self.panes {
                pane.update(cx, |pane, cx| {
                    if let Some(item) = pane.active_item() {
                        item.workspace_deactivated(window, cx);
                    }
                    for item in pane.items() {
                        if matches!(
                            item.workspace_settings(cx).autosave,
                            AutosaveSetting::OnWindowChange | AutosaveSetting::OnFocusChange
                        ) {
                            Pane::autosave_item(item.as_ref(), self.project.clone(), window, cx)
                                .detach_and_log_err(cx);
                        }
                    }
                });
            }
        }
    }
}
