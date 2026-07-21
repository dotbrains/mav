use super::*;

impl LocalLspStore {
    /// Returns the running language server for the given ID. Note if the language server is starting, it will not be returned.
    pub fn running_language_server_for_id(
        &self,
        id: LanguageServerId,
    ) -> Option<&Arc<LanguageServer>> {
        let language_server_state = self.language_servers.get(&id)?;

        match language_server_state {
            LanguageServerState::Running { server, .. } => Some(server),
            LanguageServerState::Starting { .. } => None,
        }
    }

    pub(super) fn get_or_insert_language_server(
        &mut self,
        worktree_handle: &Entity<Worktree>,
        delegate: Arc<LocalLspAdapterDelegate>,
        disposition: &Arc<LaunchDisposition>,
        language_name: &LanguageName,
        cx: &mut App,
    ) -> LanguageServerId {
        let key = LanguageServerSeed {
            worktree_id: worktree_handle.read(cx).id(),
            name: disposition.server_name.clone(),
            settings: LanguageServerSeedSettings {
                binary: disposition.settings.binary.clone(),
                initialization_options: disposition.settings.initialization_options.clone(),
            },
            toolchain: disposition.toolchain.clone(),
        };
        if let Some(state) = self.language_server_ids.get_mut(&key) {
            state.project_roots.insert(disposition.path.path.clone());
            state.id
        } else {
            let adapter = self
                .languages
                .lsp_adapters(language_name)
                .into_iter()
                .find(|adapter| adapter.name() == disposition.server_name)
                .expect("To find LSP adapter");
            let new_language_server_id = self.start_language_server(
                worktree_handle,
                delegate,
                adapter,
                disposition.settings.clone(),
                key.clone(),
                language_name.clone(),
                cx,
            );
            if let Some(state) = self.language_server_ids.get_mut(&key) {
                state.project_roots.insert(disposition.path.path.clone());
            } else {
                debug_assert!(
                    false,
                    "Expected `start_language_server` to ensure that `key` exists in a map"
                );
            }
            new_language_server_id
        }
    }
}

impl LspStore {
    pub fn init(client: &AnyProtoClient) {
        register_lsp_handlers(client);
    }

    pub fn as_remote(&self) -> Option<&RemoteLspStore> {
        match &self.mode {
            LspStoreMode::Remote(remote_lsp_store) => Some(remote_lsp_store),
            _ => None,
        }
    }

    pub fn as_local(&self) -> Option<&LocalLspStore> {
        match &self.mode {
            LspStoreMode::Local(local_lsp_store) => Some(local_lsp_store),
            _ => None,
        }
    }

    pub fn as_local_mut(&mut self) -> Option<&mut LocalLspStore> {
        match &mut self.mode {
            LspStoreMode::Local(local_lsp_store) => Some(local_lsp_store),
            _ => None,
        }
    }

    pub fn upstream_client(&self) -> Option<(AnyProtoClient, u64)> {
        match &self.mode {
            LspStoreMode::Remote(RemoteLspStore {
                upstream_client: Some(upstream_client),
                upstream_project_id,
                ..
            }) => Some((upstream_client.clone(), *upstream_project_id)),

            LspStoreMode::Remote(RemoteLspStore {
                upstream_client: None,
                ..
            }) => None,
            LspStoreMode::Local(_) => None,
        }
    }

