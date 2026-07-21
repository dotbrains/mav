use super::*;

impl Sidebar {
    pub(super) fn open_workspace_and_archive_thread(
        &mut self,
        session_id: acp::SessionId,
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
                this.update_entries(cx);
                this.archive_thread(&session_id, window, cx);
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub(super) fn close_agent_thread_tabs(
        &self,
        thread_id: ThreadId,
        workspaces_to_remove: &[Entity<Workspace>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Task<anyhow::Result<()>>> {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return Vec::new();
        };

        let workspaces: Vec<_> = multi_workspace
            .read(cx)
            .workspaces()
            .filter(|workspace| !workspaces_to_remove.contains(workspace))
            .cloned()
            .collect();

        let mut close_item_tasks = Vec::new();
        for workspace in workspaces {
            let panes = workspace.read(cx).panes().to_vec();
            for pane in panes {
                let items_to_close: Vec<EntityId> = pane
                    .read(cx)
                    .items()
                    .filter_map(|item| {
                        let item = item.downcast::<AgentThreadItem>()?;
                        (item.read(cx).thread_id(cx) == thread_id).then_some(item.entity_id())
                    })
                    .collect();

                if !items_to_close.is_empty() {
                    let task = pane.update(cx, |pane, cx| {
                        pane.close_items(window, cx, SaveIntent::Skip, &|item_id| {
                            items_to_close.contains(&item_id)
                        })
                    });
                    close_item_tasks.push(task);
                }
            }
        }

        close_item_tasks
    }

    pub(super) fn archive_thread(
        &mut self,
        session_id: &acp::SessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let store = ThreadMetadataStore::global(cx);
        let metadata = store.read(cx).entry_by_session(session_id).cloned();
        let metadata_thread_id = metadata.as_ref().map(|metadata| metadata.thread_id);
        let thread_entry = self.contents.entries.iter().find_map(|entry| match entry {
            ListEntry::Thread(thread) => metadata_thread_id
                .map_or_else(
                    || thread.metadata.session_id.as_ref() == Some(session_id),
                    |thread_id| thread.metadata.thread_id == thread_id,
                )
                .then(|| thread.clone()),
            _ => None,
        });
        let thread_id = metadata_thread_id.or_else(|| {
            thread_entry
                .as_ref()
                .map(|thread| thread.metadata.thread_id)
        });
        let active_workspace = thread_id.and_then(|thread_id| {
            self.active_entry.as_ref().and_then(|entry| {
                if entry.is_active_thread(&thread_id) {
                    Some(entry.workspace().clone())
                } else {
                    None
                }
            })
        });
        let thread_folder_paths = metadata
            .as_ref()
            .map(|metadata| metadata.folder_paths().clone())
            .or_else(|| {
                thread_entry
                    .as_ref()
                    .map(|thread| thread.metadata.folder_paths().clone())
            })
            .or_else(|| {
                active_workspace
                    .as_ref()
                    .map(|workspace| PathList::new(&workspace.read(cx).root_paths(cx)))
            });
        let thread_entry_workspace = thread_entry.map(|thread| thread.workspace.clone());

        if let (
            Some(metadata),
            Some(ThreadEntryWorkspace::Closed {
                folder_paths,
                project_group_key,
            }),
        ) = (metadata.as_ref(), thread_entry_workspace)
            && self.should_load_closed_workspace_for_archive(
                &folder_paths,
                &project_group_key,
                metadata.remote_connection.as_ref(),
                Some(metadata.thread_id),
                None,
                cx,
            )
        {
            self.open_workspace_and_archive_thread(
                session_id.clone(),
                folder_paths,
                project_group_key,
                window,
                cx,
            );
            return;
        }

        // Compute which linked worktree roots should be archived from disk if
        // this thread is archived. This must happen before we remove any
        // workspace from the MultiWorkspace, because `build_root_plan` needs
        // the currently open workspaces in order to find the affected projects
        // and repository handles for each linked worktree.
        let roots_to_archive = metadata
            .as_ref()
            .map(|metadata| {
                self.roots_to_archive_for_paths(
                    metadata.folder_paths(),
                    metadata.remote_connection.as_ref(),
                    thread_id,
                    None,
                    cx,
                )
            })
            .unwrap_or_default();

        let current_pos = self.contents.entries.iter().position(|entry| match entry {
            ListEntry::Thread(thread) => thread_id.map_or_else(
                || thread.metadata.session_id.as_ref() == Some(session_id),
                |tid| thread.metadata.thread_id == tid,
            ),
            _ => false,
        });
        let neighbor =
            current_pos.and_then(|position| self.neighboring_activatable_entry(position));

        // Check if archiving this thread would leave its worktree workspace
        // with no threads, requiring workspace removal.
        let workspace_to_remove = thread_folder_paths.as_ref().and_then(|folder_paths| {
            let thread_remote_connection =
                metadata.as_ref().and_then(|m| m.remote_connection.as_ref());
            self.linked_worktree_workspace_to_remove(
                folder_paths,
                thread_remote_connection,
                thread_id,
                None,
                &roots_to_archive,
                cx,
            )
        });

        // Also find workspaces for root plans that aren't covered by
        // workspace_to_remove. For workspaces that exclusively contain
        // worktrees being archived, remove the whole workspace. For
        // "mixed" workspaces (containing both archived and non-archived
        // worktrees), close only the editor items referencing the
        // archived worktrees so their Entity<Worktree> handles are
        // dropped without destroying the user's workspace layout.
        let mut workspaces_to_remove: Vec<Entity<Workspace>> =
            workspace_to_remove.into_iter().collect();
        let mut close_item_tasks = self.close_items_for_archived_worktrees(
            &roots_to_archive,
            &mut workspaces_to_remove,
            window,
            cx,
        );
        if let Some(thread_id) = thread_id {
            close_item_tasks.extend(self.close_agent_thread_tabs(
                thread_id,
                &workspaces_to_remove,
                window,
                cx,
            ));
        }

        if !workspaces_to_remove.is_empty() {
            let multi_workspace = self.multi_workspace.upgrade().unwrap();
            let session_id = session_id.clone();

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
            let remove_task = multi_workspace.update(cx, |mw, cx| {
                mw.remove(
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

            let thread_folder_paths = thread_folder_paths.clone();
            let thread_remote_connection = metadata
                .as_ref()
                .and_then(|metadata| metadata.remote_connection.clone());
            cx.spawn_in(window, async move |this, cx| {
                if !remove_task.await? {
                    return anyhow::Ok(());
                }

                for task in close_item_tasks {
                    let result: anyhow::Result<()> = task.await;
                    result.log_err();
                }

                this.update_in(cx, |this, window, cx| {
                    if let Some(thread_folder_paths) = thread_folder_paths.as_ref() {
                        this.delete_empty_drafts_for_archive_paths(
                            thread_folder_paths,
                            thread_remote_connection.as_ref(),
                            cx,
                        );
                    }
                    let in_flight = thread_id.and_then(|tid| {
                        this.start_archive_worktree_task(tid, roots_to_archive, cx)
                    });
                    this.archive_and_activate(
                        &session_id,
                        thread_id,
                        neighbor.as_ref(),
                        thread_folder_paths.as_ref(),
                        thread_remote_connection.as_ref(),
                        in_flight,
                        window,
                        cx,
                    );
                })?;
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        } else if !close_item_tasks.is_empty() {
            let session_id = session_id.clone();
            let thread_folder_paths = thread_folder_paths.clone();
            let thread_remote_connection = metadata
                .as_ref()
                .and_then(|metadata| metadata.remote_connection.clone());
            cx.spawn_in(window, async move |this, cx| {
                for task in close_item_tasks {
                    let result: anyhow::Result<()> = task.await;
                    result.log_err();
                }

                this.update_in(cx, |this, window, cx| {
                    let in_flight = thread_id.and_then(|tid| {
                        this.start_archive_worktree_task(tid, roots_to_archive, cx)
                    });
                    this.archive_and_activate(
                        &session_id,
                        thread_id,
                        neighbor.as_ref(),
                        thread_folder_paths.as_ref(),
                        thread_remote_connection.as_ref(),
                        in_flight,
                        window,
                        cx,
                    );
                })?;
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        } else {
            let in_flight = thread_id
                .and_then(|tid| self.start_archive_worktree_task(tid, roots_to_archive, cx));
            self.archive_and_activate(
                session_id,
                thread_id,
                neighbor.as_ref(),
                thread_folder_paths.as_ref(),
                metadata
                    .as_ref()
                    .and_then(|metadata| metadata.remote_connection.as_ref()),
                in_flight,
                window,
                cx,
            );
        }
    }

    /// Archive a thread and activate the nearest neighbor or a draft.
    ///
    /// IMPORTANT: when activating a neighbor or creating a fallback draft,
    /// this method also activates the target workspace in the MultiWorkspace.
    /// This is critical because `rebuild_contents` derives the active
    /// workspace from `mw.workspace()`. If the linked worktree workspace is
    /// still active after archiving its last thread, `rebuild_contents` sees
    /// the threadless linked worktree as active and emits a spurious
    /// "+ New Thread" entry with the worktree chip — keeping the worktree
    /// alive and preventing disk cleanup.
    ///
    /// When `in_flight_archive` is present, it is the background task that
    /// persists the linked worktree's git state and deletes it from disk.
    /// We attach it to the metadata store at the same time we mark the thread
    /// archived so failures can automatically unarchive the thread and user-
    /// initiated unarchive can cancel the task.
    pub(super) fn archive_and_activate(
        &mut self,
        _session_id: &acp::SessionId,
        thread_id: Option<agent_ui::ThreadId>,
        neighbor: Option<&ActivatableEntry>,
        thread_folder_paths: Option<&PathList>,
        thread_remote_connection: Option<&RemoteConnectionOptions>,
        in_flight_archive: Option<(Task<()>, async_channel::Sender<()>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(thread_id) = thread_id {
            ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                store.archive(thread_id, in_flight_archive, cx);
            });
        }

        let is_active = self
            .active_entry
            .as_ref()
            .is_some_and(|entry| thread_id.is_some_and(|tid| entry.is_active_thread(&tid)));

        if is_active {
            self.active_entry = None;
        }

        if !is_active {
            // The user is looking at a different thread/draft. Clear the
            // archived thread from its workspace's panel so that switching
            // to that workspace later doesn't show a stale thread.
            if let Some(folder_paths) = thread_folder_paths {
                if let Some(workspace) = self.multi_workspace.upgrade().and_then(|mw| {
                    mw.read(cx)
                        .workspace_for_paths(folder_paths, thread_remote_connection, cx)
                }) {
                    if let Some(panel) = workspace.read(cx).panel::<AgentPanel>(cx) {
                        let panel_shows_archived = panel
                            .read(cx)
                            .active_conversation_view()
                            .map(|cv| cv.read(cx).parent_id())
                            .is_some_and(|live_thread_id| {
                                thread_id.is_some_and(|id| id == live_thread_id)
                            });
                        if panel_shows_archived {
                            panel.update(cx, |panel, cx| {
                                panel.clear_base_view(window, cx);
                            });
                        }
                    }
                }
            }
            return;
        }

        if neighbor.is_some_and(|neighbor| self.activate_entry(neighbor, window, cx)) {
            return;
        }

        // No neighbor or its workspace isn't open — just clear the
        // panel so the group is left empty.
        if let Some(folder_paths) = thread_folder_paths {
            let workspace = self.multi_workspace.upgrade().and_then(|mw| {
                mw.read(cx)
                    .workspace_for_paths(folder_paths, thread_remote_connection, cx)
            });
            if let Some(workspace) = workspace {
                if let Some(panel) = workspace.read(cx).panel::<AgentPanel>(cx) {
                    panel.update(cx, |panel, cx| {
                        panel.clear_base_view(window, cx);
                    });
                }
            }
        }
    }

    pub(super) fn archive_selected_thread(
        &mut self,
        _: &ArchiveSelectedThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else {
            return;
        };
        match self.contents.entries.get(ix) {
            Some(ListEntry::Thread(thread)) => {
                match thread.status {
                    AgentThreadStatus::Running | AgentThreadStatus::WaitingForConfirmation => {
                        return;
                    }
                    AgentThreadStatus::Completed | AgentThreadStatus::Error => {}
                }
                if thread.draft.is_some() {
                    let workspace = thread.workspace.clone();
                    let draft_id = thread.metadata.thread_id;
                    self.remove_draft(draft_id, &workspace, window, cx);
                } else if let Some(session_id) = thread.metadata.session_id.clone() {
                    self.archive_thread(&session_id, window, cx);
                }
            }
            Some(ListEntry::Terminal(terminal)) => {
                let metadata = terminal.metadata.clone();
                let workspace = terminal.workspace.clone();
                self.close_terminal(&metadata, &workspace, window, cx);
            }
            _ => {}
        }
    }
}
