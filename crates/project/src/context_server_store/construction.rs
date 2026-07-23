use super::*;

impl ContextServerStore {
    pub fn local(
        worktree_store: Entity<WorktreeStore>,
        weak_project: Option<WeakEntity<Project>>,
        headless: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(
            !headless,
            None,
            ContextServerDescriptorRegistry::default_global(cx),
            worktree_store,
            weak_project,
            ContextServerStoreState::Local {
                downstream_client: None,
                is_headless: headless,
            },
            cx,
        )
    }

    pub fn remote(
        project_id: u64,
        upstream_client: Entity<RemoteClient>,
        worktree_store: Entity<WorktreeStore>,
        weak_project: Option<WeakEntity<Project>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(
            true,
            None,
            ContextServerDescriptorRegistry::default_global(cx),
            worktree_store,
            weak_project,
            ContextServerStoreState::Remote {
                project_id,
                upstream_client,
            },
            cx,
        )
    }

    pub fn init_headless(session: &AnyProtoClient) {
        session.add_entity_request_handler(Self::handle_get_context_server_command);
    }

    pub fn shared(&mut self, project_id: u64, client: AnyProtoClient) {
        if let ContextServerStoreState::Local {
            downstream_client, ..
        } = &mut self.state
        {
            *downstream_client = Some((project_id, client));
        }
    }

    pub fn is_remote_project(&self) -> bool {
        matches!(self.state, ContextServerStoreState::Remote { .. })
    }

    /// Returns all configured context server ids, excluding the ones that are disabled
    pub fn configured_server_ids(&self) -> Vec<ContextServerId> {
        self.context_server_settings
            .iter()
            .filter(|(_, settings)| settings.enabled())
            .map(|(id, _)| ContextServerId(id.clone()))
            .collect()
    }

    #[cfg(feature = "test-support")]
    pub fn test(
        registry: Entity<ContextServerDescriptorRegistry>,
        worktree_store: Entity<WorktreeStore>,
        weak_project: Option<WeakEntity<Project>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(
            false,
            None,
            registry,
            worktree_store,
            weak_project,
            ContextServerStoreState::Local {
                downstream_client: None,
                is_headless: false,
            },
            cx,
        )
    }

    #[cfg(feature = "test-support")]
    pub fn test_maintain_server_loop(
        context_server_factory: Option<ContextServerFactory>,
        registry: Entity<ContextServerDescriptorRegistry>,
        worktree_store: Entity<WorktreeStore>,
        weak_project: Option<WeakEntity<Project>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(
            true,
            context_server_factory,
            registry,
            worktree_store,
            weak_project,
            ContextServerStoreState::Local {
                downstream_client: None,
                is_headless: false,
            },
            cx,
        )
    }

    #[cfg(feature = "test-support")]
    pub fn set_context_server_factory(&mut self, factory: ContextServerFactory) {
        self.context_server_factory = Some(factory);
    }

    #[cfg(feature = "test-support")]
    pub fn registry(&self) -> &Entity<ContextServerDescriptorRegistry> {
        &self.registry
    }

    #[cfg(feature = "test-support")]
    pub fn test_start_server(&mut self, server: Arc<ContextServer>, cx: &mut Context<Self>) {
        let configuration = Arc::new(ContextServerConfiguration::Custom {
            command: ContextServerCommand {
                path: "test".into(),
                args: vec![],
                env: None,
                timeout: None,
            },
            remote: false,
        });
        self.run_server(server, configuration, cx);
    }

    fn new_internal(
        maintain_server_loop: bool,
        context_server_factory: Option<ContextServerFactory>,
        registry: Entity<ContextServerDescriptorRegistry>,
        worktree_store: Entity<WorktreeStore>,
        weak_project: Option<WeakEntity<Project>>,
        state: ContextServerStoreState,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = vec![cx.observe_global::<SettingsStore>(move |this, cx| {
            let ai_disabled = DisableAiSettings::get_global(cx).disable_ai;
            let ai_was_disabled = this.ai_disabled;
            this.ai_disabled = ai_disabled;

            let settings =
                &Self::resolve_project_settings(&this.worktree_store, cx).context_servers;
            let settings_changed = &this.context_server_settings != settings;

            if settings_changed {
                this.context_server_settings = settings.clone();
            }

            // When AI is disabled, stop all running servers
            if ai_disabled {
                let server_ids: Vec<_> = this.servers.keys().cloned().collect();
                for id in server_ids {
                    this.stop_server(&id, cx).log_err();
                }
                return;
            }

            // Trigger updates if AI was re-enabled or settings changed
            if maintain_server_loop && (ai_was_disabled || settings_changed) {
                this.available_context_servers_changed(cx);
            }
        })];

        if maintain_server_loop {
            subscriptions.push(cx.observe(&registry, |this, _registry, cx| {
                if !DisableAiSettings::get_global(cx).disable_ai {
                    this.available_context_servers_changed(cx);
                }
            }));
            subscriptions.push(cx.subscribe(&worktree_store, |this, _store, event, cx| {
                if matches!(
                    event,
                    WorktreeStoreEvent::WorktreeAdded(_)
                        | WorktreeStoreEvent::WorktreeRemoved(_, _)
                ) && !DisableAiSettings::get_global(cx).disable_ai
                {
                    this.available_context_servers_changed(cx);
                }
            }));
        }

        let ai_disabled = DisableAiSettings::get_global(cx).disable_ai;
        let mut this = Self {
            state,
            _subscriptions: subscriptions,
            context_server_settings: Self::resolve_project_settings(&worktree_store, cx)
                .context_servers
                .clone(),
            worktree_store,
            project: weak_project,
            registry,
            needs_server_update: false,
            ai_disabled,
            servers: HashMap::default(),
            server_ids: Default::default(),
            update_servers_task: None,
            context_server_factory,
        };
        if maintain_server_loop && !DisableAiSettings::get_global(cx).disable_ai {
            this.available_context_servers_changed(cx);
        }
        this
    }

