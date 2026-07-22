use super::*;

impl Project {
    fn search_impl(&mut self, query: SearchQuery, cx: &mut Context<Self>) -> SearchResultsHandle {
        let client: Option<(AnyProtoClient, _)> = if let Some(ssh_client) = &self.remote_client {
            Some((ssh_client.read(cx).proto_client(), 0))
        } else if let Some(remote_id) = self.remote_id() {
            self.is_local()
                .not()
                .then(|| (self.collab_client.clone().into(), remote_id))
        } else {
            None
        };
        let searcher = if query.is_opened_only() {
            project_search::Search::open_buffers_only(
                self.buffer_store.clone(),
                self.worktree_store.clone(),
                project_search::Search::MAX_SEARCH_RESULT_FILES + 1,
            )
        } else {
            match client {
                Some((client, remote_id)) => project_search::Search::remote(
                    self.buffer_store.clone(),
                    self.worktree_store.clone(),
                    project_search::Search::MAX_SEARCH_RESULT_FILES + 1,
                    (client, remote_id, self.remotely_created_models.clone()),
                ),
                None => project_search::Search::local(
                    self.fs.clone(),
                    self.buffer_store.clone(),
                    self.worktree_store.clone(),
                    project_search::Search::MAX_SEARCH_RESULT_FILES + 1,
                    cx,
                ),
            }
        };
        searcher.into_handle(query, cx)
    }

    pub fn search(
        &mut self,
        query: SearchQuery,
        cx: &mut Context<Self>,
    ) -> SearchResults<SearchResult> {
        self.search_impl(query, cx).results(cx)
    }

