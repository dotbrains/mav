use super::*;

impl WorkspaceDb {
    pub(crate) async fn save_workspace(&self, workspace: SerializedWorkspace) {
        let paths = workspace.paths.serialize();
        let identity_paths = workspace.identity_paths.map(|paths| paths.serialize());
        log::debug!("Saving workspace at location: {:?}", workspace.location);
        self.0.write(move |conn| {
            conn.with_savepoint("update_worktrees", || {
                let remote_connection_id = match workspace.location.clone() {
                    SerializedWorkspaceLocation::Local => None,
                    SerializedWorkspaceLocation::Remote(connection_options) => {
                        Some(Self::get_or_create_remote_connection_internal(
                            conn,
                            connection_options
                        )?.0)
                    }
                };

                // Clear out panes and pane_groups
                conn.exec_bound(sql!(
                    DELETE FROM pane_groups WHERE workspace_id = ?1;
                    DELETE FROM panes WHERE workspace_id = ?1;))?(workspace.id)
                    .context("Clearing old panes")?;

                conn.exec_bound(
                    sql!(
                        DELETE FROM bookmarks WHERE workspace_id = ?1;
                    )
                )?(workspace.id).context("Clearing old bookmarks")?;

                for (path, bookmarks) in workspace.bookmarks {
                    for bookmark in bookmarks {
                        conn.exec_bound(sql!(
                            INSERT INTO bookmarks (workspace_id, path, row, label)
                            VALUES (?1, ?2, ?3, ?4);
                        ))?((workspace.id, path.as_ref(), bookmark.row, bookmark.label)).context("Inserting bookmark")?;
                    }
                }

                conn.exec_bound(
                    sql!(
                        DELETE FROM breakpoints WHERE workspace_id = ?1;
                    )
                )?(workspace.id).context("Clearing old breakpoints")?;

                for (path, breakpoints) in workspace.breakpoints {
                    for bp in breakpoints {
                        let state = BreakpointStateWrapper::from(bp.state);
                        match conn.exec_bound(sql!(
                            INSERT INTO breakpoints (workspace_id, path, breakpoint_location,  log_message, condition, hit_condition, state)
                            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);))?

                        ((
                            workspace.id,
                            path.as_ref(),
                            bp.row,
                            bp.message,
                            bp.condition,
                            bp.hit_condition,
                            state,
                        )) {
                            Ok(_) => {
                                log::debug!("Stored breakpoint at row: {} in path: {}", bp.row, path.to_string_lossy())
                            }
                            Err(err) => {
                                log::error!("{err}");
                                continue;
                            }
                        }
                    }
                }

                conn.exec_bound(
                    sql!(
                        DELETE FROM user_toolchains WHERE workspace_id = ?1;
                    )
                )?(workspace.id).context("Clearing old user toolchains")?;

                for (scope, toolchains) in workspace.user_toolchains {
                    for toolchain in toolchains {
                        let query = sql!(INSERT OR REPLACE INTO user_toolchains(remote_connection_id, workspace_id, worktree_root_path, relative_worktree_path, language_name, name, path, raw_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8));
                        let (workspace_id, worktree_root_path, relative_worktree_path) = match scope {
                            ToolchainScope::Subproject(ref worktree_root_path, ref path) => (Some(workspace.id), Some(worktree_root_path.to_string_lossy().into_owned()), Some(path.as_unix_str().to_owned())),
                            ToolchainScope::Project => (Some(workspace.id), None, None),
                            ToolchainScope::Global => (None, None, None),
                        };
                        let args = (remote_connection_id, workspace_id.unwrap_or(WorkspaceId(0)), worktree_root_path.unwrap_or_default(), relative_worktree_path.unwrap_or_default(),
                        toolchain.language_name.as_ref().to_owned(), toolchain.name.to_string(), toolchain.path.to_string(), toolchain.as_json.to_string());
                        if let Err(err) = conn.exec_bound(query)?(args) {
                            log::error!("{err}");
                            continue;
                        }
                    }
                }

                // Clear out old workspaces with the same paths.
                // Skip this for empty workspaces - they are identified by workspace_id, not paths.
                // Multiple empty workspaces with different content should coexist.
                if !paths.paths.is_empty() {
                    conn.exec_bound(sql!(
                        DELETE
                        FROM workspaces
                        WHERE
                            workspace_id != ?1 AND
                            paths IS ?2 AND
                            remote_connection_id IS ?3
                    ))?((
                        workspace.id,
                        paths.paths.clone(),
                        remote_connection_id,
                    ))
                    .context("clearing out old locations")?;
                }

                // Upsert
                let query = sql!(
                    INSERT INTO workspaces(
                        workspace_id,
                        paths,
                        paths_order,
                        identity_paths,
                        identity_paths_order,
                        remote_connection_id,
                        left_dock_visible,
                        left_dock_active_panel,
                        left_dock_zoom,
                        right_dock_visible,
                        right_dock_active_panel,
                        right_dock_zoom,
                        bottom_dock_visible,
                        bottom_dock_active_panel,
                        bottom_dock_zoom,
                        session_id,
                        window_id,
                        timestamp
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, CURRENT_TIMESTAMP)
                    ON CONFLICT DO
                    UPDATE SET
                        paths = ?2,
                        paths_order = ?3,
                        identity_paths = ?4,
                        identity_paths_order = ?5,
                        remote_connection_id = ?6,
                        left_dock_visible = ?7,
                        left_dock_active_panel = ?8,
                        left_dock_zoom = ?9,
                        right_dock_visible = ?10,
                        right_dock_active_panel = ?11,
                        right_dock_zoom = ?12,
                        bottom_dock_visible = ?13,
                        bottom_dock_active_panel = ?14,
                        bottom_dock_zoom = ?15,
                        session_id = ?16,
                        window_id = ?17,
                        timestamp = CURRENT_TIMESTAMP
                );
                let mut prepared_query = conn.exec_bound(query)?;
                let args = (
                    workspace.id,
                    paths.paths.clone(),
                    paths.order.clone(),
                    identity_paths.as_ref().map(|paths| paths.paths.clone()),
                    identity_paths.as_ref().map(|paths| paths.order.clone()),
                    remote_connection_id,
                    workspace.docks,
                    workspace.session_id,
                    workspace.window_id,
                );

                prepared_query(args).context("Updating workspace")?;

                // Save center pane group
                Self::save_pane_group(conn, workspace.id, &workspace.center_group, None)
                    .context("save pane group in save workspace")?;

                Ok(())
            })
            .log_err();
        })
        .await;
    }
}