    pub fn get_server(&self, id: &ContextServerId) -> Option<Arc<ContextServer>> {
        self.servers.get(id).map(|state| state.server())
    }

    pub fn get_running_server(&self, id: &ContextServerId) -> Option<Arc<ContextServer>> {
        if let Some(ContextServerState::Running { server, .. }) = self.servers.get(id) {
            Some(server.clone())
        } else {
            None
        }
    }

    pub fn status_for_server(&self, id: &ContextServerId) -> Option<ContextServerStatus> {
        self.servers.get(id).map(ContextServerStatus::from_state)
    }

    pub fn configuration_for_server(
        &self,
        id: &ContextServerId,
    ) -> Option<Arc<ContextServerConfiguration>> {
        self.servers.get(id).map(|state| state.configuration())
    }

    /// Returns the configured settings for a server, if it is present in the user
    /// or project settings. This is available regardless of whether the server is
    /// currently running, unlike [`Self::configuration_for_server`].
    pub fn settings_for_server(&self, id: &ContextServerId) -> Option<&ContextServerSettings> {
        self.context_server_settings.get(&id.0)
    }

    /// Returns whether a server is provided by an extension (as opposed to a
    /// custom Stdio/HTTP server configured directly in settings).
    ///
    /// This is derived from the configured settings rather than the runtime
    /// configuration, so it stays correct even when a custom server is disabled
    /// or has not been started yet (in which case it has no runtime state).
    pub fn is_extension_provided(&self, id: &ContextServerId, cx: &App) -> bool {
        match self.context_server_settings.get(&id.0) {
            Some(ContextServerSettings::Stdio { .. } | ContextServerSettings::Http { .. }) => false,
            Some(ContextServerSettings::Extension { .. }) => true,
            // No custom settings entry: the server can only originate from an
            // extension descriptor in the registry.
            None => self
                .registry
                .read(cx)
                .context_server_descriptor(&id.0)
                .is_some(),
        }
    }

    /// Returns a sorted slice of available unique context server IDs. Within the
    /// slice, context servers which have `mcp-server-` as a prefix in their ID will
    /// appear after servers that do not have this prefix in their ID.
    pub fn server_ids(&self) -> &[ContextServerId] {
        self.server_ids.as_slice()
    }

    pub(super) fn populate_server_ids(&mut self, cx: &App) {
        self.server_ids = self
            .servers
            .keys()
            .cloned()
            .chain(
                self.registry
                    .read(cx)
                    .context_server_descriptors()
                    .into_iter()
                    .map(|(id, _)| ContextServerId(id)),
            )
            .chain(
                self.context_server_settings
                    .keys()
                    .map(|id| ContextServerId(id.clone())),
            )
            .unique()
            .sorted_unstable_by(
                // Sort context servers: ones without mcp-server- prefix first, then prefixed ones
                |a, b| {
                    const MCP_PREFIX: &str = "mcp-server-";
                    match (a.0.strip_prefix(MCP_PREFIX), b.0.strip_prefix(MCP_PREFIX)) {
                        // If one has mcp-server- prefix and other doesn't, non-mcp comes first
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        // If both have same prefix status, sort by appropriate key
                        (Some(a), Some(b)) => a.cmp(b),
                        (None, None) => a.0.cmp(&b.0),
                    }
                },
            )
            .collect();
    }

    pub fn running_servers(&self) -> Vec<Arc<ContextServer>> {
        self.servers
            .values()
            .filter_map(|state| {
                if let ContextServerState::Running { server, .. } = state {
                    Some(server.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}
