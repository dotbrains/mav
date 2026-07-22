use super::*;

impl RecentProjectsDelegate {
    fn open_recent_projects(
        &mut self,
        candidate_id: usize,
        secondary: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let Some(candidate_workspace) = self.workspaces.get(candidate_id) else {
            return;
        };

        let replace_current_window = self.create_new_window == secondary;
        let candidate_workspace_id = candidate_workspace.workspace_id;
        let candidate_workspace_location = candidate_workspace.location.clone();
        let candidate_workspace_paths = candidate_workspace.paths.clone();

        workspace.update(cx, |workspace, cx| {
            if workspace.database_id() == Some(candidate_workspace_id) {
                return;
            }
            match candidate_workspace_location {
                SerializedWorkspaceLocation::Local => {
                    let paths = candidate_workspace_paths.paths().to_vec();
                    if replace_current_window {
                        if let Some(handle) = window.window_handle().downcast::<MultiWorkspace>() {
                            cx.defer(move |cx| {
                                if let Some(task) = handle
                                    .update(cx, |multi_workspace, window, cx| {
                                        multi_workspace.open_project(
                                            paths,
                                            OpenMode::Activate,
                                            window,
                                            cx,
                                        )
                                    })
                                    .log_err()
                                {
                                    task.detach_and_log_err(cx);
                                }
                            });
                        }
                        return;
                    } else {
                        workspace
                            .open_workspace_for_paths(OpenMode::NewWindow, paths, window, cx)
                            .detach_and_prompt_err(
                                "Failed to open project",
                                window,
                                cx,
                                |_, _, _| None,
                            );
                    }
                }
                SerializedWorkspaceLocation::Remote(mut connection) => {
                    let app_state = workspace.app_state().clone();
                    let replace_window = if replace_current_window {
                        window.window_handle().downcast::<MultiWorkspace>()
                    } else {
                        None
                    };
                    let open_options = OpenOptions {
                        requesting_window: replace_window,
                        ..Default::default()
                    };
                    if let RemoteConnectionOptions::Ssh(connection) = &mut connection {
                        RemoteSettings::get_global(cx)
                            .fill_connection_options_from_settings(connection);
                    };
                    let paths = candidate_workspace_paths.paths().to_vec();
                    cx.spawn_in(window, async move |_, cx| {
                        open_remote_project(connection.clone(), paths, app_state, open_options, cx)
                            .await
                    })
                    .detach_and_prompt_err(
                        "Failed to open project",
                        window,
                        cx,
                        |_, _, _| None,
                    );
                }
            }
        });
        cx.emit(DismissEvent);
    }

    fn add_paths_to_project(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let open_paths_task = workspace.update(cx, |workspace, cx| {
            workspace.open_paths(
                paths,
                OpenOptions {
                    visible: Some(OpenVisible::All),
                    ..Default::default()
                },
                None,
                window,
                cx,
            )
        });
        cx.spawn_in(window, async move |picker, cx| {
            let _result = open_paths_task.await;
            picker
                .update_in(cx, |picker, window, cx| {
                    let Some(workspace) = picker.delegate.workspace.upgrade() else {
                        return;
                    };
                    picker.delegate.open_folders = get_open_folders(workspace.read(cx), cx);
                    let query = picker.query(cx);
                    picker.update_matches(query, window, cx);
                })
                .ok();
        })
        .detach();
    }

    /// Returns the new selection index after the entry at `deleted_index`
    /// is removed.
    ///
    /// - Prefers the nearest entry matching `prefer_section` so the user
    ///   stays in the same section they were navigating.
    /// - Falls back to any other selectable entry so the picker doesn't
    ///   land on a header.
    fn replacement_index_after_deletion(
        &self,
        deleted_index: usize,
        prefer_previous: bool,
        prefer_section: fn(&ProjectPickerEntry) -> bool,
    ) -> Option<usize> {
        let replacement_index = |matches_entry: fn(&ProjectPickerEntry) -> bool| {
            let next_index = self
                .filtered_entries
                .iter()
                .enumerate()
                .skip(deleted_index)
                .find_map(|(index, entry)| matches_entry(entry).then_some(index));
            let previous_index = self
                .filtered_entries
                .iter()
                .enumerate()
                .take(deleted_index.min(self.filtered_entries.len()))
                .rev()
                .find_map(|(index, entry)| matches_entry(entry).then_some(index));

            if prefer_previous {
                previous_index.or(next_index)
            } else {
                next_index.or(previous_index)
            }
        };

        replacement_index(prefer_section).or_else(|| replacement_index(is_selectable_entry))
    }

