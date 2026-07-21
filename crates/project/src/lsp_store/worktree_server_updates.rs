use super::*;

impl LspStore {
    pub(super) fn update_local_worktree_language_servers(
        &mut self,
        worktree_handle: &Entity<Worktree>,
        changes: &[(Arc<RelPath>, ProjectEntryId, PathChange)],
        cx: &mut Context<Self>,
    ) {
        if changes.is_empty() {
            return;
        }

        let Some(local) = self.as_local_mut() else {
            return;
        };

        local.prettier_store.update(cx, |prettier_store, cx| {
            prettier_store.update_prettier_settings(worktree_handle, changes, cx)
        });

        let worktree_id = worktree_handle.read(cx).id();
        let mut language_server_ids = local
            .language_server_ids
            .iter()
            .filter_map(|(seed, v)| seed.worktree_id.eq(&worktree_id).then(|| v.id))
            .collect::<Vec<_>>();
        language_server_ids.sort_unstable();
        language_server_ids.dedup();

        // let abs_path = worktree_handle.read(cx).abs_path();
        for server_id in &language_server_ids {
            if let Some(LanguageServerState::Running { server, .. }) =
                local.language_servers.get(server_id)
                && let Some(watched_paths) = local
                    .language_server_watched_paths
                    .get_mut(server_id)
                    .and_then(|paths| paths.worktree_paths.get_mut(&worktree_id))
            {
                let params = lsp::DidChangeWatchedFilesParams {
                    changes: changes
                        .iter()
                        .filter_map(|(path, _, change)| {
                            let typ = match change {
                                PathChange::Loaded => return None,
                                PathChange::Added => lsp::FileChangeType::CREATED,
                                PathChange::Removed => lsp::FileChangeType::DELETED,
                                PathChange::Updated => lsp::FileChangeType::CHANGED,
                                PathChange::AddedOrUpdated => lsp::FileChangeType::CHANGED,
                            };
                            if !watched_paths.is_match(path.as_std_path()) {
                                return None;
                            }
                            let uri = lsp::Uri::from_file_path(
                                worktree_handle.read(cx).absolutize(&path),
                            )
                            .ok()?;
                            Some(lsp::FileEvent { uri, typ })
                        })
                        .collect(),
                };
                if !params.changes.is_empty() {
                    server
                        .notify::<lsp::notification::DidChangeWatchedFiles>(params)
                        .ok();
                }
            }
        }
        for (path, _, _) in changes {
            if let Some(file_name) = path.file_name()
                && local.watched_manifest_filenames.contains(file_name)
            {
                self.request_workspace_config_refresh();
                break;
            }
        }
    }
}