    pub fn request_lsp<R: LspCommand>(
        &mut self,
        buffer_handle: Entity<Buffer>,
        server: LanguageServerToQuery,
        request: R,
        cx: &mut Context<Self>,
    ) -> Task<Result<R::Response>>
    where
        <R::LspRequest as lsp::request::Request>::Result: Send,
        <R::LspRequest as lsp::request::Request>::Params: Send,
    {
        let guard = self.retain_remotely_created_models(cx);
        let task = self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.request_lsp(buffer_handle, server, request, cx)
        });
        cx.background_spawn(async move {
            let result = task.await;
            drop(guard);
            result
        })
    }

    /// Move a worktree to a new position in the worktree order.
    ///
    /// The worktree will moved to the opposite side of the destination worktree.
    ///
    /// # Example
    ///
    /// Given the worktree order `[11, 22, 33]` and a call to move worktree `22` to `33`,
    /// worktree_order will be updated to produce the indexes `[11, 33, 22]`.
    ///
    /// Given the worktree order `[11, 22, 33]` and a call to move worktree `22` to `11`,
    /// worktree_order will be updated to produce the indexes `[22, 11, 33]`.
    ///
    /// # Errors
    ///
    /// An error will be returned if the worktree or destination worktree are not found.
    pub fn move_worktree(
        &mut self,
        source: WorktreeId,
        destination: WorktreeId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.move_worktree(source, destination, cx)
        })
    }

    /// Attempts to convert the input path to a WSL path if this is a wsl remote project and the input path is a host windows path.
    pub fn try_windows_path_to_wsl(
        &self,
        abs_path: &Path,
        cx: &App,
    ) -> impl Future<Output = Result<PathBuf>> + use<> {
        let fut = if cfg!(windows)
            && let (
                ProjectClientState::Local | ProjectClientState::Shared { .. },
                Some(remote_client),
            ) = (&self.client_state, &self.remote_client)
            && let RemoteConnectionOptions::Wsl(wsl) = remote_client.read(cx).connection_options()
        {
            Either::Left(wsl.abs_windows_path_to_wsl_path(abs_path))
        } else {
            Either::Right(abs_path.to_owned())
        };
        async move {
            match fut {
                Either::Left(fut) => fut.await.map(Into::into),
                Either::Right(path) => Ok(path),
            }
        }
    }

    pub fn find_or_create_worktree(
        &mut self,
        abs_path: impl AsRef<Path>,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<(Entity<Worktree>, Arc<RelPath>)>> {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.find_or_create_worktree(abs_path, visible, cx)
        })
    }

    pub fn find_worktree(
        &self,
        abs_path: &Path,
        cx: &App,
    ) -> Option<(Entity<Worktree>, Arc<RelPath>)> {
        self.worktree_store.read(cx).find_worktree(abs_path, cx)
    }

    pub fn is_shared(&self) -> bool {
        match &self.client_state {
            ProjectClientState::Shared { .. } => true,
            ProjectClientState::Local => false,
            ProjectClientState::Collab { .. } => true,
        }
    }

    /// Returns the resolved version of `path`, that was found in `buffer`, if it exists.
    pub fn resolve_path_in_buffer(
        &self,
        path: &str,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Option<ResolvedPath>> {
        if util::paths::is_absolute(path, self.path_style(cx)) || path.starts_with("~") {
            self.resolve_abs_path(path, cx)
        } else {
            self.resolve_path_in_worktrees(path, buffer, cx)
        }
    }

    pub fn resolve_abs_file_path(
        &self,
        path: &str,
        cx: &mut Context<Self>,
    ) -> Task<Option<ResolvedPath>> {
        let resolve_task = self.resolve_abs_path(path, cx);
        cx.background_spawn(async move {
            let resolved_path = resolve_task.await;
            resolved_path.filter(|path| path.is_file())
        })
    }

    pub fn resolve_abs_path(&self, path: &str, cx: &App) -> Task<Option<ResolvedPath>> {
        if self.is_local() {
            let expanded = PathBuf::from(shellexpand::tilde(&path).into_owned());
            let fs = self.fs.clone();
            cx.background_spawn(async move {
                let metadata = fs.metadata(&expanded).await.ok().flatten();

                metadata.map(|metadata| ResolvedPath::AbsPath {
                    path: expanded.to_string_lossy().into_owned(),
                    is_dir: metadata.is_dir,
                })
            })
        } else if let Some(ssh_client) = self.remote_client.as_ref() {
            let request = ssh_client
                .read(cx)
                .proto_client()
                .request(proto::GetPathMetadata {
                    project_id: REMOTE_SERVER_PROJECT_ID,
                    path: path.into(),
                });
            cx.background_spawn(async move {
                let response = request.await.log_err()?;
                if response.exists {
                    Some(ResolvedPath::AbsPath {
                        path: response.path,
                        is_dir: response.is_dir,
                    })
                } else {
                    None
                }
            })
        } else {
            Task::ready(None)
        }
    }

    fn resolve_path_in_worktrees(
        &self,
        path: &str,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Option<ResolvedPath>> {
        let mut candidates = vec![];
        let path_style = self.path_style(cx);
        if let Ok(path) = RelPath::new(path.as_ref(), path_style) {
            candidates.push(path.into_arc());
        }

        if let Some(file) = buffer.read(cx).file()
            && let Some(dir) = file.path().parent()
        {
            if let Some(joined) = path_style.join(&*dir.display(path_style), path)
                && let Some(joined) = RelPath::new(joined.as_ref(), path_style).ok()
            {
                candidates.push(joined.into_arc());
            }
        }

        let buffer_worktree_id = buffer.read(cx).file().map(|file| file.worktree_id(cx));
        let worktrees_with_ids: Vec<_> = self
            .worktrees(cx)
            .map(|worktree| {
                let id = worktree.read(cx).id();
                (worktree, id)
            })
            .collect();

        cx.spawn(async move |_, cx| {
            if let Some(buffer_worktree_id) = buffer_worktree_id
                && let Some((worktree, _)) = worktrees_with_ids
                    .iter()
                    .find(|(_, id)| *id == buffer_worktree_id)
            {
                for candidate in candidates.iter() {
                    if let Some(path) = Self::resolve_path_in_worktree(worktree, candidate, cx) {
                        return Some(path);
                    }
                }
            }
            for (worktree, id) in worktrees_with_ids {
                if Some(id) == buffer_worktree_id {
                    continue;
                }
                for candidate in candidates.iter() {
                    if let Some(path) = Self::resolve_path_in_worktree(&worktree, candidate, cx) {
                        return Some(path);
                    }
                }
            }
            None
        })
    }

    fn resolve_path_in_worktree(
        worktree: &Entity<Worktree>,
        path: &RelPath,
        cx: &mut AsyncApp,
    ) -> Option<ResolvedPath> {
        worktree.read_with(cx, |worktree, _| {
            worktree.entry_for_path(path).map(|entry| {
                let project_path = ProjectPath {
                    worktree_id: worktree.id(),
                    path: entry.path.clone(),
                };
                ResolvedPath::ProjectPath {
                    project_path,
                    is_dir: entry.is_dir(),
                }
            })
        })
    }

    pub fn list_directory(
        &self,
        query: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<DirectoryItem>>> {
        if self.is_local() {
            DirectoryLister::Local(cx.entity(), self.fs.clone()).list_directory(query, cx)
        } else if let Some(session) = self.remote_client.as_ref() {
            let request = proto::ListRemoteDirectory {
                dev_server_id: REMOTE_SERVER_PROJECT_ID,
                path: query,
                config: Some(proto::ListRemoteDirectoryConfig { is_dir: true }),
            };

            let response = session.read(cx).proto_client().request(request);
            cx.background_spawn(async move {
                let proto::ListRemoteDirectoryResponse {
                    entries,
                    entry_info,
                } = response.await?;
                Ok(entries
                    .into_iter()
                    .zip(entry_info)
                    .map(|(entry, info)| DirectoryItem {
                        path: PathBuf::from(entry),
                        is_dir: info.is_dir,
                    })
                    .collect())
            })
        } else {
            Task::ready(Err(anyhow!("cannot list directory in remote project")))
        }
    }

    pub fn create_worktree(
        &mut self,
        abs_path: impl AsRef<Path>,
        visible: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Worktree>>> {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.create_worktree(abs_path, visible, cx)
        })
    }

    /// Returns a task that resolves when the given worktree's `Entity` is
    /// fully dropped (all strong references released), not merely when
    /// `remove_worktree` is called. `remove_worktree` drops the store's
    /// reference and emits `WorktreeRemoved`, but other code may still
    /// hold a strong handle — the worktree isn't safe to delete from
    /// disk until every handle is gone.
    ///
    /// We use `observe_release` on the specific entity rather than
    /// listening for `WorktreeReleased` events because it's simpler at
    /// the call site (one awaitable task, no subscription / channel /
    /// ID filtering).
    pub fn wait_for_worktree_release(
        &mut self,
        worktree_id: WorktreeId,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(worktree) = self.worktree_for_id(worktree_id, cx) else {
            return Task::ready(Ok(()));
        };

        let (released_tx, released_rx) = futures::channel::oneshot::channel();
        let released_tx = std::sync::Arc::new(Mutex::new(Some(released_tx)));
        let release_subscription =
            cx.observe_release(&worktree, move |_project, _released_worktree, _cx| {
                if let Some(released_tx) = released_tx.lock().take() {
                    let _ = released_tx.send(());
                }
            });

        cx.spawn(async move |_project, _cx| {
            let _release_subscription = release_subscription;
            released_rx
                .await
                .map_err(|_| anyhow!("worktree release observer dropped before release"))?;
            Ok(())
        })
    }

    pub fn remove_worktree(&mut self, id_to_remove: WorktreeId, cx: &mut Context<Self>) {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.remove_worktree(id_to_remove, cx);
        });
    }

    pub fn remove_worktree_for_main_worktree_path(
        &mut self,
        path: impl AsRef<Path>,
        cx: &mut Context<Self>,
    ) {
        let path = path.as_ref();
        self.worktree_store.update(cx, |worktree_store, cx| {
            if let Some(worktree) = worktree_store.worktree_for_main_worktree_path(path, cx) {
                worktree_store.remove_worktree(worktree.read(cx).id(), cx);
            }
        });
    }

    fn add_worktree(&mut self, worktree: &Entity<Worktree>, cx: &mut Context<Self>) {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.add(worktree, cx);
        });
    }
}
