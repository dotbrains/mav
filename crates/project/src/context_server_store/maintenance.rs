use super::*;

impl ContextServerStore {
    pub(super) fn update_server_state(
        &mut self,
        id: ContextServerId,
        state: ContextServerState,
        cx: &mut Context<Self>,
    ) {
        let status = ContextServerStatus::from_state(&state);
        self.servers.insert(id.clone(), state);
        cx.emit(ServerStatusChangedEvent {
            server_id: id,
            status,
        });
    }

    pub(super) fn available_context_servers_changed(&mut self, cx: &mut Context<Self>) {
        if self.update_servers_task.is_some() {
            self.needs_server_update = true;
        } else {
            self.needs_server_update = false;
            self.update_servers_task = Some(cx.spawn(async move |this, cx| {
                if let Err(err) = Self::maintain_servers(this.clone(), cx).await {
                    log::error!("Error maintaining context servers: {}", err);
                }

                this.update(cx, |this, cx| {
                    this.populate_server_ids(cx);
                    cx.notify();
                    this.update_servers_task.take();
                    if this.needs_server_update {
                        this.available_context_servers_changed(cx);
                    }
                })?;

                Ok(())
            }));
        }
    }

    async fn maintain_servers(this: WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        // Don't start context servers if AI is disabled
        let ai_disabled = this.update(cx, |_, cx| DisableAiSettings::get_global(cx).disable_ai)?;
        if ai_disabled {
            // Stop all running servers when AI is disabled
            this.update(cx, |this, cx| {
                let server_ids: Vec<_> = this.servers.keys().cloned().collect();
                for id in server_ids {
                    let _ = this.stop_server(&id, cx);
                }
            })?;
            return Ok(());
        }

        let (mut configured_servers, registry, worktree_store) = this.update(cx, |this, _| {
            (
                this.context_server_settings.clone(),
                this.registry.clone(),
                this.worktree_store.clone(),
            )
        })?;

        for (id, _) in registry.read_with(cx, |registry, _| registry.context_server_descriptors()) {
            configured_servers
                .entry(id)
                .or_insert(ContextServerSettings::default_extension());
        }

        let (enabled_servers, disabled_servers): (HashMap<_, _>, HashMap<_, _>) =
            configured_servers
                .into_iter()
                .partition(|(_, settings)| settings.enabled());

        let configured_servers = join_all(enabled_servers.into_iter().map(|(id, settings)| {
            let id = ContextServerId(id);
            ContextServerConfiguration::from_settings(
                settings,
                id.clone(),
                registry.clone(),
                worktree_store.clone(),
                cx,
            )
            .map(move |config| (id, config))
        }))
        .await
        .into_iter()
        .filter_map(|(id, config)| config.map(|config| (id, config)))
        .collect::<HashMap<_, _>>();

        let mut servers_to_start = Vec::new();
        let mut servers_to_remove = HashSet::default();
        let mut servers_to_stop = HashSet::default();

        this.update(cx, |this, _cx| {
            for server_id in this.servers.keys() {
                // All servers that are not in desired_servers should be removed from the store.
                // This can happen if the user removed a server from the context server settings.
                if !configured_servers.contains_key(server_id) {
                    if disabled_servers.contains_key(&server_id.0) {
                        servers_to_stop.insert(server_id.clone());
                    } else {
                        servers_to_remove.insert(server_id.clone());
                    }
                }
            }

            for (id, config) in configured_servers {
                let state = this.servers.get(&id);
                let is_stopped = matches!(state, Some(ContextServerState::Stopped { .. }));
                let existing_config = state.as_ref().map(|state| state.configuration());
                if existing_config.as_deref() != Some(&config) || is_stopped {
                    let config = Arc::new(config);
                    servers_to_start.push((id.clone(), config));
                    if this.servers.contains_key(&id) {
                        servers_to_stop.insert(id);
                    }
                }
            }

            anyhow::Ok(())
        })??;

        this.update(cx, |this, inner_cx| {
            for id in servers_to_stop {
                this.stop_server(&id, inner_cx)?;
            }
            for id in servers_to_remove {
                this.remove_server(&id, inner_cx)?;
            }
            anyhow::Ok(())
        })??;

        for (id, config) in servers_to_start {
            match Self::create_context_server(this.clone(), id.clone(), config, cx).await {
                Ok((server, config)) => {
                    this.update(cx, |this, cx| {
                        this.run_server(server, config, cx);
                    })?;
                }
                Err(err) => {
                    log::error!("{id} context server failed to create: {err:#}");
                    this.update(cx, |_this, cx| {
                        cx.emit(ServerStatusChangedEvent {
                            server_id: id,
                            status: ContextServerStatus::Error(err.to_string().into()),
                        });
                        cx.notify();
                    })?;
                }
            }
        }

        Ok(())
    }
}