    fn update_picker_after_recent_project_deletion(
        picker: &mut Picker<Self>,
        deleted_index: usize,
        workspaces: Vec<RecentWorkspace>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        let prefer_previous = picker.is_scrolled_to_end() == Some(true);
        picker.delegate.set_workspaces(workspaces);
        picker.delegate.snap_selection_to_first_non_header_match = false;
        picker.update_matches_with_options(
            picker.query(cx),
            ScrollBehavior::PreserveOffset,
            window,
            cx,
        );
        if let Some(replacement_index) = picker.delegate.replacement_index_after_deletion(
            deleted_index,
            prefer_previous,
            |entry| matches!(entry, ProjectPickerEntry::RecentProject(_)),
        ) {
            picker.set_selected_index(replacement_index, None, false, window, cx);
        }
    }

    fn delete_recent_project(
        &self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        if let Some(ProjectPickerEntry::RecentProject(selected_match)) =
            self.filtered_entries.get(ix)
        {
            let Some(recent_workspace) = self.workspaces.get(selected_match.candidate_id).cloned()
            else {
                return;
            };
            let fs = self
                .workspace
                .upgrade()
                .map(|ws| ws.read(cx).app_state().fs.clone());
            let db = WorkspaceDb::global(cx);
            cx.spawn_in(window, async move |this, cx| {
                let Some(fs) = fs else { return };
                let deleted_workspace_ids = db
                    .delete_recent_workspace_group(&recent_workspace)
                    .await
                    .log_err()
                    .unwrap_or_default();
                let workspaces = db
                    .recent_project_workspaces(fs.as_ref())
                    .await
                    .unwrap_or_default();
                this.update_in(cx, move |picker, window, cx| {
                    Self::update_picker_after_recent_project_deletion(
                        picker, ix, workspaces, window, cx,
                    );
                    // After deleting a project, we want to update the history manager to reflect the change.
                    // But we do not emit a update event when user opens a project, because it's handled in `workspace::load_workspace`.
                    if let Some(history_manager) = HistoryManager::global(cx) {
                        history_manager.update(cx, |this, cx| {
                            for workspace_id in &deleted_workspace_ids {
                                this.delete_history(*workspace_id, cx);
                            }
                        });
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    fn remove_project_group(
        &mut self,
        key: ProjectGroupKey,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        if let Some(handle) = window.window_handle().downcast::<MultiWorkspace>() {
            let key_for_remove = key.clone();
            cx.defer(move |cx| {
                handle
                    .update(cx, |multi_workspace, window, cx| {
                        multi_workspace
                            .remove_project_group(&key_for_remove, window, cx)
                            .detach_and_log_err(cx);
                    })
                    .log_err();
            });
        }

        self.window_project_groups.retain(|k| k != &key);
    }

    fn is_current_workspace(
        &self,
        workspace_id: WorkspaceId,
        cx: &mut Context<Picker<Self>>,
    ) -> bool {
        if let Some(workspace) = self.workspace.upgrade() {
            let workspace = workspace.read(cx);
            if Some(workspace_id) == workspace.database_id() {
                return true;
            }
        }

        false
    }

    fn is_active_project_group(&self, key: &ProjectGroupKey, cx: &App) -> bool {
        if let Some(workspace) = self.workspace.upgrade() {
            return workspace.read(cx).project_group_key(cx) == *key;
        }
        false
    }

    fn is_in_current_window_groups(&self, workspace: &RecentWorkspace) -> bool {
        self.window_project_groups
            .iter()
            .any(|key| key.matches(&workspace.project_group_key()))
    }

    fn is_open_folder(&self, paths: &PathList) -> bool {
        if self.open_folders.is_empty() {
            return false;
        }

        for workspace_path in paths.paths() {
            for open_folder in &self.open_folders {
                if workspace_path == &open_folder.path {
                    return true;
                }
            }
        }

        false
    }

    fn is_valid_recent_candidate(
        &self,
        workspace: &RecentWorkspace,
        cx: &mut Context<Picker<Self>>,
    ) -> bool {
        !self.is_current_workspace(workspace.workspace_id, cx)
            && !self.is_in_current_window_groups(workspace)
            && !self.is_open_folder(&workspace.paths)
    }
}
