use super::*;

impl Pane {
    pub fn handle_tab_drop(
        &mut self,
        dragged_tab: &DraggedTab,
        ix: usize,
        is_pane_target: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_pane_target && !self.can_drop_on_body(dragged_tab, window, cx) {
            self.clear_drag_drop_target(cx);
            return;
        }

        if !self.is_tabbed() && self.drag_split_direction.is_none() {
            return;
        }

        if is_pane_target
            && ix == self.active_item_index
            && let Some(active_item) = self.active_item()
            && active_item.handle_drop(self, dragged_tab, window, cx)
        {
            self.clear_drag_drop_target(cx);
            return;
        }

        let mut to_pane = cx.entity();
        let split_direction = self.take_drag_split_direction();
        let item_id = dragged_tab.item.item_id();
        self.unpreview_item_if_preview(item_id);

        let is_clone = cfg!(target_os = "macos") && window.modifiers().alt
            || cfg!(not(target_os = "macos")) && window.modifiers().control;

        let from_pane = dragged_tab.pane.clone();

        self.workspace
            .update(cx, |_, cx| {
                cx.defer_in(window, move |workspace, window, cx| {
                    if let Some(split_direction) = split_direction
                        && !is_clone
                        && from_pane != to_pane
                        && from_pane.read_with(cx, |pane, _| {
                            pane.items_len() == 1 && pane.index_for_item_id(item_id).is_some()
                        })
                    {
                        workspace.move_pane_to_pane(
                            from_pane.clone(),
                            to_pane.clone(),
                            Some(split_direction),
                            window,
                            cx,
                        );
                        return;
                    }

                    if let Some(split_direction) = split_direction {
                        to_pane = workspace.split_pane(to_pane, split_direction, window, cx);
                    }
                    let database_id = workspace.database_id();
                    let was_pinned_in_from_pane = from_pane.read_with(cx, |pane, _| {
                        pane.index_for_item_id(item_id)
                            .is_some_and(|ix| pane.is_tab_pinned(ix))
                    });
                    let to_pane_old_length = to_pane.read(cx).items.len();
                    if is_clone {
                        let Some(item) = from_pane
                            .read(cx)
                            .items()
                            .find(|item| item.item_id() == item_id)
                            .cloned()
                        else {
                            return;
                        };
                        if item.can_split(cx) {
                            let task = item.clone_on_split(database_id, window, cx);
                            let to_pane = to_pane.downgrade();
                            cx.spawn_in(window, async move |_, cx| {
                                if let Some(item) = task.await {
                                    to_pane
                                        .update_in(cx, |pane, window, cx| {
                                            pane.add_item(item, true, true, None, window, cx)
                                        })
                                        .ok();
                                }
                            })
                            .detach();
                        } else {
                            move_item(&from_pane, &to_pane, item_id, ix, true, window, cx);
                        }
                    } else {
                        move_item(&from_pane, &to_pane, item_id, ix, true, window, cx);
                    }
                    to_pane.update(cx, |this, _| {
                        if to_pane == from_pane {
                            let actual_ix = this
                                .items
                                .iter()
                                .position(|item| item.item_id() == item_id)
                                .unwrap_or(0);

                            let is_pinned_in_to_pane = this.is_tab_pinned(actual_ix);

                            if !was_pinned_in_from_pane && is_pinned_in_to_pane {
                                this.pinned_tab_count += 1;
                            } else if was_pinned_in_from_pane && !is_pinned_in_to_pane {
                                this.pinned_tab_count -= 1;
                            }
                        } else if this.items.len() >= to_pane_old_length {
                            let is_pinned_in_to_pane = this.is_tab_pinned(ix);
                            let item_created_pane = to_pane_old_length == 0;
                            let is_first_position = ix == 0;
                            let was_dropped_at_beginning = item_created_pane || is_first_position;
                            let should_remain_pinned = is_pinned_in_to_pane
                                || (was_pinned_in_from_pane && was_dropped_at_beginning);

                            if should_remain_pinned {
                                this.pinned_tab_count += 1;
                            }
                        }
                    });
                });
            })
            .log_err();
    }

