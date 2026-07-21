use super::*;

impl Sidebar {
    pub(super) fn should_load_closed_workspace_for_archive(
        &self,
        folder_paths: &PathList,
        project_group_key: &ProjectGroupKey,
        remote_connection: Option<&RemoteConnectionOptions>,
        except_thread_id: Option<ThreadId>,
        except_terminal_id: Option<TerminalId>,
        cx: &App,
    ) -> bool {
        if folder_paths.is_empty() || folder_paths == project_group_key.path_list() {
            return false;
        }

        let archive_workspaces = self.archive_workspaces(cx);
        let thread_store = ThreadMetadataStore::global(cx);
        let thread_store = thread_store.read(cx);
        if folder_paths.ordered_paths().any(|path| {
            Self::path_is_referenced_by_unarchived_threads_for_archive(
                &thread_store,
                except_thread_id,
                path,
                remote_connection,
                &archive_workspaces,
                cx,
            )
        }) {
            return false;
        }

        TerminalThreadMetadataStore::try_global(cx).is_none_or(|terminal_store| {
            let terminal_store = terminal_store.read(cx);
            !folder_paths.ordered_paths().any(|path| {
                terminal_store.path_is_referenced_by_terminal(
                    except_terminal_id,
                    path,
                    remote_connection,
                )
            })
        })
    }

    pub(super) fn path_is_referenced_by_unarchived_threads_for_archive(
        thread_store: &ThreadMetadataStore,
        except_thread_id: Option<ThreadId>,
        path: &Path,
        remote_connection: Option<&RemoteConnectionOptions>,
        archive_workspaces: &[Entity<Workspace>],
        cx: &App,
    ) -> bool {
        thread_store.path_is_referenced_by_unarchived_threads_matching(
            except_thread_id,
            path,
            remote_connection,
            |thread| Self::thread_blocks_worktree_archive(thread, archive_workspaces, cx),
        )
    }

    pub(super) fn archive_workspaces(&self, cx: &App) -> Vec<Entity<Workspace>> {
        let multi_workspace = self.multi_workspace.upgrade();
        thread_worktree_archive::workspaces_for_archive(multi_workspace.as_ref(), cx)
    }

    pub(super) fn count_threads_blocking_worktree_archive(
        &self,
        path_list: &PathList,
        remote_connection: Option<&RemoteConnectionOptions>,
        except_thread_id: Option<ThreadId>,
        cx: &App,
    ) -> usize {
        let archive_workspaces = self.archive_workspaces(cx);
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries_for_path(path_list, remote_connection)
            .filter(|thread| Some(thread.thread_id) != except_thread_id)
            .filter(|thread| Self::thread_blocks_worktree_archive(thread, &archive_workspaces, cx))
            .count()
    }

    pub(super) fn roots_to_archive_for_paths(
        &self,
        folder_paths: &PathList,
        remote_connection: Option<&RemoteConnectionOptions>,
        except_thread_id: Option<ThreadId>,
        except_terminal_id: Option<TerminalId>,
        cx: &App,
    ) -> Vec<thread_worktree_archive::RootPlan> {
        let workspaces = self.archive_workspaces(cx);
        folder_paths
            .ordered_paths()
            .filter_map(|path| {
                thread_worktree_archive::build_root_plan(path, remote_connection, &workspaces, cx)
            })
            .filter(|plan| {
                let store = ThreadMetadataStore::global(cx);
                let store = store.read(cx);
                !Self::path_is_referenced_by_unarchived_threads_for_archive(
                    &store,
                    except_thread_id,
                    plan.root_path.as_path(),
                    remote_connection,
                    &workspaces,
                    cx,
                )
            })
            .filter(|root| {
                TerminalThreadMetadataStore::try_global(cx).is_none_or(|terminal_store| {
                    !terminal_store.read(cx).path_is_referenced_by_terminal(
                        except_terminal_id,
                        root.root_path.as_path(),
                        remote_connection,
                    )
                })
            })
            .collect()
    }

