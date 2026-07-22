use super::*;

impl Project {
    pub fn supplementary_language_servers<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl 'a + Iterator<Item = (LanguageServerId, LanguageServerName)> {
        self.lsp_store.read(cx).supplementary_language_servers()
    }

    pub fn any_language_server_supports_inlay_hints(&self, buffer: &Buffer, cx: &mut App) -> bool {
        let Some(language) = buffer.language().cloned() else {
            return false;
        };
        self.lsp_store.update(cx, |lsp_store, _| {
            let relevant_language_servers = lsp_store
                .languages
                .lsp_adapters(&language.name())
                .into_iter()
                .map(|lsp_adapter| lsp_adapter.name())
                .collect::<HashSet<_>>();
            lsp_store
                .language_server_statuses()
                .filter_map(|(server_id, server_status)| {
                    relevant_language_servers
                        .contains(&server_status.name)
                        .then_some(server_id)
                })
                .filter_map(|server_id| lsp_store.lsp_server_capabilities.get(&server_id))
                .any(InlayHints::check_capabilities)
        })
    }

    pub fn any_language_server_supports_semantic_tokens(
        &self,
        buffer: &Buffer,
        cx: &mut App,
    ) -> bool {
        let Some(language) = buffer.language().cloned() else {
            return false;
        };
        let lsp_store = self.lsp_store.read(cx);
        let relevant_language_servers = lsp_store
            .languages
            .lsp_adapters(&language.name())
            .into_iter()
            .map(|lsp_adapter| lsp_adapter.name())
            .collect::<HashSet<_>>();
        lsp_store
            .language_server_statuses()
            .filter_map(|(server_id, server_status)| {
                relevant_language_servers
                    .contains(&server_status.name)
                    .then_some(server_id)
            })
            .filter_map(|server_id| lsp_store.lsp_server_capabilities.get(&server_id))
            .any(|capabilities| capabilities.semantic_tokens_provider.is_some())
    }

    pub fn language_server_id_for_name(
        &self,
        buffer: &Buffer,
        name: &LanguageServerName,
        cx: &App,
    ) -> Option<LanguageServerId> {
        let language = buffer.language()?;
        let relevant_language_servers = self
            .languages
            .lsp_adapters(&language.name())
            .into_iter()
            .map(|lsp_adapter| lsp_adapter.name())
            .collect::<HashSet<_>>();
        if !relevant_language_servers.contains(name) {
            return None;
        }
        self.language_server_statuses(cx)
            .filter(|(_, server_status)| relevant_language_servers.contains(&server_status.name))
            .find_map(|(server_id, server_status)| {
                if &server_status.name == name {
                    Some(server_id)
                } else {
                    None
                }
            })
    }

    #[cfg(feature = "test-support")]
    pub fn has_language_servers_for(&self, buffer: &Buffer, cx: &mut App) -> bool {
        self.lsp_store.update(cx, |this, cx| {
            this.running_language_servers_for_local_buffer(buffer, cx)
                .next()
                .is_some()
        })
    }

    pub fn git_init(
        &self,
        path: Arc<Path>,
        fallback_branch_name: String,
        cx: &App,
    ) -> Task<Result<()>> {
        self.git_store
            .read(cx)
            .git_init(path, fallback_branch_name, cx)
    }

    pub fn git_config(&self, path: Arc<Path>, args: Vec<String>, cx: &App) -> Task<Result<String>> {
        self.git_store.read(cx).git_config(path, args, cx)
    }

    pub fn buffer_store(&self) -> &Entity<BufferStore> {
        &self.buffer_store
    }

    pub fn git_store(&self) -> &Entity<GitStore> {
        &self.git_store
    }

    pub fn agent_server_store(&self) -> &Entity<AgentServerStore> {
        &self.agent_server_store
    }

    #[cfg(feature = "test-support")]
    pub fn git_scans_complete(&self, cx: &Context<Self>) -> Task<()> {
        use futures::future::join_all;
        cx.spawn(async move |this, cx| {
            let scans_complete = this
                .read_with(cx, |this, cx| {
                    this.worktrees(cx)
                        .filter_map(|worktree| Some(worktree.read(cx).as_local()?.scan_complete()))
                        .collect::<Vec<_>>()
                })
                .unwrap();
            join_all(scans_complete).await;
            let barriers = this
                .update(cx, |this, cx| {
                    let repos = this.repositories(cx).values().cloned().collect::<Vec<_>>();
                    repos
                        .into_iter()
                        .map(|repo| repo.update(cx, |repo, _| repo.barrier()))
                        .collect::<Vec<_>>()
                })
                .unwrap();
            join_all(barriers).await;
        })
    }

    pub fn active_repository(&self, cx: &App) -> Option<Entity<Repository>> {
        self.git_store.read(cx).active_repository()
    }

    pub fn repositories<'a>(&self, cx: &'a App) -> &'a HashMap<RepositoryId, Entity<Repository>> {
        self.git_store.read(cx).repositories()
    }

    pub fn status_for_buffer_id(&self, buffer_id: BufferId, cx: &App) -> Option<FileStatus> {
        self.git_store.read(cx).status_for_buffer_id(buffer_id, cx)
    }

    pub fn set_agent_location(
        &mut self,
        new_location: Option<AgentLocation>,
        cx: &mut Context<Self>,
    ) {
        if let Some(old_location) = self.agent_location.as_ref() {
            old_location
                .buffer
                .update(cx, |buffer, cx| buffer.remove_agent_selections(cx))
                .ok();
        }

        if let Some(location) = new_location.as_ref() {
            location
                .buffer
                .update(cx, |buffer, cx| {
                    buffer.set_agent_selections(
                        Arc::from([language::Selection {
                            id: 0,
                            start: location.position,
                            end: location.position,
                            reversed: false,
                            goal: language::SelectionGoal::None,
                        }]),
                        false,
                        CursorShape::Hollow,
                        cx,
                    )
                })
                .ok();
        }

        self.agent_location = new_location;
        cx.emit(Event::AgentLocationChanged);
    }

    pub fn agent_location(&self) -> Option<AgentLocation> {
        self.agent_location.clone()
    }

    pub fn path_style(&self, cx: &App) -> PathStyle {
        self.worktree_store.read(cx).path_style()
    }

    pub fn contains_local_settings_file(
        &self,
        worktree_id: WorktreeId,
        rel_path: &RelPath,
        cx: &App,
    ) -> bool {
        self.worktree_for_id(worktree_id, cx)
            .map_or(false, |worktree| {
                worktree.read(cx).entry_for_path(rel_path).is_some()
            })
    }

    pub fn worktree_paths(&self, cx: &App) -> WorktreePaths {
        self.worktree_store.read(cx).paths(cx)
    }

    pub fn project_group_key(&self, cx: &App) -> ProjectGroupKey {
        ProjectGroupKey::from_project(self, cx)
    }
}