    pub(super) fn handle_pinned_tab_bar_drop(
        &mut self,
        dragged_tab: &DraggedTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let item_id = dragged_tab.item.item_id();
        let pinned_count = self.pinned_tab_count;

        self.handle_tab_drop(dragged_tab, pinned_count, false, window, cx);

        let to_pane = cx.entity();

        self.workspace
            .update(cx, |_, cx| {
                cx.defer_in(window, move |_, _, cx| {
                    to_pane.update(cx, |this, cx| {
                        if let Some(actual_ix) = this.index_for_item_id(item_id) {
                            // If the tab ended up at or after pinned_tab_count, it's not pinned
                            // so we pin it now
                            if actual_ix >= this.pinned_tab_count {
                                let was_active = this.active_item_index == actual_ix;
                                let destination_ix = this.pinned_tab_count;

                                // Move item to pinned area if needed
                                if actual_ix != destination_ix {
                                    let item = this.items.remove(actual_ix);
                                    this.items.insert(destination_ix, item);

                                    // Update active_item_index to follow the moved item
                                    if was_active {
                                        this.active_item_index = destination_ix;
                                    } else if this.active_item_index > actual_ix
                                        && this.active_item_index <= destination_ix
                                    {
                                        // Item moved left past the active item
                                        this.active_item_index -= 1;
                                    } else if this.active_item_index >= destination_ix
                                        && this.active_item_index < actual_ix
                                    {
                                        // Item moved right past the active item
                                        this.active_item_index += 1;
                                    }
                                }
                                this.pinned_tab_count += 1;
                                cx.notify();
                            }
                        }
                    });
                });
            })
            .log_err();
    }

    pub(super) fn handle_dragged_selection_drop(
        &mut self,
        dragged_selection: &DraggedSelection,
        dragged_onto: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_pane_target = dragged_selection.active_selection_is_file
            && (dragged_onto.is_some()
                || self.drag_split_direction.is_some()
                || self.drag_tab_target);

        if !is_pane_target
            && let Some(active_item) = self.active_item()
            && active_item.handle_drop(self, dragged_selection, window, cx)
        {
            self.clear_drag_drop_target(cx);
            return;
        }

        if (dragged_onto.is_none() && !self.can_drop_on_body(dragged_selection, window, cx))
            || !dragged_selection.active_selection_is_file
        {
            self.clear_drag_drop_target(cx);
            return;
        }

        self.handle_project_entry_drop(
            &dragged_selection.active_selection.entry_id,
            dragged_onto,
            window,
            cx,
        );
    }

    fn handle_project_entry_drop(
        &mut self,
        project_entry_id: &ProjectEntryId,
        target: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_tabbed() && self.drag_split_direction.is_none() {
            return;
        }

        let mut to_pane = cx.entity();
        let split_direction = self.take_drag_split_direction();
        let project_entry_id = *project_entry_id;
        self.workspace
            .update(cx, |_, cx| {
                cx.defer_in(window, move |workspace, window, cx| {
                    if let Some(project_path) = workspace
                        .project()
                        .read(cx)
                        .path_for_entry(project_entry_id, cx)
                    {
                        let load_path_task = workspace.load_path(project_path.clone(), window, cx);
                        cx.spawn_in(window, async move |workspace, mut cx| {
                            if let Some((project_entry_id, build_item)) = load_path_task
                                .await
                                .notify_workspace_async_err(workspace.clone(), &mut cx)
                            {
                                let (to_pane, new_item_handle) = workspace
                                    .update_in(cx, |workspace, window, cx| {
                                        if let Some(split_direction) = split_direction {
                                            to_pane = workspace.split_pane(
                                                to_pane,
                                                split_direction,
                                                window,
                                                cx,
                                            );
                                        }
                                        let new_item_handle = to_pane.update(cx, |pane, cx| {
                                            pane.open_item(
                                                project_entry_id,
                                                project_path,
                                                true,
                                                false,
                                                true,
                                                target,
                                                window,
                                                cx,
                                                build_item,
                                            )
                                        });
                                        (to_pane, new_item_handle)
                                    })
                                    .log_err()?;
                                to_pane
                                    .update_in(cx, |this, window, cx| {
                                        let Some(index) = this.index_for_item(&*new_item_handle)
                                        else {
                                            return;
                                        };

                                        if target.is_some_and(|target| this.is_tab_pinned(target)) {
                                            this.pin_tab_at(index, window, cx);
                                        }
                                    })
                                    .ok()?
                            }
                            Some(())
                        })
                        .detach();
                    };
                });
            })
            .log_err();
    }

