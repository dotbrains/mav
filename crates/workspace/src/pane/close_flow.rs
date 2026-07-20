use super::*;

impl Pane {
    pub(super) fn close_items_on_item_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.max_tabs.map(|m| m.get());
        let protect_active_item = false;
        self.close_items_to_target_count(target, protect_active_item, window, cx);
    }

    pub(super) fn close_items_on_settings_change(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = self.max_tabs.map(|m| m.get() + 1);
        // The active item in this case is the settings.json file, which should be protected from being closed
        let protect_active_item = true;
        self.close_items_to_target_count(target, protect_active_item, window, cx);
    }

    pub(super) fn close_items_to_target_count(
        &mut self,
        target_count: Option<usize>,
        protect_active_item: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target_count) = target_count else {
            return;
        };

        let mut index_list = Vec::new();
        let mut items_len = self.items_len();
        let mut indexes: HashMap<EntityId, usize> = HashMap::default();
        let active_ix = self.active_item_index();

        for (index, item) in self.items.iter().enumerate() {
            indexes.insert(item.item_id(), index);
        }

        // Close least recently used items to reach target count.
        // The target count is allowed to be exceeded, as we protect pinned
        // items, dirty items, and sometimes, the active item.
        for entry in self.activation_history.iter() {
            if items_len < target_count {
                break;
            }

            let Some(&index) = indexes.get(&entry.entity_id) else {
                continue;
            };

            if protect_active_item && index == active_ix {
                continue;
            }

            if let Some(true) = self.items.get(index).map(|item| item.is_dirty(cx)) {
                continue;
            }

            if self.is_tab_pinned(index) {
                continue;
            }

            index_list.push(index);
            items_len -= 1;
        }
        // The sort and reverse is necessary since we remove items
        // using their index position, hence removing from the end
        // of the list first to avoid changing indexes.
        index_list.sort_unstable();
        index_list
            .iter()
            .rev()
            .for_each(|&index| self._remove_item(index, false, false, None, window, cx));
    }

    // Usually when you close an item that has unsaved changes, we prompt you to
    // save it. That said, if you still have the buffer open in a different pane
    // we can close this one without fear of losing data.
    pub fn skip_save_on_close(item: &dyn ItemHandle, workspace: &Workspace, cx: &App) -> bool {
        let mut dirty_project_item_ids = Vec::new();
        item.for_each_project_item(cx, &mut |project_item_id, project_item| {
            if project_item.is_dirty() {
                dirty_project_item_ids.push(project_item_id);
            }
        });
        if dirty_project_item_ids.is_empty() {
            return !(item.buffer_kind(cx) == ItemBufferKind::Singleton && item.is_dirty(cx));
        }

        for open_item in workspace.items(cx) {
            if open_item.item_id() == item.item_id() {
                continue;
            }
            if open_item.buffer_kind(cx) != ItemBufferKind::Singleton {
                continue;
            }
            let other_project_item_ids = open_item.project_item_model_ids(cx);
            dirty_project_item_ids.retain(|id| !other_project_item_ids.contains(id));
        }
        dirty_project_item_ids.is_empty()
    }

    pub(crate) fn file_names_for_prompt(
        items: &mut dyn Iterator<Item = &Box<dyn ItemHandle>>,
        cx: &App,
    ) -> String {
        let mut file_names = BTreeSet::default();
        for item in items {
            item.for_each_project_item(cx, &mut |_, project_item| {
                if !project_item.is_dirty() {
                    return;
                }
                let filename = project_item
                    .project_path(cx)
                    .and_then(|path| path.path.file_name().map(ToOwned::to_owned));
                file_names.insert(filename.unwrap_or("untitled".to_string()));
            });
        }
        if file_names.len() > 6 {
            format!(
                "{}\n.. and {} more",
                file_names.iter().take(5).join("\n"),
                file_names.len() - 5
            )
        } else {
            file_names.into_iter().join("\n")
        }
    }

    pub fn close_items(
        &self,
        window: &mut Window,
        cx: &mut Context<Pane>,
        mut save_intent: SaveIntent,
        should_close: &dyn Fn(EntityId) -> bool,
    ) -> Task<Result<()>> {
        if !self.is_tabbed() {
            return Task::ready(Ok(()));
        }

        // Find the items to close.
        let mut items_to_close = Vec::new();
        for item in &self.items {
            if should_close(item.item_id()) {
                items_to_close.push(item.boxed_clone());
            }
        }

        let active_item_id = self.active_item().map(|item| item.item_id());

        items_to_close.sort_by_key(|item| {
            let path = item.project_path(cx);
            // Put the currently active item at the end, because if the currently active item is not closed last
            // closing the currently active item will cause the focus to switch to another item
            // This will cause Mav to expand the content of the currently active item
            //
            // Beyond that sort in order of project path, with untitled files and multibuffers coming last.
            (active_item_id == Some(item.item_id()), path.is_none(), path)
        });

        let workspace = self.workspace.clone();
        let Some(project) = self.project.upgrade() else {
            return Task::ready(Ok(()));
        };
        cx.spawn_in(window, async move |pane, cx| {
            let dirty_items = workspace.update(cx, |workspace, cx| {
                items_to_close
                    .iter()
                    .filter(|item| {
                        item.is_dirty(cx) && !Self::skip_save_on_close(item.as_ref(), workspace, cx)
                    })
                    .map(|item| item.boxed_clone())
                    .collect::<Vec<_>>()
            })?;

            if save_intent == SaveIntent::Close && dirty_items.len() > 1 {
                let answer = pane.update_in(cx, |_, window, cx| {
                    let detail = Self::file_names_for_prompt(&mut dirty_items.iter(), cx);
                    window.prompt(
                        PromptLevel::Warning,
                        "Do you want to save changes to the following files?",
                        Some(&detail),
                        &["Save all", "Discard all", "Cancel"],
                        cx,
                    )
                })?;
                match answer.await {
                    Ok(0) => save_intent = SaveIntent::SaveAll,
                    Ok(1) => save_intent = SaveIntent::Skip,
                    Ok(2) => return Ok(()),
                    _ => {}
                }
            }

            for item_to_close in items_to_close {
                let mut should_close = true;
                let mut should_save = true;
                if save_intent == SaveIntent::Close {
                    workspace.update(cx, |workspace, cx| {
                        if Self::skip_save_on_close(item_to_close.as_ref(), workspace, cx) {
                            should_save = false;
                        }
                    })?;
                }

                if should_save {
                    match Self::save_item(project.clone(), &pane, &*item_to_close, save_intent, cx)
                        .await
                    {
                        Ok(success) => {
                            if !success {
                                should_close = false;
                            }
                        }
                        Err(err) => {
                            let answer = pane.update_in(cx, |_, window, cx| {
                                let detail = Self::file_names_for_prompt(
                                    &mut [&item_to_close].into_iter(),
                                    cx,
                                );
                                window.prompt(
                                    PromptLevel::Warning,
                                    &format!("Unable to save file: {}", &err),
                                    Some(&detail),
                                    &["Close Without Saving", "Cancel"],
                                    cx,
                                )
                            })?;
                            match answer.await {
                                Ok(0) => {}
                                Ok(1..) | Err(_) => should_close = false,
                            }
                        }
                    }
                }

                if should_close {
                    let close_task =
                        cx.update(|_window, cx| item_to_close.on_close(save_intent, cx))?;
                    should_close = close_task.await?;
                }

                // Remove the item from the pane.
                if should_close {
                    pane.update_in(cx, |pane, window, cx| {
                        pane.remove_item(
                            item_to_close.item_id(),
                            false,
                            pane.close_pane_if_empty,
                            window,
                            cx,
                        );
                    })
                    .ok();
                }
            }

            pane.update(cx, |_, cx| cx.notify()).ok();
            Ok(())
        })
    }

    pub fn take_active_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Box<dyn ItemHandle>> {
        if !self.is_tabbed() {
            return None;
        }

        let item = self.active_item()?;
        self.remove_item(item.item_id(), false, false, window, cx);
        Some(item)
    }

    pub fn remove_item(
        &mut self,
        item_id: EntityId,
        activate_pane: bool,
        close_pane_if_empty: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item_index) = self.index_for_item_id(item_id) else {
            return;
        };
        self._remove_item(
            item_index,
            activate_pane,
            close_pane_if_empty,
            None,
            window,
            cx,
        )
    }

    pub fn remove_item_and_focus_on_pane(
        &mut self,
        item_index: usize,
        activate_pane: bool,
        focus_on_pane_if_closed: Entity<Pane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._remove_item(
            item_index,
            activate_pane,
            true,
            Some(focus_on_pane_if_closed),
            window,
            cx,
        )
    }

    pub(super) fn _remove_item(
        &mut self,
        item_index: usize,
        activate_pane: bool,
        close_pane_if_empty: bool,
        focus_on_pane_if_closed: Option<Entity<Pane>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let activate_on_close = &ItemSettings::get_global(cx).activate_on_close;
        self.activation_history
            .retain(|entry| entry.entity_id != self.items[item_index].item_id());

        if self.is_tab_pinned(item_index) {
            self.pinned_tab_count -= 1;
        }
        if item_index == self.active_item_index {
            let left_neighbour_index = || item_index.min(self.items.len()).saturating_sub(1);
            let index_to_activate = match activate_on_close {
                ActivateOnClose::History => self
                    .activation_history
                    .pop()
                    .and_then(|last_activated_item| {
                        self.items.iter().enumerate().find_map(|(index, item)| {
                            (item.item_id() == last_activated_item.entity_id).then_some(index)
                        })
                    })
                    // We didn't have a valid activation history entry, so fallback
                    // to activating the item to the left
                    .unwrap_or_else(left_neighbour_index),
                ActivateOnClose::Neighbour => {
                    self.activation_history.pop();
                    if item_index + 1 < self.items.len() {
                        item_index + 1
                    } else {
                        item_index.saturating_sub(1)
                    }
                }
                ActivateOnClose::LeftNeighbour => {
                    self.activation_history.pop();
                    left_neighbour_index()
                }
            };

            let should_activate = activate_pane || self.has_focus(window, cx);
            if self.items.len() == 1 && should_activate {
                self.focus_handle.focus(window, cx);
            } else {
                self.activate_item(
                    index_to_activate,
                    should_activate,
                    should_activate,
                    window,
                    cx,
                );
            }
        }

        let item = self.items.remove(item_index);

        cx.emit(Event::RemovedItem { item: item.clone() });
        if self.items.is_empty() {
            item.deactivated(window, cx);
            if close_pane_if_empty {
                self.update_toolbar(window, cx);
                cx.emit(Event::Remove {
                    focus_on_pane: focus_on_pane_if_closed,
                });
            }
        }

        if item_index < self.active_item_index {
            self.active_item_index -= 1;
        }

        let mode = self.nav_history.mode();
        self.nav_history.set_mode(NavigationMode::ClosingItem);
        item.deactivated(window, cx);
        item.on_removed(cx);
        self.nav_history.set_mode(mode);
        self.unpreview_item_if_preview(item.item_id());

        if let Some(path) = item.project_path(cx) {
            let abs_path = self
                .nav_history
                .0
                .lock()
                .paths_by_item
                .get(&item.item_id())
                .and_then(|(_, abs_path)| abs_path.clone());

            self.nav_history
                .0
                .lock()
                .paths_by_item
                .insert(item.item_id(), (path, abs_path));
        } else {
            self.nav_history
                .0
                .lock()
                .paths_by_item
                .remove(&item.item_id());
        }

        if self.zoom_out_on_close && self.items.is_empty() && close_pane_if_empty && self.zoomed {
            cx.emit(Event::ZoomOut);
        }

        cx.notify();
    }
}
