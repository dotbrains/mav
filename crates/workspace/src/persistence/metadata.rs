use super::*;

impl WorkspaceDb {
    pub(super) fn bookmarks(
        &self,
        workspace_id: WorkspaceId,
    ) -> BTreeMap<Arc<Path>, Vec<SerializedBookmark>> {
        let bookmarks: Result<Vec<(PathBuf, Bookmark)>> = self
            .select_bound(sql! {
                SELECT path, row, label
                FROM bookmarks
                WHERE workspace_id = ?
                ORDER BY path, row
            })
            .and_then(|mut prepared_statement| (prepared_statement)(workspace_id));

        match bookmarks {
            Ok(bookmarks) => {
                if bookmarks.is_empty() {
                    log::debug!("Bookmarks are empty after querying database for them");
                }

                let mut map: BTreeMap<_, Vec<_>> = BTreeMap::default();

                for (path, bookmark) in bookmarks {
                    let path: Arc<Path> = path.into();
                    map.entry(path.clone())
                        .or_default()
                        .push(SerializedBookmark {
                            row: bookmark.row,
                            label: bookmark.label,
                        })
                }

                map
            }
            Err(e) => {
                log::error!("Failed to load bookmarks: {}", e);
                BTreeMap::default()
            }
        }
    }

    pub(super) fn breakpoints(
        &self,
        workspace_id: WorkspaceId,
    ) -> BTreeMap<Arc<Path>, Vec<SourceBreakpoint>> {
        let breakpoints: Result<Vec<(PathBuf, Breakpoint)>> = self
            .select_bound(sql! {
                SELECT path, breakpoint_location, log_message, condition, hit_condition, state
                FROM breakpoints
                WHERE workspace_id = ?
            })
            .and_then(|mut prepared_statement| (prepared_statement)(workspace_id));

        match breakpoints {
            Ok(bp) => {
                if bp.is_empty() {
                    log::debug!("Breakpoints are empty after querying database for them");
                }

                let mut map: BTreeMap<Arc<Path>, Vec<SourceBreakpoint>> = Default::default();

                for (path, breakpoint) in bp {
                    let path: Arc<Path> = path.into();
                    map.entry(path.clone()).or_default().push(SourceBreakpoint {
                        row: breakpoint.position,
                        path,
                        message: breakpoint.message,
                        condition: breakpoint.condition,
                        hit_condition: breakpoint.hit_condition,
                        state: breakpoint.state,
                    });
                }

                for (path, bps) in map.iter() {
                    log::info!(
                        "Got {} breakpoints from database at path: {}",
                        bps.len(),
                        path.to_string_lossy()
                    );
                }

                map
            }
            Err(msg) => {
                log::error!("Breakpoints query failed with msg: {msg}");
                Default::default()
            }
        }
    }

    pub(super) fn user_toolchains(
        &self,
        workspace_id: WorkspaceId,
        remote_connection_id: Option<RemoteConnectionId>,
    ) -> BTreeMap<ToolchainScope, IndexSet<Toolchain>> {
        type RowKind = (WorkspaceId, String, String, String, String, String, String);

        let toolchains: Vec<RowKind> = self
            .select_bound(sql! {
                SELECT workspace_id, worktree_root_path, relative_worktree_path,
                language_name, name, path, raw_json
                FROM user_toolchains WHERE remote_connection_id IS ?1 AND (
                      workspace_id IN (0, ?2)
                )
            })
            .and_then(|mut statement| {
                (statement)((remote_connection_id.map(|id| id.0), workspace_id))
            })
            .unwrap_or_default();
        let mut ret = BTreeMap::<_, IndexSet<_>>::default();

        for (
            _workspace_id,
            worktree_root_path,
            relative_worktree_path,
            language_name,
            name,
            path,
            raw_json,
        ) in toolchains
        {
            // INTEGER's that are primary keys (like workspace ids, remote connection ids and such) start at 1, so we're safe to
            let scope = if _workspace_id == WorkspaceId(0) {
                debug_assert_eq!(worktree_root_path, String::default());
                debug_assert_eq!(relative_worktree_path, String::default());
                ToolchainScope::Global
            } else {
                debug_assert_eq!(workspace_id, _workspace_id);
                debug_assert_eq!(
                    worktree_root_path == String::default(),
                    relative_worktree_path == String::default()
                );

                let Some(relative_path) = RelPath::unix(&relative_worktree_path).log_err() else {
                    continue;
                };
                if worktree_root_path != String::default()
                    && relative_worktree_path != String::default()
                {
                    ToolchainScope::Subproject(
                        Arc::from(worktree_root_path.as_ref()),
                        relative_path.into(),
                    )
                } else {
                    ToolchainScope::Project
                }
            };
            let Ok(as_json) = serde_json::from_str(&raw_json) else {
                continue;
            };
            let toolchain = Toolchain {
                name: SharedString::from(name),
                path: SharedString::from(path),
                language_name: LanguageName::from_proto(language_name),
                as_json,
            };
            ret.entry(scope).or_default().insert(toolchain);
        }

        ret
    }
}
