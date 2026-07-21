use super::*;

impl Sidebar {
    pub(super) fn open_workspace_and_remove_draft(
        &mut self,
        draft_id: ThreadId,
        folder_paths: PathList,
        project_group_key: ProjectGroupKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((open_task, modal_workspace)) =
            self.open_workspace_for_archive(folder_paths, project_group_key, window, cx)
        else {
            return;
        };

        cx.spawn_in(window, async move |this, cx| {
            let result = open_task.await;
            remote_connection::dismiss_connection_modal(&modal_workspace, cx);
            let workspace = result?;
            Self::wait_for_archive_workspace_metadata(&workspace, cx).await;

            this.update_in(cx, |this, window, cx| {
                let workspace = ThreadEntryWorkspace::Open(workspace);
                this.remove_draft(draft_id, &workspace, window, cx);
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub(super) fn remove_draft(
        &mut self,
        draft_id: ThreadId,
        workspace: &ThreadEntryWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let metadata = ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(draft_id)
            .cloned();

        if let ThreadEntryWorkspace::Closed {
            folder_paths,
            project_group_key,
        } = workspace
            && self.should_load_closed_workspace_for_archive(
                folder_paths,
                project_group_key,
                metadata
                    .as_ref()
                    .and_then(|metadata| metadata.remote_connection.as_ref()),
                Some(draft_id),
                None,
                cx,
            )
        {
            self.open_workspace_and_remove_draft(
                draft_id,
                folder_paths.clone(),
                project_group_key.clone(),
                window,
                cx,
            );
            return;
        }

        let draft_folder_paths = metadata
            .as_ref()
            .map(|metadata| metadata.folder_paths().clone())
            .or_else(|| match workspace {
                ThreadEntryWorkspace::Open(workspace) => {
                    Some(PathList::new(&workspace.read(cx).root_paths(cx)))
                }
                ThreadEntryWorkspace::Closed { folder_paths, .. } => Some(folder_paths.clone()),
            });
        let draft_remote_connection = metadata
            .as_ref()
            .and_then(|metadata| metadata.remote_connection.clone());
        let roots_to_archive = metadata
            .as_ref()
            .map(|metadata| {
                self.roots_to_archive_for_paths(
                    metadata.folder_paths(),
                    metadata.remote_connection.as_ref(),
                    Some(draft_id),
                    None,
                    cx,
                )
            })
            .unwrap_or_default();

        let was_active = self
            .active_entry
            .as_ref()
            .is_some_and(|entry| entry.is_active_thread(&draft_id));
        let neighbor = self
            .contents
            .entries
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    ListEntry::Thread(thread) if thread.metadata.thread_id == draft_id
                )
            })
            .and_then(|position| self.neighboring_activatable_entry(position));

        let workspace_to_remove = draft_folder_paths.as_ref().and_then(|folder_paths| {
            self.linked_worktree_workspace_to_remove(
                folder_paths,
                draft_remote_connection.as_ref(),
                Some(draft_id),
                None,
                &roots_to_archive,
                cx,
            )
        });
        let mut workspaces_to_remove: Vec<Entity<Workspace>> =
            workspace_to_remove.into_iter().collect();
        let close_item_tasks = self.close_items_for_archived_worktrees(
            &roots_to_archive,
            &mut workspaces_to_remove,
            window,
            cx,
        );

        if !workspaces_to_remove.is_empty() {
            let Some(multi_workspace) = self.multi_workspace.upgrade() else {
                return;
            };
            let draft_workspace_removed = matches!(
                workspace,
                ThreadEntryWorkspace::Open(workspace) if workspaces_to_remove.contains(workspace)
            );
            let (fallback_paths, project_group_key) = neighbor
                .as_ref()
                .map(|neighbor| neighbor.project_location(cx))
                .unwrap_or_else(|| {
                    workspaces_to_remove
                        .first()
                        .map(|workspace| {
                            let key = workspace.read(cx).project_group_key(cx);
                            (key.path_list().clone(), key)
                        })
                        .unwrap_or_default()
                });

            let excluded = workspaces_to_remove.clone();
            let remove_task = multi_workspace.update(cx, |multi_workspace, cx| {
                multi_workspace.remove(
                    workspaces_to_remove,
                    move |this, window, cx| {
                        let active_workspace = this.workspace().clone();
                        this.find_or_create_workspace(
                            fallback_paths,
                            project_group_key.host(),
                            Some(project_group_key),
                            |options, window, cx| {
                                connect_remote(active_workspace, options, window, cx)
                            },
                            &excluded,
                            None,
                            OpenMode::Activate,
                            window,
                            cx,
                        )
                    },
                    window,
                    cx,
                )
            });

            let workspace = workspace.clone();
            cx.spawn_in(window, async move |this, cx| {
                if !remove_task.await? {
                    return anyhow::Ok(());
                }

                for task in close_item_tasks {
                    let result: anyhow::Result<()> = task.await;
                    result.log_err();
                }

                this.update_in(cx, |this, window, cx| {
                    if draft_workspace_removed {
                        if let Some(draft_folder_paths) = draft_folder_paths.as_ref() {
                            this.delete_empty_drafts_for_archive_paths(
                                draft_folder_paths,
                                draft_remote_connection.as_ref(),
                                cx,
                            );
                        }
                    }
                    this.remove_draft_entry(
                        draft_id,
                        &workspace,
                        was_active,
                        neighbor.as_ref(),
                        !draft_workspace_removed,
                        roots_to_archive,
                        window,
                        cx,
                    );
                })?;
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        } else if !close_item_tasks.is_empty() {
            let workspace = workspace.clone();
            cx.spawn_in(window, async move |this, cx| {
                for task in close_item_tasks {
                    let result: anyhow::Result<()> = task.await;
                    result.log_err();
                }

                this.update_in(cx, |this, window, cx| {
                    this.remove_draft_entry(
                        draft_id,
                        &workspace,
                        was_active,
                        neighbor.as_ref(),
                        true,
                        roots_to_archive,
                        window,
                        cx,
                    );
                })?;
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        } else {
            self.remove_draft_entry(
                draft_id,
                workspace,
                was_active,
                neighbor.as_ref(),
                true,
                roots_to_archive,
                window,
                cx,
            );
        }
    }

    pub(super) fn remove_draft_entry(
        &mut self,
        draft_id: ThreadId,
        workspace: &ThreadEntryWorkspace,
        was_active: bool,
        neighbor: Option<&ActivatableEntry>,
        activate_panel_draft: bool,
        roots_to_archive: Vec<thread_worktree_archive::RootPlan>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Fallback to a neighbor thread when the discarded
        // draft was the active entry.
        let activate_panel_draft = activate_panel_draft && !(was_active && neighbor.is_some());

        let removed_from_panel = if let ThreadEntryWorkspace::Open(workspace) = workspace {
            workspace.update(cx, |workspace, cx| {
                if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                    panel.update(cx, |panel, cx| {
                        if activate_panel_draft {
                            panel.remove_thread(draft_id, window, cx);
                        } else {
                            panel.remove_thread_without_activating_draft(draft_id, window, cx);
                        }
                    });
                    true
                } else {
                    false
                }
            })
        } else {
            false
        };

        if !removed_from_panel {
            ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                store.delete(draft_id, cx);
            });
        }

        self.start_detached_archive_worktree_task(roots_to_archive, cx);

        if was_active {
            self.active_entry = None;
            if !activate_panel_draft {
                if neighbor
                    .as_ref()
                    .is_some_and(|neighbor| self.activate_entry(neighbor, window, cx))
                {
                    return;
                }
                self.sync_active_entry_from_active_workspace(cx);
            }
        }

        self.update_entries(cx);
    }
}
