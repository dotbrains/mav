use super::*;

impl ProjectPanel {
    pub(super) fn refresh_drag_cursor_style(
        &self,
        modifiers: &Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(existing_cursor) = cx.active_drag_cursor_style() {
            let new_cursor = if Self::is_copy_modifier_set(modifiers) {
                CursorStyle::DragCopy
            } else {
                CursorStyle::PointingHand
            };
            if existing_cursor != new_cursor {
                cx.set_active_drag_cursor_style(new_cursor, window);
            }
        }
    }

    pub(super) fn is_copy_modifier_set(modifiers: &Modifiers) -> bool {
        cfg!(target_os = "macos") && modifiers.alt
            || cfg!(not(target_os = "macos")) && modifiers.control
    }

    pub(super) fn drag_onto(
        &mut self,
        selections: &DraggedSelection,
        target_entry_id: ProjectEntryId,
        is_file: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let resolved_selections = selections
            .items()
            .map(|entry| SelectedEntry {
                entry_id: self.resolve_entry(entry.entry_id),
                worktree_id: entry.worktree_id,
            })
            .collect::<BTreeSet<SelectedEntry>>();
        let entries = self.disjoint_entries(resolved_selections, cx);

        if Self::is_copy_modifier_set(&window.modifiers()) {
            let _ = maybe!({
                let project = self.project.read(cx);
                let target_worktree = project.worktree_for_entry(target_entry_id, cx)?;
                let worktree_id = target_worktree.read(cx).id();
                let target_entry = target_worktree
                    .read(cx)
                    .entry_for_id(target_entry_id)?
                    .clone();

                let mut copy_tasks = Vec::new();
                let mut disambiguation_range = None;
                for selection in &entries {
                    let (new_path, new_disambiguation_range) = self.create_paste_path(
                        selection,
                        (target_worktree.clone(), &target_entry),
                        cx,
                    )?;

                    let task = self.project.update(cx, |project, cx| {
                        project.copy_entry(selection.entry_id, (worktree_id, new_path).into(), cx)
                    });
                    copy_tasks.push(task);
                    disambiguation_range = new_disambiguation_range.or(disambiguation_range);
                }

                let item_count = copy_tasks.len();

                cx.spawn_in(window, async move |project_panel, cx| {
                    let mut last_succeed = None;
                    let mut changes = Vec::new();
                    for task in copy_tasks.into_iter() {
                        if let Some(Some(entry)) = task.await.log_err() {
                            last_succeed = Some(entry.id);
                            changes.push(Change::Created((worktree_id, entry.path).into()));
                        }
                    }
                    // update selection
                    if let Some(entry_id) = last_succeed {
                        project_panel.update_in(cx, |project_panel, window, cx| {
                            project_panel.selection = Some(SelectedEntry {
                                worktree_id,
                                entry_id,
                            });
                            // if only one entry was dragged and it was disambiguated, open the rename editor
                            if item_count == 1 && disambiguation_range.is_some() {
                                project_panel.rename_impl(disambiguation_range, window, cx);
                            }

                            project_panel.undo_manager.record(changes)
                        })??;
                    }

                    std::result::Result::Ok::<(), anyhow::Error>(())
                })
                .detach();
                Some(())
            });
        } else {
            let update_marks = !self.marked_entries.is_empty();
            let active_selection = selections.active_selection;

            // For folded selections, track the leaf suffix relative to the resolved
            // entry so we can refresh it after the move completes.
            let (folded_selection_info, folded_selection_entries): (
                Vec<(ProjectEntryId, RelPathBuf)>,
                HashSet<SelectedEntry>,
            ) = {
                let project = self.project.read(cx);
                let mut info = Vec::new();
                let mut folded_entries = HashSet::default();

                for selection in selections.items() {
                    let resolved_id = self.resolve_entry(selection.entry_id);
                    if resolved_id == selection.entry_id {
                        continue;
                    }
                    folded_entries.insert(*selection);
                    let Some(source_path) = project.path_for_entry(resolved_id, cx) else {
                        continue;
                    };
                    let Some(leaf_path) = project.path_for_entry(selection.entry_id, cx) else {
                        continue;
                    };
                    let Ok(suffix) = leaf_path.path.strip_prefix(source_path.path.as_ref()) else {
                        continue;
                    };
                    if suffix.as_unix_str().is_empty() {
                        continue;
                    }

                    info.push((resolved_id, suffix.to_rel_path_buf()));
                }
                (info, folded_entries)
            };

            // Capture old paths before moving so we can record undo operations.
            let old_paths: HashMap<ProjectEntryId, ProjectPath> = {
                let project = self.project.read(cx);
                entries
                    .iter()
                    .filter_map(|entry| {
                        let path = project.path_for_entry(entry.entry_id, cx)?;
                        Some((entry.entry_id, path))
                    })
                    .collect()
            };
            let destination_worktree_id = self
                .project
                .read(cx)
                .worktree_for_entry(target_entry_id, cx)
                .map(|wt| wt.read(cx).id());

            // Collect move tasks paired with their source entry ID so we can correlate
            // results with folded selections that need refreshing.
            let mut move_tasks: Vec<(ProjectEntryId, Task<Result<CreatedEntry>>)> = Vec::new();
            for entry in entries {
                if let Some(task) = self.move_entry(entry.entry_id, target_entry_id, is_file, cx) {
                    move_tasks.push((entry.entry_id, task));
                }
            }

            if move_tasks.is_empty() {
                return;
            }

            let workspace = self.workspace.clone();
            if folded_selection_info.is_empty() {
                cx.spawn_in(window, async move |project_panel, mut cx| {
                    let mut changes = Vec::new();
                    for (entry_id, task) in move_tasks {
                        if let Some(CreatedEntry::Included(new_entry)) = task
                            .await
                            .notify_workspace_async_err(workspace.clone(), &mut cx)
                        {
                            if let (Some(old_path), Some(worktree_id)) =
                                (old_paths.get(&entry_id), destination_worktree_id)
                            {
                                changes.push(Change::Renamed(
                                    old_path.clone(),
                                    (worktree_id, new_entry.path).into(),
                                ));
                            }
                        }
                    }
                    project_panel
                        .update(cx, |this, _| {
                            this.undo_manager.record(changes).log_err();
                        })
                        .ok();
                })
                .detach();
            } else {
                cx.spawn_in(window, async move |project_panel, mut cx| {
                    // Await all move tasks and collect successful results
                    let mut move_results: Vec<(ProjectEntryId, Entry)> = Vec::new();
                    let mut operations = Vec::new();
                    for (entry_id, task) in move_tasks {
                        if let Some(CreatedEntry::Included(new_entry)) = task
                            .await
                            .notify_workspace_async_err(workspace.clone(), &mut cx)
                        {
                            if let (Some(old_path), Some(worktree_id)) =
                                (old_paths.get(&entry_id), destination_worktree_id)
                            {
                                operations.push(Change::Renamed(
                                    old_path.clone(),
                                    (worktree_id, new_entry.path.clone()).into(),
                                ));
                            }
                            move_results.push((entry_id, new_entry));
                        }
                    }

                    if move_results.is_empty() {
                        return;
                    }

                    project_panel
                        .update(cx, |this, _| {
                            this.undo_manager.record(operations).log_err();
                        })
                        .ok();

                    // For folded selections, we need to refresh the leaf paths (with suffixes)
                    // because they may not be indexed yet after the parent directory was moved.
                    // First collect the paths to refresh, then refresh them.
                    let paths_to_refresh: Vec<(Entity<Worktree>, Arc<RelPath>)> = project_panel
                        .update(cx, |project_panel, cx| {
                            let project = project_panel.project.read(cx);
                            folded_selection_info
                                .iter()
                                .filter_map(|(resolved_id, suffix)| {
                                    let (_, new_entry) =
                                        move_results.iter().find(|(id, _)| id == resolved_id)?;
                                    let worktree = project.worktree_for_entry(new_entry.id, cx)?;
                                    let leaf_path = new_entry.path.join(suffix);
                                    Some((worktree, leaf_path))
                                })
                                .collect()
                        })
                        .ok()
                        .unwrap_or_default();

                    let refresh_tasks: Vec<_> = paths_to_refresh
                        .into_iter()
                        .filter_map(|(worktree, leaf_path)| {
                            worktree.update(cx, |worktree, cx| {
                                worktree
                                    .as_local_mut()
                                    .map(|local| local.refresh_entry(leaf_path, None, cx))
                            })
                        })
                        .collect();

                    for task in refresh_tasks {
                        task.await.log_err();
                    }

                    if update_marks && !folded_selection_entries.is_empty() {
                        project_panel
                            .update(cx, |project_panel, cx| {
                                project_panel.marked_entries.retain(|entry| {
                                    !folded_selection_entries.contains(entry)
                                        || *entry == active_selection
                                });
                                cx.notify();
                            })
                            .ok();
                    }
                })
                .detach();
            }
        }
    }

