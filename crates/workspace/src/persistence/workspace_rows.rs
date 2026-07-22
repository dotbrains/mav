use super::*;

impl WorkspaceDb {
    /// Returns a serialized workspace for the given worktree_roots. If the passed array
    /// is empty, the most recent workspace is returned instead. If no workspace for the
    /// passed roots is stored, returns none.
    pub(crate) fn workspace_for_roots<P: AsRef<Path>>(
        &self,
        worktree_roots: &[P],
    ) -> Option<SerializedWorkspace> {
        self.workspace_for_roots_internal(worktree_roots, None)
    }

    pub(crate) fn remote_workspace_for_roots<P: AsRef<Path>>(
        &self,
        worktree_roots: &[P],
        remote_project_id: RemoteConnectionId,
    ) -> Option<SerializedWorkspace> {
        self.workspace_for_roots_internal(worktree_roots, Some(remote_project_id))
    }

    pub(crate) fn workspace_for_roots_internal<P: AsRef<Path>>(
        &self,
        worktree_roots: &[P],
        remote_connection_id: Option<RemoteConnectionId>,
    ) -> Option<SerializedWorkspace> {
        // paths are sorted before db interactions to ensure that the order of the paths
        // doesn't affect the workspace selection for existing workspaces
        let root_paths = PathList::new(worktree_roots);

        // Empty workspaces cannot be matched by paths (all empty workspaces have paths = "").
        // They should only be restored via workspace_for_id during session restoration.
        if root_paths.is_empty() && remote_connection_id.is_none() {
            return None;
        }

        // Note that we re-assign the workspace_id here in case it's empty
        // and we've grabbed the most recent workspace
        let (
            workspace_id,
            paths,
            paths_order,
            identity_paths,
            identity_paths_order,
            window_bounds,
            display,
            centered_layout,
            docks,
            window_id,
        ): (
            WorkspaceId,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<SerializedWindowBounds>,
            Option<Uuid>,
            Option<bool>,
            DockStructure,
            Option<u64>,
        ) = self
            .select_row_bound(sql! {
                SELECT
                    workspace_id,
                    paths,
                    paths_order,
                    identity_paths,
                    identity_paths_order,
                    window_state,
                    window_x,
                    window_y,
                    window_width,
                    window_height,
                    display,
                    centered_layout,
                    left_dock_visible,
                    left_dock_active_panel,
                    left_dock_zoom,
                    right_dock_visible,
                    right_dock_active_panel,
                    right_dock_zoom,
                    bottom_dock_visible,
                    bottom_dock_active_panel,
                    bottom_dock_zoom,
                    window_id
                FROM workspaces
                WHERE
                    paths IS ? AND
                    remote_connection_id IS ?
                LIMIT 1
            })
            .and_then(|mut prepared_statement| {
                (prepared_statement)((
                    root_paths.serialize().paths,
                    remote_connection_id.map(|id| id.0 as i32),
                ))
            })
            .context("No workspaces found")
            .warn_on_err()
            .flatten()?;

        let paths = PathList::deserialize(&SerializedPathList {
            paths,
            order: paths_order,
        });
        let identity_paths = identity_paths.map(|paths| {
            PathList::deserialize(&SerializedPathList {
                paths,
                order: identity_paths_order.unwrap_or_default(),
            })
        });

        let remote_connection_options = if let Some(remote_connection_id) = remote_connection_id {
            self.remote_connection(remote_connection_id)
                .context("Get remote connection")
                .log_err()
        } else {
            None
        };

        Some(SerializedWorkspace {
            id: workspace_id,
            location: match remote_connection_options {
                Some(options) => SerializedWorkspaceLocation::Remote(options),
                None => SerializedWorkspaceLocation::Local,
            },
            paths,
            identity_paths,
            center_group: self
                .get_center_pane_group(workspace_id)
                .context("Getting center group")
                .log_err()?,
            window_bounds,
            centered_layout: centered_layout.unwrap_or(false),
            display,
            docks,
            session_id: None,
            bookmarks: self.bookmarks(workspace_id),
            breakpoints: self.breakpoints(workspace_id),
            window_id,
            user_toolchains: self.user_toolchains(workspace_id, remote_connection_id),
        })
    }

    /// Returns the workspace with the given ID, loading all associated data.
    pub(crate) fn workspace_for_id(
        &self,
        workspace_id: WorkspaceId,
    ) -> Option<SerializedWorkspace> {
        let (
            paths,
            paths_order,
            identity_paths,
            identity_paths_order,
            window_bounds,
            display,
            centered_layout,
            docks,
            window_id,
            remote_connection_id,
        ): (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<SerializedWindowBounds>,
            Option<Uuid>,
            Option<bool>,
            DockStructure,
            Option<u64>,
            Option<i32>,
        ) = self
            .select_row_bound(sql! {
                SELECT
                    paths,
                    paths_order,
                    identity_paths,
                    identity_paths_order,
                    window_state,
                    window_x,
                    window_y,
                    window_width,
                    window_height,
                    display,
                    centered_layout,
                    left_dock_visible,
                    left_dock_active_panel,
                    left_dock_zoom,
                    right_dock_visible,
                    right_dock_active_panel,
                    right_dock_zoom,
                    bottom_dock_visible,
                    bottom_dock_active_panel,
                    bottom_dock_zoom,
                    window_id,
                    remote_connection_id
                FROM workspaces
                WHERE workspace_id = ?
            })
            .and_then(|mut prepared_statement| (prepared_statement)(workspace_id))
            .context("No workspace found for id")
            .warn_on_err()
            .flatten()?;

        let paths = PathList::deserialize(&SerializedPathList {
            paths,
            order: paths_order,
        });
        let identity_paths = identity_paths.map(|paths| {
            PathList::deserialize(&SerializedPathList {
                paths,
                order: identity_paths_order.unwrap_or_default(),
            })
        });

        let remote_connection_id = remote_connection_id.map(|id| RemoteConnectionId(id as u64));
        let remote_connection_options = if let Some(remote_connection_id) = remote_connection_id {
            self.remote_connection(remote_connection_id)
                .context("Get remote connection")
                .log_err()
        } else {
            None
        };

        Some(SerializedWorkspace {
            id: workspace_id,
            location: match remote_connection_options {
                Some(options) => SerializedWorkspaceLocation::Remote(options),
                None => SerializedWorkspaceLocation::Local,
            },
            paths,
            identity_paths,
            center_group: self
                .get_center_pane_group(workspace_id)
                .context("Getting center group")
                .log_err()?,
            window_bounds,
            centered_layout: centered_layout.unwrap_or(false),
            display,
            docks,
            session_id: None,
            bookmarks: self.bookmarks(workspace_id),
            breakpoints: self.breakpoints(workspace_id),
            window_id,
            user_toolchains: self.user_toolchains(workspace_id, remote_connection_id),
        })
    }
}