    pub(super) fn linked_worktree_workspace_to_remove(
        &self,
        folder_paths: &PathList,
        remote_connection: Option<&RemoteConnectionOptions>,
        except_thread_id: Option<ThreadId>,
        except_terminal_id: Option<TerminalId>,
        roots_to_archive: &[thread_worktree_archive::RootPlan],
        cx: &App,
    ) -> Option<Entity<Workspace>> {
        if folder_paths.is_empty() {
            return None;
        }

        let remaining = self.count_threads_blocking_worktree_archive(
            folder_paths,
            remote_connection,
            except_thread_id,
            cx,
        );

        if remaining > 0 {
            return None;
        }

        let multi_workspace = self.multi_workspace.upgrade()?;
        let workspace =
            multi_workspace
                .read(cx)
                .workspace_for_paths(folder_paths, remote_connection, cx)?;

        if workspace_has_terminal_metadata_except(&workspace, except_terminal_id, cx) {
            return None;
        }

        if !roots_to_archive.is_empty() {
            let archive_paths: HashSet<&Path> = roots_to_archive
                .iter()
                .map(|root| root.root_path.as_path())
                .collect();
            let project = workspace.read(cx).project().clone();
            let visible_worktree_paths = project
                .read(cx)
                .visible_worktrees(cx)
                .map(|worktree| worktree.read(cx).abs_path())
                .collect::<Vec<_>>();
            return (!visible_worktree_paths.is_empty()
                && visible_worktree_paths
                    .iter()
                    .all(|path| archive_paths.contains(path.as_ref())))
            .then_some(workspace);
        }

        let group_key = workspace.read(cx).project_group_key(cx);
        (group_key.path_list() != folder_paths).then_some(workspace)
    }

    pub(super) fn delete_empty_drafts_for_archive_roots(
        &self,
        roots: &[thread_worktree_archive::RootPlan],
        cx: &mut Context<Self>,
    ) {
        self.delete_empty_drafts_for_archive_targets(
            roots
                .iter()
                .map(|root| (root.root_path.as_path(), root.remote_connection.as_ref())),
            cx,
        );
    }

    pub(super) fn delete_empty_drafts_for_archive_paths(
        &self,
        paths: &PathList,
        remote_connection: Option<&RemoteConnectionOptions>,
        cx: &mut Context<Self>,
    ) {
        self.delete_empty_drafts_for_archive_targets(
            paths
                .ordered_paths()
                .map(|path| (path.as_path(), remote_connection)),
            cx,
        );
    }

    pub(super) fn delete_empty_drafts_for_archive_targets<'a>(
        &self,
        targets: impl IntoIterator<Item = (&'a Path, Option<&'a RemoteConnectionOptions>)>,
        cx: &mut Context<Self>,
    ) {
        let targets = targets.into_iter().collect::<Vec<_>>();
        if targets.is_empty() {
            return;
        }

        let archive_workspaces = self.archive_workspaces(cx);
        let draft_thread_ids = ThreadMetadataStore::global(cx)
            .read(cx)
            .unarchived_draft_ids_matching(|thread| {
                targets.iter().any(|(path, remote_connection)| {
                    thread.matches_remote_connection(*remote_connection)
                        && thread.references_folder_path(path)
                }) && !Self::thread_blocks_worktree_archive(thread, &archive_workspaces, cx)
            });
        if draft_thread_ids.is_empty() {
            return;
        }

        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.delete_all(draft_thread_ids, cx);
        });
    }

    pub(super) fn thread_blocks_worktree_archive(
        thread: &ThreadMetadata,
        archive_workspaces: &[Entity<Workspace>],
        cx: &App,
    ) -> bool {
        if !thread.is_draft() {
            return true;
        }

        agent_ui::draft_prompt_store::draft_has_user_content(
            thread.thread_id,
            archive_workspaces,
            cx,
        )
    }

    pub(super) async fn wait_for_archive_workspace_metadata(
        workspace: &Entity<Workspace>,
        cx: &mut gpui::AsyncApp,
    ) {
        let scans_complete =
            workspace.read_with(cx, |workspace, cx| workspace.worktree_scans_complete(cx));
        scans_complete.await;

        let project = workspace.read_with(cx, |workspace, _| workspace.project().clone());
        let barriers = project.update(cx, |project, cx| {
            let repositories = project
                .repositories(cx)
                .values()
                .cloned()
                .collect::<Vec<_>>();
            repositories
                .into_iter()
                .map(|repository| repository.update(cx, |repository, _| repository.barrier()))
                .collect::<Vec<_>>()
        });
        for barrier in barriers {
            let result: anyhow::Result<()> = barrier.await.map_err(|_| {
                anyhow::anyhow!("git repository barrier canceled while archiving worktree")
            });
            result.log_err();
        }
    }

    pub(super) fn open_workspace_for_archive(
        &mut self,
        folder_paths: PathList,
        project_group_key: ProjectGroupKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Task<anyhow::Result<Entity<Workspace>>>, Entity<Workspace>)> {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return None;
        };

        let host = project_group_key.host();
        let active_workspace = multi_workspace.read(cx).workspace().clone();
        let modal_workspace = active_workspace.clone();

        let open_task = multi_workspace.update(cx, |this, cx| {
            this.find_or_create_workspace(
                folder_paths,
                host,
                Some(project_group_key),
                |options, window, cx| connect_remote(active_workspace, options, window, cx),
                &[],
                None,
                OpenMode::Add,
                window,
                cx,
            )
        });

        Some((open_task, modal_workspace))
    }
}