    pub(super) fn highlight_entry_for_external_drag(
        &self,
        target_entry: &Entry,
        target_worktree: &Worktree,
    ) -> Option<ProjectEntryId> {
        // Always highlight directory or parent directory if it's file
        if target_entry.is_dir() {
            Some(target_entry.id)
        } else {
            target_entry
                .path
                .parent()
                .and_then(|parent_path| target_worktree.entry_for_path(parent_path))
                .map(|parent_entry| parent_entry.id)
        }
    }

    pub(super) fn highlight_entry_for_selection_drag(
        &self,
        target_entry: &Entry,
        target_worktree: &Worktree,
        drag_state: &DraggedSelection,
        cx: &Context<Self>,
    ) -> Option<ProjectEntryId> {
        let target_parent_path = target_entry.path.parent();

        // In case of single item drag, we do not highlight existing
        // directory which item belongs too
        if drag_state.items().count() == 1
            && drag_state.active_selection.worktree_id == target_worktree.id()
        {
            let active_entry_path = self
                .project
                .read(cx)
                .path_for_entry(drag_state.active_selection.entry_id, cx)?;

            if let Some(active_parent_path) = active_entry_path.path.parent() {
                // Do not highlight active entry parent
                if active_parent_path == target_entry.path.as_ref() {
                    return None;
                }

                // Do not highlight active entry sibling files
                if Some(active_parent_path) == target_parent_path && target_entry.is_file() {
                    return None;
                }
            }
        }

        // Always highlight directory or parent directory if it's file
        if target_entry.is_dir() {
            Some(target_entry.id)
        } else {
            target_parent_path
                .and_then(|parent_path| target_worktree.entry_for_path(parent_path))
                .map(|parent_entry| parent_entry.id)
        }
    }

    pub(super) fn should_highlight_background_for_selection_drag(
        &self,
        drag_state: &DraggedSelection,
        last_root_id: ProjectEntryId,
        cx: &App,
    ) -> bool {
        // Always highlight for multiple entries
        if drag_state.items().count() > 1 {
            return true;
        }

        // Since root will always have empty relative path
        if let Some(entry_path) = self
            .project
            .read(cx)
            .path_for_entry(drag_state.active_selection.entry_id, cx)
        {
            if let Some(parent_path) = entry_path.path.parent() {
                if !parent_path.is_empty() {
                    return true;
                }
            }
        }

        // If parent is empty, check if different worktree
        if let Some(last_root_worktree_id) = self
            .project
            .read(cx)
            .worktree_id_for_entry(last_root_id, cx)
        {
            if drag_state.active_selection.worktree_id != last_root_worktree_id {
                return true;
            }
        }

        false
    }
}
