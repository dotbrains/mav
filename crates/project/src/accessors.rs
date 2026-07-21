use super::*;

impl Project {
    #[inline]
    pub fn dap_store(&self) -> Entity<DapStore> {
        self.dap_store.clone()
    }

    #[inline]
    pub fn bookmark_store(&self) -> Entity<BookmarkStore> {
        self.bookmark_store.clone()
    }

    #[inline]
    pub fn breakpoint_store(&self) -> Entity<BreakpointStore> {
        self.breakpoint_store.clone()
    }

    pub fn active_debug_session(&self, cx: &App) -> Option<(Entity<Session>, ActiveStackFrame)> {
        let active_position = self.breakpoint_store.read(cx).active_position()?;
        let session = self
            .dap_store
            .read(cx)
            .session_by_id(active_position.session_id)?;
        Some((session, active_position.clone()))
    }

    #[inline]
    pub fn lsp_store(&self) -> Entity<LspStore> {
        self.lsp_store.clone()
    }

    #[inline]
    pub fn worktree_store(&self) -> Entity<WorktreeStore> {
        self.worktree_store.clone()
    }

    /// Returns a future that resolves when all visible worktrees have completed
    /// their initial scan.
    pub fn wait_for_initial_scan(&self, cx: &App) -> impl Future<Output = ()> + use<> {
        self.worktree_store.read(cx).wait_for_initial_scan()
    }

    #[inline]
    pub fn context_server_store(&self) -> Entity<ContextServerStore> {
        self.context_server_store.clone()
    }

    #[inline]
    pub fn buffer_for_id(&self, remote_id: BufferId, cx: &App) -> Option<Entity<Buffer>> {
        self.buffer_store.read(cx).get(remote_id)
    }

    #[inline]
    pub fn languages(&self) -> &Arc<LanguageRegistry> {
        &self.languages
    }

    #[inline]
    pub fn client(&self) -> Arc<Client> {
        self.collab_client.clone()
    }

    #[inline]
    pub fn remote_client(&self) -> Option<Entity<RemoteClient>> {
        self.remote_client.clone()
    }

    #[inline]
    pub fn user_store(&self) -> Entity<UserStore> {
        self.user_store.clone()
    }

    #[inline]
    pub fn node_runtime(&self) -> Option<&NodeRuntime> {
        self.node.as_ref()
    }

    #[inline]
    pub fn opened_buffers(&self, cx: &App) -> Vec<Entity<Buffer>> {
        self.buffer_store.read(cx).buffers().collect()
    }

    #[inline]
    pub fn environment(&self) -> &Entity<ProjectEnvironment> {
        &self.environment
    }

    #[inline]
    pub fn cli_environment(&self, cx: &App) -> Option<HashMap<String, String>> {
        self.environment.read(cx).get_cli_environment()
    }

    #[inline]
    pub fn peek_environment_error<'a>(&'a self, cx: &'a App) -> Option<&'a String> {
        self.environment.read(cx).peek_environment_error()
    }

    #[inline]
    pub fn pop_environment_error(&mut self, cx: &mut Context<Self>) {
        self.environment.update(cx, |environment, _| {
            environment.pop_environment_error();
        });
    }

    #[cfg(feature = "test-support")]
    #[inline]
    pub fn has_open_buffer(&self, path: impl Into<ProjectPath>, cx: &App) -> bool {
        self.buffer_store
            .read(cx)
            .get_by_path(&path.into())
            .is_some()
    }

    #[inline]
    pub fn fs(&self) -> &Arc<dyn Fs> {
        &self.fs
    }

    #[inline]
    pub fn remote_id(&self) -> Option<u64> {
        match self.client_state {
            ProjectClientState::Local => None,
            ProjectClientState::Shared { remote_id, .. }
            | ProjectClientState::Collab { remote_id, .. } => Some(remote_id),
        }
    }

    #[inline]
    pub fn supports_terminal(&self, _cx: &App) -> bool {
        self.is_local() || self.is_via_remote_server()
    }

    #[inline]
    pub fn remote_connection_state(&self, cx: &App) -> Option<remote::ConnectionState> {
        self.remote_client
            .as_ref()
            .map(|remote| remote.read(cx).connection_state())
    }

