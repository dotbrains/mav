use super::*;

impl RecentProjectsDelegate {
    pub(super) fn confirm_delegate(
        &mut self,
        secondary: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        match self.filtered_entries.get(self.selected_index) {
            Some(ProjectPickerEntry::OpenFolder { index, .. }) => {
                let Some(folder) = self.open_folders.get(*index) else {
                    return;
                };
                let worktree_id = folder.worktree_id;
                if let Some(workspace) = self.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        let git_store = workspace.project().read(cx).git_store().clone();
                        git_store.update(cx, |git_store, cx| {
                            git_store.set_active_repo_for_worktree(worktree_id, cx);
                        });
                    });
                }
                cx.emit(DismissEvent);
            }
            Some(ProjectPickerEntry::ProjectGroup(selected_match)) => {
                let Some(key) = self.window_project_groups.get(selected_match.candidate_id) else {
                    return;
                };

                if secondary && key.host().is_none() && self.window_project_groups.len() >= 2 {
                    move_project_group_to_new_window(key, window, cx);
                    cx.emit(DismissEvent);
                    return;
                }

                let key = key.clone();
                if let Some(handle) = window.window_handle().downcast::<MultiWorkspace>() {
                    cx.defer(move |cx| {
                        // Try to activate an existing workspace for this project group
                        // first, so we preserve the actual worktree paths (which may
                        // differ from the main git worktree paths stored in the key).
                        if let Some(workspace) = handle
                            .update(cx, |multi_workspace, _window, cx| {
                                multi_workspace.last_active_workspace_for_group(&key, cx)
                            })
                            .log_err()
                            .flatten()
                        {
                            handle
                                .update(cx, |multi_workspace, window, cx| {
                                    multi_workspace.activate(workspace, None, window, cx);
                                })
                                .log_err();
                        } else {
                            let path_list = key.path_list().clone();
                            let host = key.host();
                            if let Some(task) = handle
                                .update(cx, |multi_workspace, window, cx| {
                                    let modal_workspace = multi_workspace.workspace().clone();
                                    multi_workspace.find_or_create_workspace(
                                        path_list,
                                        host,
                                        Some(key.clone()),
                                        move |options, window, cx| {
                                            connect_with_modal(
                                                &modal_workspace,
                                                options,
                                                window,
                                                cx,
                                            )
                                        },
                                        &[],
                                        None,
                                        OpenMode::Activate,
                                        window,
                                        cx,
                                    )
                                })
                                .log_err()
                            {
                                task.detach_and_log_err(cx);
                            }
                        }
                    });
                }
                cx.emit(DismissEvent);
            }
            Some(ProjectPickerEntry::RecentProject(selected_match)) => {
                let candidate_id = selected_match.candidate_id;
                self.open_recent_projects(candidate_id, secondary, window, cx);
            }
            _ => {}
        }
    }
}
