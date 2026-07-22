use super::*;

impl WorkspaceDb {
    fn recent_workspaces(
        &self,
    ) -> Result<
        Vec<(
            WorkspaceId,
            PathList,
            Option<PathList>,
            Option<RemoteConnectionId>,
            Option<String>,
            DateTime<Utc>,
        )>,
    > {
        Ok(self
            .recent_workspaces_query()?
            .into_iter()
            .map(
                |(
                    id,
                    paths,
                    order,
                    identity_paths,
                    identity_paths_order,
                    remote_connection_id,
                    session_id,
                    timestamp,
                )| {
                    (
                        id,
                        PathList::deserialize(&SerializedPathList { paths, order }),
                        identity_paths.map(|paths| {
                            PathList::deserialize(&SerializedPathList {
                                paths,
                                order: identity_paths_order.unwrap_or_default(),
                            })
                        }),
                        remote_connection_id.map(RemoteConnectionId),
                        session_id,
                        parse_timestamp(&timestamp),
                    )
                },
            )
            .collect())
    }
    fn session_workspaces(
        &self,
        session_id: String,
    ) -> Result<
        Vec<(
            WorkspaceId,
            PathList,
            Option<u64>,
            Option<RemoteConnectionId>,
        )>,
    > {
        Ok(self
            .session_workspaces_query(session_id)?
            .into_iter()
            .map(
                |(workspace_id, paths, order, window_id, remote_connection_id)| {
                    (
                        WorkspaceId(workspace_id),
                        PathList::deserialize(&SerializedPathList { paths, order }),
                        window_id,
                        remote_connection_id.map(RemoteConnectionId),
                    )
                },
            )
            .collect())
    }
    async fn all_paths_exist_with_a_directory(paths: &[PathBuf], fs: &dyn Fs) -> bool {
        let mut any_dir = false;
        for path in paths {
            match fs.metadata(path).await.ok().flatten() {
                None => return false,
                Some(meta) => {
                    if meta.is_dir {
                        any_dir = true;
                    }
                }
            }
        }
        any_dir
    }

    // Returns the raw recent workspace history. Scratch workspaces (no paths) are filtered
    // out because they are restored separately by `last_session_workspace_locations`.
    pub async fn recent_project_workspaces_ungrouped(
        &self,
        fs: &dyn Fs,
    ) -> Result<Vec<RecentWorkspace>> {
        let remote_connections = self.remote_connections()?;
        let mut result = Vec::new();
        for (id, paths, identity_paths_hint, remote_connection_id, _session_id, timestamp) in
            self.recent_workspaces()?
        {
            if let Some(remote_connection_id) = remote_connection_id {
                if let Some(connection_options) = remote_connections.get(&remote_connection_id) {
                    result.push(RecentWorkspace {
                        workspace_id: id,
                        location: SerializedWorkspaceLocation::Remote(connection_options.clone()),
                        paths: paths.clone(),
                        identity_paths: identity_paths_hint.unwrap_or(paths),
                        timestamp,
                    });
                }
                continue;
            }

            if paths.paths().is_empty() || contains_wsl_path(&paths) {
                continue;
            }

            if Self::all_paths_exist_with_a_directory(paths.paths(), fs).await {
                let identity_paths = resolve_local_workspace_identity(fs, &paths)
                    .await
                    .or(identity_paths_hint)
                    .unwrap_or_else(|| paths.clone());
                result.push(RecentWorkspace {
                    workspace_id: id,
                    location: SerializedWorkspaceLocation::Local,
                    paths,
                    identity_paths,
                    timestamp,
                });
            }
        }

        Ok(result)
    }

    // Returns the recent project workspaces suitable for recent-project UIs.
    // Entries are deduplicated by git worktree identity, but preserve the original
    // serialized paths for reopening.
    pub async fn recent_project_workspaces(&self, fs: &dyn Fs) -> Result<Vec<RecentWorkspace>> {
        Ok(dedupe_recent_workspaces(
            self.recent_project_workspaces_ungrouped(fs).await?,
        ))
    }