    #[inline]
    pub fn remote_connection_options(&self, cx: &App) -> Option<RemoteConnectionOptions> {
        self.remote_client
            .as_ref()
            .map(|remote| remote.read(cx).connection_options())
    }

    /// Reveals the given path in the system file manager.
    ///
    /// On Windows with a WSL remote connection, this converts the POSIX path
    /// to a Windows UNC path before revealing.
    pub fn reveal_path(&self, path: &Path, cx: &mut Context<Self>) {
        #[cfg(target_os = "windows")]
        if let Some(RemoteConnectionOptions::Wsl(wsl_options)) = self.remote_connection_options(cx)
        {
            let path = path.to_path_buf();
            cx.spawn(async move |_, cx| {
                wsl_path_to_windows_path(&wsl_options, &path)
                    .await
                    .map(|windows_path| cx.update(|cx| cx.reveal_path(&windows_path)))
            })
            .detach_and_log_err(cx);
            return;
        }

        cx.reveal_path(path);
    }

    #[inline]
    pub fn replica_id(&self) -> ReplicaId {
        match self.client_state {
            ProjectClientState::Collab { replica_id, .. } => replica_id,
            _ => {
                if self.remote_client.is_some() {
                    ReplicaId::REMOTE_SERVER
                } else {
                    ReplicaId::LOCAL
                }
            }
        }
    }

    #[inline]
    pub fn task_store(&self) -> &Entity<TaskStore> {
        &self.task_store
    }

    #[inline]
    pub fn snippets(&self) -> &Entity<SnippetProvider> {
        &self.snippets
    }

    #[inline]
    pub fn search_history(&self, kind: SearchInputKind) -> &SearchHistory {
        match kind {
            SearchInputKind::Query => &self.search_history,
            SearchInputKind::Include => &self.search_included_history,
            SearchInputKind::Exclude => &self.search_excluded_history,
        }
    }

    #[inline]
    pub fn search_history_mut(&mut self, kind: SearchInputKind) -> &mut SearchHistory {
        match kind {
            SearchInputKind::Query => &mut self.search_history,
            SearchInputKind::Include => &mut self.search_included_history,
            SearchInputKind::Exclude => &mut self.search_excluded_history,
        }
    }

    #[inline]
    pub fn collaborators(&self) -> &HashMap<proto::PeerId, Collaborator> {
        &self.collaborators
    }

    #[inline]
    pub fn host(&self) -> Option<&Collaborator> {
        self.collaborators.values().find(|c| c.is_host)
    }

    /// Collect all worktrees, including ones that don't appear in the project panel
    #[inline]
    pub fn worktrees<'a>(
        &self,
        cx: &'a App,
    ) -> impl 'a + DoubleEndedIterator<Item = Entity<Worktree>> {
        self.worktree_store.read(cx).worktrees()
    }

    /// Collect all user-visible worktrees, the ones that appear in the project panel.
    #[inline]
    pub fn visible_worktrees<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl 'a + DoubleEndedIterator<Item = Entity<Worktree>> {
        self.worktree_store.read(cx).visible_worktrees(cx)
    }

    pub(crate) fn default_visible_worktree_paths(
        worktree_store: &WorktreeStore,
        cx: &App,
    ) -> Vec<PathBuf> {
        worktree_store
            .visible_worktrees(cx)
            .sorted_by(|left, right| {
                left.read(cx)
                    .is_single_file()
                    .cmp(&right.read(cx).is_single_file())
            })
            .filter_map(|worktree| {
                let worktree = worktree.read(cx);
                let path = worktree.abs_path();
                if worktree.is_single_file() {
                    Some(path.parent()?.to_path_buf())
                } else {
                    Some(path.to_path_buf())
                }
            })
            .collect()
    }

    pub fn default_path_list(&self, cx: &App) -> PathList {
        let worktree_roots =
            Self::default_visible_worktree_paths(&self.worktree_store.read(cx), cx);

        if worktree_roots.is_empty() {
            PathList::new(&[paths::home_dir().as_path()])
        } else {
            PathList::new(&worktree_roots)
        }
    }

}
