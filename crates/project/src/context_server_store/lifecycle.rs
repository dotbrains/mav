use super::*;

impl ContextServerStore {
    pub fn start_server(&mut self, server: Arc<ContextServer>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let this = this.upgrade().context("Context server store dropped")?;
            let id = server.id();
            let settings = this
                .update(cx, |this, _| {
                    this.context_server_settings.get(&id.0).cloned()
                })
                .context("Failed to get context server settings")?;

            if !settings.enabled() {
                return anyhow::Ok(());
            }

            let (registry, worktree_store) = this.update(cx, |this, _| {
                (this.registry.clone(), this.worktree_store.clone())
            });
            let configuration = ContextServerConfiguration::from_settings(
                settings,
                id.clone(),
                registry,
                worktree_store,
                cx,
            )
            .await
            .context("Failed to create context server configuration")?;

            this.update(cx, |this, cx| {
                this.run_server(server, Arc::new(configuration), cx)
            });
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn stop_server(&mut self, id: &ContextServerId, cx: &mut Context<Self>) -> Result<()> {
        if matches!(
            self.servers.get(id),
            Some(ContextServerState::Stopped { .. })
        ) {
            return Ok(());
        }

        let state = self
            .servers
            .remove(id)
            .context("Context server not found")?;

        let server = state.server();
        let configuration = state.configuration();
        let result = server.stop();
        drop(state);

        self.update_server_state(
            id.clone(),
            ContextServerState::Stopped {
                configuration,
                server,
            },
            cx,
        );

        result
    }

    pub(super) fn run_server(
        &mut self,
        server: Arc<ContextServer>,
        configuration: Arc<ContextServerConfiguration>,
        cx: &mut Context<Self>,
    ) {
        let id = server.id();
        if matches!(
            self.servers.get(&id),
            Some(
                ContextServerState::Starting { .. }
                    | ContextServerState::Running { .. }
                    | ContextServerState::Authenticating { .. },
            )
        ) {
            self.stop_server(&id, cx).log_err();
        }
        let task = cx.spawn({
            let id = server.id();
            let server = server.clone();
            let configuration = configuration.clone();

            async move |this, cx| {
                let new_state = match server.clone().start(cx).await {
                    Ok(_) => {
                        debug_assert!(server.client().is_some());
                        ContextServerState::Running {
                            server,
                            configuration,
                        }
                    }
                    Err(err) => {
                        start_failure::resolve_start_failure(&id, err, server, configuration, cx)
                            .await
                    }
                };
                this.update(cx, |this, cx| {
                    this.update_server_state(id.clone(), new_state, cx)
                })
                .log_err();
            }
        });

        self.update_server_state(
            id.clone(),
            ContextServerState::Starting {
                configuration,
                _task: task,
                server,
            },
            cx,
        );
    }

    pub(super) fn remove_server(
        &mut self,
        id: &ContextServerId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let state = self
            .servers
            .remove(id)
            .context("Context server not found")?;

        if let ContextServerConfiguration::Http { url, .. } = state.configuration().as_ref() {
            let server_url = url.clone();
            let id = id.clone();
            cx.spawn(async move |_this, cx| {
                let credentials_provider = cx.update(|cx| mav_credentials_provider::global(cx));
                if let Err(err) = Self::clear_session(&credentials_provider, &server_url, &cx).await
                {
                    log::warn!("{} failed to clear OAuth session on removal: {}", id, err);
                }
            })
            .detach();
        }

        drop(state);
        cx.emit(ServerStatusChangedEvent {
            server_id: id.clone(),
            status: ContextServerStatus::Stopped,
        });
        Ok(())
    }
}