    pub async fn delete_recent_workspace_group(
        &self,
        target: &RecentWorkspace,
    ) -> Result<Vec<WorkspaceId>> {
        let target_paths = &target.identity_paths;
        let target_remote_connection = match &target.location {
            SerializedWorkspaceLocation::Local => None,
            SerializedWorkspaceLocation::Remote(connection) => {
                Some(remote_connection_identity(connection))
            }
        };

        let remote_connections = self.remote_connections()?;

        let mut workspace_ids = Vec::new();
        for (workspace_id, paths, identity_paths, remote_connection_id, _, _) in
            self.recent_workspaces()?
        {
            let remote_connection = if let Some(id) = remote_connection_id {
                let Some(connection_options) = remote_connections.get(&id) else {
                    continue;
                };
                Some(remote_connection_identity(connection_options))
            } else {
                None
            };
            if remote_connection == target_remote_connection
                && &identity_paths.unwrap_or(paths) == target_paths
            {
                workspace_ids.push(workspace_id);
            }
        }

        futures::future::join_all(
            workspace_ids
                .iter()
                .copied()
                .map(|workspace_id| self.delete_workspace_by_id(workspace_id)),
        )
        .await;

        Ok(workspace_ids)
    }

    // Deletes workspace rows that can no longer be restored from. Remote workspaces whose
    // connection was removed, and (on Windows) workspaces pointing at WSL paths, are cleaned
    // up immediately. Local workspaces with no valid paths on disk are kept for seven days
    // after going stale. Workspaces belonging to the current session or the last session are
    // always preserved so that an in-progress restore can rehydrate them.
    pub async fn garbage_collect_workspaces(
        &self,
        fs: &dyn Fs,
        current_session_id: &str,
        last_session_id: Option<&str>,
    ) -> Result<()> {
        let remote_connections = self.remote_connections()?;
        let now = Utc::now();
        let mut workspaces_to_delete = Vec::new();
        for (id, paths, _identity_paths_hint, remote_connection_id, session_id, timestamp) in
            self.recent_workspaces()?
        {
            if let Some(session_id) = session_id.as_deref() {
                if session_id == current_session_id || Some(session_id) == last_session_id {
                    continue;
                }
            }

            if let Some(remote_connection_id) = remote_connection_id {
                if !remote_connections.contains_key(&remote_connection_id) {
                    workspaces_to_delete.push(id);
                }
                continue;
            }

            // Delete the workspace if any of the paths are WSL paths. If a
            // local workspace points to WSL, attempting to read its metadata
            // will wait for the WSL VM and file server to boot up. This can
            // block for many seconds. Supported scenarios use remote
            // workspaces.
            if contains_wsl_path(&paths) {
                workspaces_to_delete.push(id);
                continue;
            }

            if !Self::all_paths_exist_with_a_directory(paths.paths(), fs).await
                && now - timestamp >= chrono::Duration::days(7)
            {
                workspaces_to_delete.push(id);
            }
        }

        futures::future::join_all(
            workspaces_to_delete
                .into_iter()
                .map(|id| self.delete_workspace_by_id(id)),
        )
        .await;
        Ok(())
    }

    pub async fn last_workspace(&self, fs: &dyn Fs) -> Result<Option<RecentWorkspace>> {
        Ok(self.recent_project_workspaces(fs).await?.into_iter().next())
    }

    // Returns the locations of the workspaces that were still opened when the last
    // session was closed (i.e. when Mav was quit).
    // If `last_session_window_order` is provided, the returned locations are ordered
    // according to that.
    pub async fn last_session_workspace_locations(
        &self,
        last_session_id: &str,
        last_session_window_stack: Option<Vec<WindowId>>,
        fs: &dyn Fs,
    ) -> Result<Vec<SessionWorkspace>> {
        let mut workspaces = Vec::new();

        for (workspace_id, paths, window_id, remote_connection_id) in
            self.session_workspaces(last_session_id.to_owned())?
        {
            let window_id = window_id.map(WindowId::from);

            if let Some(remote_connection_id) = remote_connection_id {
                workspaces.push(SessionWorkspace {
                    workspace_id,
                    location: SerializedWorkspaceLocation::Remote(
                        self.remote_connection(remote_connection_id)?,
                    ),
                    paths,
                    window_id,
                });
                continue;
            }

            if paths.is_empty() || Self::all_paths_exist_with_a_directory(paths.paths(), fs).await {
                workspaces.push(SessionWorkspace {
                    workspace_id,
                    location: SerializedWorkspaceLocation::Local,
                    paths,
                    window_id,
                });
            }
        }

        if let Some(stack) = last_session_window_stack {
            workspaces.sort_by_key(|workspace| {
                workspace
                    .window_id
                    .and_then(|id| stack.iter().position(|&order_id| order_id == id))
                    .unwrap_or(usize::MAX)
            });
        }

        Ok(workspaces)
    }
}