    pub fn new_local(
        buffer_store: Entity<BufferStore>,
        worktree_store: Entity<WorktreeStore>,
        prettier_store: Entity<PrettierStore>,
        toolchain_store: Entity<LocalToolchainStore>,
        environment: Entity<ProjectEnvironment>,
        manifest_tree: Entity<ManifestTree>,
        languages: Arc<LanguageRegistry>,
        http_client: Arc<dyn HttpClient>,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) -> Self {
        let yarn = YarnPathStore::new(fs.clone(), cx);
        cx.subscribe(&buffer_store, Self::on_buffer_store_event)
            .detach();
        cx.subscribe(&worktree_store, Self::on_worktree_store_event)
            .detach();
        cx.subscribe(&prettier_store, Self::on_prettier_store_event)
            .detach();
        cx.subscribe(&toolchain_store, Self::on_toolchain_store_event)
            .detach();
        cx.observe_global::<SettingsStore>(Self::on_settings_changed)
            .detach();
        subscribe_to_binary_statuses(&languages, cx).detach();

        let _maintain_workspace_config = {
            let (sender, receiver) = watch::channel();
            (Self::maintain_workspace_config(receiver, cx), sender)
        };

        Self {
            mode: LspStoreMode::Local(LocalLspStore {
                weak: cx.weak_entity(),
                worktree_store: worktree_store.clone(),

                supplementary_language_servers: Default::default(),
                languages: languages.clone(),
                language_server_ids: Default::default(),
                language_servers: Default::default(),
                last_workspace_edits_by_language_server: Default::default(),
                language_server_watched_paths: Default::default(),
                language_server_paths_watched_for_rename: Default::default(),
                language_server_dynamic_registrations: Default::default(),
                buffers_being_formatted: Default::default(),
                buffers_to_refresh_hash_set: HashSet::default(),
                buffers_to_refresh_queue: VecDeque::new(),
                _background_diagnostics_worker: Task::ready(()).shared(),
                buffer_snapshots: Default::default(),
                prettier_store,
                environment,
                http_client,
                fs,
                yarn,
                next_diagnostic_group_id: Default::default(),
                diagnostics: Default::default(),
                _subscription: cx.on_app_quit(|this, _| {
                    this.as_local_mut()
                        .unwrap()
                        .shutdown_language_servers_on_quit()
                }),
                lsp_tree: LanguageServerTree::new(
                    manifest_tree,
                    languages.clone(),
                    toolchain_store.clone(),
                ),
                toolchain_store,
                registered_buffers: HashMap::default(),
                buffers_opened_in_servers: HashMap::default(),
                buffer_pull_diagnostics_result_ids: HashMap::default(),
                workspace_pull_diagnostics_result_ids: HashMap::default(),
                restricted_worktrees_tasks: HashMap::default(),
                all_language_servers_stopped: false,
                stopped_language_servers: HashSet::default(),
                watched_manifest_filenames: ManifestProvidersStore::global(cx)
                    .manifest_file_names(),
            }),
            last_formatting_failure: None,
            downstream_client: None,
            buffer_store,
            worktree_store,
            languages: languages.clone(),
            language_server_statuses: Default::default(),
            nonce: StdRng::from_os_rng().random(),
            diagnostic_summaries: HashMap::default(),
            lsp_server_capabilities: HashMap::default(),
            semantic_token_config: SemanticTokenConfig::new(cx),
            lsp_data: HashMap::default(),
            buffer_reload_tasks: HashMap::default(),
            next_hint_id: Arc::default(),
            active_entry: None,
            _maintain_workspace_config,
            _maintain_buffer_languages: Self::maintain_buffer_languages(languages, cx),
        }
    }

    pub(super) fn send_lsp_proto_request<R: LspCommand>(
        &self,
        buffer: Entity<Buffer>,
        client: AnyProtoClient,
        upstream_project_id: u64,
        request: R,
        cx: &mut Context<LspStore>,
    ) -> Task<anyhow::Result<<R as LspCommand>::Response>> {
        if !self.is_capable_for_proto_request(&buffer, &request, cx) {
            return Task::ready(Ok(R::Response::default()));
        }
        let message = request.to_proto(upstream_project_id, buffer.read(cx));
        cx.spawn(async move |this, cx| {
            let response = client.request(message).await?;
            let this = this.upgrade().context("project dropped")?;
            request
                .response_from_proto(response, this, buffer, cx.clone())
                .await
        })
    }

    pub(crate) fn new_remote(
        buffer_store: Entity<BufferStore>,
        worktree_store: Entity<WorktreeStore>,
        languages: Arc<LanguageRegistry>,
        upstream_client: AnyProtoClient,
        project_id: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&buffer_store, Self::on_buffer_store_event)
            .detach();
        cx.subscribe(&worktree_store, Self::on_worktree_store_event)
            .detach();
        subscribe_to_binary_statuses(&languages, cx).detach();
        let _maintain_workspace_config = {
            let (sender, receiver) = watch::channel();
            (Self::maintain_workspace_config(receiver, cx), sender)
        };
        Self {
            mode: LspStoreMode::Remote(RemoteLspStore {
                upstream_client: Some(upstream_client),
                upstream_project_id: project_id,
            }),
            downstream_client: None,
            last_formatting_failure: None,
            buffer_store,
            worktree_store,
            languages: languages.clone(),
            language_server_statuses: Default::default(),
            nonce: StdRng::from_os_rng().random(),
            diagnostic_summaries: HashMap::default(),
            lsp_server_capabilities: HashMap::default(),
            semantic_token_config: SemanticTokenConfig::new(cx),
            next_hint_id: Arc::default(),
            lsp_data: HashMap::default(),
            buffer_reload_tasks: HashMap::default(),
            active_entry: None,

            _maintain_workspace_config,
            _maintain_buffer_languages: Self::maintain_buffer_languages(languages.clone(), cx),
        }
    }
}
