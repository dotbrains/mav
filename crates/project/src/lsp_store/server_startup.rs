use super::*;

impl LocalLspStore {
    pub(super) fn start_language_server(
        &mut self,
        worktree_handle: &Entity<Worktree>,
        delegate: Arc<LocalLspAdapterDelegate>,
        adapter: Arc<CachedLspAdapter>,
        settings: Arc<LspSettings>,
        key: LanguageServerSeed,
        language_name: LanguageName,
        cx: &mut App,
    ) -> LanguageServerId {
        let worktree = worktree_handle.read(cx);

        let worktree_id = worktree.id();
        let worktree_abs_path = worktree.abs_path();
        let toolchain = key.toolchain.clone();
        let override_options = settings.initialization_options.clone();

        let stderr_capture = Arc::new(Mutex::new(Some(String::new())));

        let server_id = self.languages.next_language_server_id();
        log::trace!(
            "attempting to start language server {:?}, path: {worktree_abs_path:?}, id: {server_id}",
            adapter.name.0
        );

        let wait_until_worktree_trust =
            TrustedWorktrees::try_get_global(cx).and_then(|trusted_worktrees| {
                let can_trust = trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                    trusted_worktrees.can_trust(&self.worktree_store, worktree_id, cx)
                });
                if can_trust {
                    self.restricted_worktrees_tasks.remove(&worktree_id);
                    None
                } else {
                    match self.restricted_worktrees_tasks.entry(worktree_id) {
                        hash_map::Entry::Occupied(o) => Some(o.get().1.clone()),
                        hash_map::Entry::Vacant(v) => {
                            let (mut tx, rx) = watch::channel::<bool>();
                            let lsp_store = self.weak.clone();
                            let subscription = cx.subscribe(&trusted_worktrees, move |_, e, cx| {
                                if let TrustedWorktreesEvent::Trusted(_, trusted_paths) = e {
                                    if trusted_paths.contains(&PathTrust::Worktree(worktree_id)) {
                                        tx.blocking_send(true).ok();
                                        lsp_store
                                            .update(cx, |lsp_store, _| {
                                                if let Some(local_lsp_store) =
                                                    lsp_store.as_local_mut()
                                                {
                                                    local_lsp_store
                                                        .restricted_worktrees_tasks
                                                        .remove(&worktree_id);
                                                }
                                            })
                                            .ok();
                                    }
                                }
                            });
                            v.insert((subscription, rx.clone()));
                            Some(rx)
                        }
                    }
                }
            });
        let update_binary_status = wait_until_worktree_trust.is_none();

        let binary = self.get_language_server_binary(
            worktree_abs_path.clone(),
            adapter.clone(),
            settings,
            toolchain.clone(),
            delegate.clone(),
            true,
            wait_until_worktree_trust,
            cx,
        );
        let pending_workspace_folders = Arc::<Mutex<BTreeSet<Uri>>>::default();

        let pending_server = cx.spawn({
            let adapter = adapter.clone();
            let server_name = adapter.name.clone();
            let stderr_capture = stderr_capture.clone();
            #[cfg(any(test, feature = "test-support"))]
            let lsp_store = self.weak.clone();
            let pending_workspace_folders = pending_workspace_folders.clone();
            async move |cx| {
                let binary = binary.await?;
                #[cfg(any(test, feature = "test-support"))]
                if let Some(server) = lsp_store
                    .update(&mut cx.clone(), |this, cx| {
                        this.languages.create_fake_language_server(
                            server_id,
                            &server_name,
                            binary.clone(),
                            &mut cx.to_async(),
                        )
                    })
                    .ok()
                    .flatten()
                {
                    return Ok(server);
                }

                let code_action_kinds = adapter.code_action_kinds();
                lsp::LanguageServer::new(
                    stderr_capture,
                    server_id,
                    server_name,
                    binary,
                    &worktree_abs_path,
                    code_action_kinds,
                    Some(pending_workspace_folders),
                    cx,
                )
            }
        });

        let startup = {
            let server_name = adapter.name.0.clone();
            let delegate = delegate as Arc<dyn LspAdapterDelegate>;
            let key = key.clone();
            let adapter = adapter.clone();
            let lsp_store = self.weak.clone();
            let pending_workspace_folders = pending_workspace_folders.clone();
            let pull_diagnostics = ProjectSettings::get_global(cx)
                .diagnostics
                .lsp_pull_diagnostics
                .enabled;
            let settings_location = SettingsLocation {
                worktree_id,
                path: RelPath::empty(),
            };
            let augments_syntax_tokens = AllLanguageSettings::get(Some(settings_location), cx)
                .language(Some(settings_location), Some(&language_name), cx)
                .semantic_tokens
                .use_tree_sitter();
            cx.spawn(async move |cx| {
                let result = async {
                    let language_server = pending_server.await?;

                    let workspace_config = Self::workspace_configuration_for_adapter(
                        adapter.adapter.clone(),
                        &delegate,
                        toolchain,
                        None,
                        cx,
                    )
                    .await?;

                    let mut initialization_options = Self::initialization_options_for_adapter(
                        adapter.adapter.clone(),
                        &delegate,
                        cx,
                    )
                    .await?;

                    match (&mut initialization_options, override_options) {
                        (Some(initialization_options), Some(override_options)) => {
                            merge_json_value_into(override_options, initialization_options);
                        }
                        (None, override_options) => initialization_options = override_options,
                        _ => {}
                    }

                    let initialization_params = cx.update(|cx| {
                        let mut params = language_server.default_initialize_params(
                            pull_diagnostics,
                            augments_syntax_tokens,
                            cx,
                        );
                        params.initialization_options = initialization_options;
                        adapter.adapter.prepare_initialize_params(params, cx)
                    })?;

                    Self::setup_lsp_messages(
                        lsp_store.clone(),
                        &language_server,
                        delegate.clone(),
                        adapter.clone(),
                    );

                    let did_change_configuration_params = lsp::DidChangeConfigurationParams {
                        settings: workspace_config,
                    };
                    let language_server = cx
                        .update(|cx| {
                            let request_timeout = ProjectSettings::get_global(cx)
                                .global_lsp_settings
                                .get_request_timeout();

                            language_server.initialize(
                                initialization_params,
                                Arc::new(did_change_configuration_params.clone()),
                                request_timeout,
                                cx,
                            )
                        })
                        .await
                        .inspect_err(|_| {
                            if let Some(lsp_store) = lsp_store.upgrade() {
                                lsp_store.update(cx, |lsp_store, cx| {
                                    lsp_store.cleanup_lsp_data(server_id);
                                    cx.emit(LspStoreEvent::LanguageServerRemoved(server_id))
                                });
                            }
                        })?;

                    language_server.notify::<lsp::notification::DidChangeConfiguration>(
                        did_change_configuration_params,
                    )?;

                    anyhow::Ok(language_server)
                }
                .await;

                match result {
                    Ok(server) => {
                        lsp_store
                            .update(cx, |lsp_store, cx| {
                                lsp_store.insert_newly_running_language_server(
                                    adapter,
                                    server.clone(),
                                    server_id,
                                    key,
                                    language_name,
                                    pending_workspace_folders,
                                    cx,
                                );
                            })
                            .ok();
                        stderr_capture.lock().take();
                        Some(server)
                    }

                    Err(err) => {
                        let log = stderr_capture.lock().take().unwrap_or_default();
                        delegate.update_status(
                            adapter.name(),
                            BinaryStatus::Failed {
                                error: if log.is_empty() {
                                    format!("{err:#}")
                                } else {
                                    format!("{err:#}\n-- stderr --\n{log}")
                                },
                            },
                        );
                        log::error!(
                            "Failed to start language server {server_name:?}: {}",
                            redact_command(&format!("{err:?}"))
                        );
                        if !log.is_empty() {
                            log::error!("server stderr: {}", redact_command(&log));
                        }
                        None
                    }
                }
            })
        };
        let state = LanguageServerState::Starting {
            startup,
            pending_workspace_folders,
        };

        if update_binary_status {
            self.languages
                .update_lsp_binary_status(adapter.name(), BinaryStatus::Starting);
        }

        self.language_servers.insert(server_id, state);
        self.language_server_ids
            .entry(key)
            .or_insert(UnifiedLanguageServer {
                id: server_id,
                project_roots: Default::default(),
            });
        server_id
    }
}
