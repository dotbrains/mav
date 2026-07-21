use super::*;

impl LspStore {
    pub(super) async fn refresh_workspace_configurations(
        lsp_store: &WeakEntity<Self>,
        cx: &mut AsyncApp,
    ) {
        maybe!(async move {
            let mut refreshed_servers = HashSet::default();
            let servers = lsp_store
                .update(cx, |lsp_store, cx| {
                    let local = lsp_store.as_local()?;

                    let servers = local
                        .language_server_ids
                        .iter()
                        .filter_map(|(seed, state)| {
                            let worktree = lsp_store
                                .worktree_store
                                .read(cx)
                                .worktree_for_id(seed.worktree_id, cx);
                            let delegate: Arc<dyn LspAdapterDelegate> =
                                worktree.map(|worktree| {
                                    LocalLspAdapterDelegate::new(
                                        local.languages.clone(),
                                        &local.environment,
                                        cx.weak_entity(),
                                        &worktree,
                                        local.http_client.clone(),
                                        local.fs.clone(),
                                        cx,
                                    )
                                })?;
                            let server_id = state.id;

                            let states = local.language_servers.get(&server_id)?;

                            match states {
                                LanguageServerState::Starting { .. } => None,
                                LanguageServerState::Running {
                                    adapter, server, ..
                                } => {
                                    let adapter = adapter.clone();
                                    let server = server.clone();
                                    refreshed_servers.insert(server.name());
                                    let toolchain = seed.toolchain.clone();
                                    Some(cx.spawn(async move |_, cx| {
                                        let settings =
                                            LocalLspStore::workspace_configuration_for_adapter(
                                                adapter.adapter.clone(),
                                                &delegate,
                                                toolchain,
                                                None,
                                                cx,
                                            )
                                            .await
                                            .ok()?;
                                        server
                                            .notify::<lsp::notification::DidChangeConfiguration>(
                                                lsp::DidChangeConfigurationParams { settings },
                                            )
                                            .ok()?;
                                        Some(())
                                    }))
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    Some(servers)
                })
                .ok()
                .flatten()?;

            log::debug!("Refreshing workspace configurations for servers {refreshed_servers:?}");
            // TODO this asynchronous job runs concurrently with extension (de)registration and may take enough time for a certain extension
            // to stop and unregister its language server wrapper.
            // This is racy : an extension might have already removed all `local.language_servers` state, but here we `.clone()` and hold onto it anyway.
            // This now causes errors in the logs, we should find a way to remove such servers from the processing everywhere.
            let _: Vec<Option<()>> = join_all(servers).await;

            Some(())
        })
        .await;
    }

    pub(super) fn maintain_workspace_config(
        mut external_refresh_requests: watch::Receiver<()>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        // Multiple things can happen when a workspace environment (selected toolchain + settings) change:
        // - We might shut down a language server if it's no longer enabled for a given language (and there are no buffers using it otherwise).
        // - We might also shut it down when the workspace configuration of all of the users of a given language server converges onto that of the other.
        // - In the same vein, we might also decide to start a new language server if the workspace configuration *diverges* from the other.
        // - In the easiest case (where we're not wrangling the lifetime of a language server anyhow), if none of the roots of a single language server diverge in their configuration,
        // but it is still different to what we had before, we're gonna send out a workspace configuration update.
        //
        // Settings-store changes reach this loop via `on_settings_changed` -> `request_workspace_config_refresh`,
        // which writes to `external_refresh_requests`. Observing `SettingsStore` here as well would cause every
        // settings change to drive the loop twice and emit duplicate `workspace/didChangeConfiguration` notifications.
        cx.spawn(async move |this, cx| {
            while let Some(()) = external_refresh_requests.next().await {
                this.update(cx, |this, cx| {
                    this.refresh_server_tree(cx);
                })
                .ok();

                Self::refresh_workspace_configurations(&this, cx).await;
            }

            anyhow::Ok(())
        })
    }
}