    pub(super) fn handle_external_paths_drop(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_drop_on_body(paths, window, cx) {
            self.clear_drag_drop_target(cx);
            return;
        }

        if !self.is_tabbed() && self.drag_split_direction.is_none() {
            return;
        }

        if let Some(active_item) = self.active_item()
            && active_item.handle_drop(self, paths, window, cx)
        {
            self.clear_drag_drop_target(cx);
            return;
        }

        let mut to_pane = cx.entity();
        let mut split_direction = self.take_drag_split_direction();
        let paths = paths.paths().to_vec();
        let is_remote = self
            .workspace
            .update(cx, |workspace, cx| {
                if workspace.project().read(cx).is_via_collab() {
                    workspace.show_error("Cannot drop files on a remote project", cx);
                    true
                } else {
                    false
                }
            })
            .unwrap_or(true);
        if is_remote {
            return;
        }

        self.workspace
            .update(cx, |workspace, cx| {
                let fs = Arc::clone(workspace.project().read(cx).fs());
                cx.spawn_in(window, async move |workspace, cx| {
                    let mut is_file_checks = FuturesUnordered::new();
                    for path in &paths {
                        is_file_checks.push(fs.is_file(path))
                    }
                    let mut has_files_to_open = false;
                    while let Some(is_file) = is_file_checks.next().await {
                        if is_file {
                            has_files_to_open = true;
                            break;
                        }
                    }
                    drop(is_file_checks);
                    if !has_files_to_open {
                        split_direction = None;
                    }

                    if let Ok((open_task, to_pane)) =
                        workspace.update_in(cx, |workspace, window, cx| {
                            if let Some(split_direction) = split_direction {
                                to_pane =
                                    workspace.split_pane(to_pane, split_direction, window, cx);
                            }
                            (
                                workspace.open_paths(
                                    paths,
                                    OpenOptions {
                                        visible: Some(OpenVisible::OnlyDirectories),
                                        ..Default::default()
                                    },
                                    Some(to_pane.downgrade()),
                                    window,
                                    cx,
                                ),
                                to_pane,
                            )
                        })
                    {
                        let opened_items: Vec<_> = open_task.await;
                        _ = workspace.update_in(cx, |workspace, window, cx| {
                            for item in opened_items.into_iter().flatten() {
                                if let Err(e) = item {
                                    workspace.show_error(format!("Error: {e}"), cx);
                                }
                            }
                            if to_pane.read(cx).items_len() == 0 {
                                workspace.remove_pane(to_pane, None, window, cx);
                            }
                        });
                    }
                })
                .detach();
            })
            .log_err();
    }

    pub(super) fn handle_pane_drop(
        &mut self,
        dragged_pane: &DraggedPane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_drop_on_body(dragged_pane, window, cx) {
            self.clear_drag_drop_target(cx);
            return;
        }

        let target_pane = cx.entity();
        let split_direction = self.take_drag_split_direction();
        let pane_to_move = dragged_pane.pane.clone();
        self.workspace
            .update(cx, |_, cx| {
                cx.defer_in(window, move |workspace, window, cx| {
                    workspace.move_pane_to_pane(
                        pane_to_move,
                        target_pane,
                        split_direction,
                        window,
                        cx,
                    );
                });
            })
            .log_err();
    }
}
