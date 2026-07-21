use super::*;

impl LocalLspStore {
    fn register_watcher(
        &mut self,
        worktrees: &[Entity<Worktree>],
        watcher: &FileSystemWatcher,
        registration_id: &str,
        language_server_id: LanguageServerId,
        cx: &mut Context<LspStore>,
    ) {
        let watched = self
            .language_server_watched_paths
            .entry(language_server_id)
            .or_default();

        if let Some((worktree, literal_prefix, pattern)) =
            Self::worktree_and_path_for_file_watcher(worktrees, watcher, cx)
        {
            if worktree.read(cx).as_local().is_some() {
                if let Some(glob) = Glob::new(&pattern).log_err() {
                    let worktree_id = worktree.read(cx).id();
                    watched
                        .worktree_paths
                        .entry(worktree_id)
                        .or_default()
                        .add(registration_id, glob);
                    worktree.update(cx, |worktree, _| {
                        if let Some(tree) = worktree.as_local_mut() {
                            tree.add_path_prefix_to_scan(literal_prefix);
                        }
                    });
                }
            }

            return;
        }

        let (path, pattern) = match &watcher.glob_pattern {
            lsp::GlobPattern::String(s) => {
                let watcher_path = SanitizedPath::new(s);
                let path = glob_literal_prefix(watcher_path.as_path());
                let pattern = watcher_path
                    .as_path()
                    .strip_prefix(&path)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|e| {
                        debug_panic!(
                            "Failed to strip prefix for string pattern: {}, with prefix: {}, with error: {}",
                            s,
                            path.display(),
                            e
                        );
                        watcher_path.as_path().to_string_lossy().into_owned()
                    });
                (path, pattern)
            }
            lsp::GlobPattern::Relative(rp) => {
                let Ok(mut base_uri) = match &rp.base_uri {
                    lsp::OneOf::Left(workspace_folder) => &workspace_folder.uri,
                    lsp::OneOf::Right(base_uri) => base_uri,
                }
                .to_file_path() else {
                    return;
                };

                let path = glob_literal_prefix(Path::new(&rp.pattern));
                let pattern = Path::new(&rp.pattern)
                    .strip_prefix(&path)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|e| {
                        debug_panic!(
                            "Failed to strip prefix for relative pattern: {}, with prefix: {}, with error: {}",
                            rp.pattern,
                            path.display(),
                            e
                        );
                        rp.pattern.clone()
                    });
                base_uri.push(path);
                (base_uri, pattern)
            }
        };

        if let Some(glob) = Glob::new(&pattern).log_err() {
            if !path
                .components()
                .any(|c| matches!(c, path::Component::Normal(_)))
            {
                // For an unrooted glob like `**/Cargo.toml`, watch it within each worktree,
                // rather than adding a new watcher for `/`.
                for worktree in worktrees {
                    watched
                        .worktree_paths
                        .entry(worktree.read(cx).id())
                        .or_default()
                        .add(registration_id, glob.clone());
                }
            } else {
                let abs_path: Arc<Path> = path.into();
                let fs = self.fs.clone();
                let entry = watched
                    .abs_paths
                    .entry(abs_path.clone())
                    .or_insert_with(|| {
                        let task = LanguageServerWatchedPaths::spawn_abs_path_watcher(
                            abs_path,
                            fs,
                            language_server_id,
                            cx,
                        );
                        (LazyGlobSet::default(), task)
                    });
                entry.0.add(registration_id, glob);
            }
        }
    }

    fn worktree_and_path_for_file_watcher(
        worktrees: &[Entity<Worktree>],
        watcher: &FileSystemWatcher,
        cx: &App,
    ) -> Option<(Entity<Worktree>, Arc<RelPath>, String)> {
        worktrees.iter().find_map(|worktree| {
            let tree = worktree.read(cx);
            let worktree_root_path = tree.abs_path();
            let path_style = tree.path_style();
            match &watcher.glob_pattern {
                lsp::GlobPattern::String(s) => {
                    let watcher_path = SanitizedPath::new(s);
                    let relative = watcher_path
                        .as_path()
                        .strip_prefix(&worktree_root_path)
                        .ok()?;
                    let literal_prefix = glob_literal_prefix(relative);
                    Some((
                        worktree.clone(),
                        RelPath::new(&literal_prefix, path_style).ok()?.into_arc(),
                        relative.to_string_lossy().into_owned(),
                    ))
                }
                lsp::GlobPattern::Relative(rp) => {
                    let base_uri = match &rp.base_uri {
                        lsp::OneOf::Left(workspace_folder) => &workspace_folder.uri,
                        lsp::OneOf::Right(base_uri) => base_uri,
                    }
                    .to_file_path()
                    .ok()?;
                    let relative = base_uri.strip_prefix(&worktree_root_path).ok()?;
                    let mut literal_prefix = relative.to_owned();
                    literal_prefix.push(glob_literal_prefix(Path::new(&rp.pattern)));
                    Some((
                        worktree.clone(),
                        RelPath::new(&literal_prefix, path_style).ok()?.into_arc(),
                        rp.pattern.clone(),
                    ))
                }
            }
        })
    }

    pub(super) fn on_lsp_did_change_watched_files(
        &mut self,
        language_server_id: LanguageServerId,
        registration_id: &str,
        params: DidChangeWatchedFilesRegistrationOptions,
        cx: &mut Context<LspStore>,
    ) {
        log::trace!(
            "Processing new watcher paths for language server with id {}",
            language_server_id
        );

        let worktrees: Vec<Entity<Worktree>> = self
            .worktree_store
            .read(cx)
            .worktrees()
            .filter_map(|worktree| {
                self.language_servers_for_worktree(worktree.read(cx).id())
                    .find(|server| server.server_id() == language_server_id)
                    .map(|_| worktree)
            })
            .collect();

        for watcher in &params.watchers {
            self.register_watcher(&worktrees, watcher, registration_id, language_server_id, cx);
        }

        let registrations = self
            .language_server_dynamic_registrations
            .entry(language_server_id)
            .or_default();
        registrations
            .did_change_watched_files
            .insert(registration_id.to_string());

        cx.notify();
    }

    pub(super) fn on_lsp_unregister_did_change_watched_files(
        &mut self,
        language_server_id: LanguageServerId,
        registration_id: &str,
        cx: &mut Context<LspStore>,
    ) {
        let Some(registrations) = self
            .language_server_dynamic_registrations
            .get_mut(&language_server_id)
        else {
            return;
        };

        if registrations
            .did_change_watched_files
            .remove(registration_id)
        {
            log::info!(
                "language server {}: unregistered workspace/DidChangeWatchedFiles capability with id {}",
                language_server_id,
                registration_id
            );
        } else {
            log::warn!(
                "language server {}: failed to unregister workspace/DidChangeWatchedFiles capability with id {}. not registered.",
                language_server_id,
                registration_id
            );
            return;
        }

        if let Some(watched) = self
            .language_server_watched_paths
            .get_mut(&language_server_id)
        {
            watched.worktree_paths.retain(|_, glob_set| {
                glob_set.remove(registration_id);
                !glob_set.is_empty()
            });
            watched.abs_paths.retain(|_, (glob_set, _)| {
                glob_set.remove(registration_id);
                !glob_set.is_empty()
            });
        }

        cx.notify();
    }
}
