use super::*;

impl LocalLspStore {
    pub(super) fn setup_lsp_messages(
        lsp_store: WeakEntity<LspStore>,
        language_server: &LanguageServer,
        delegate: Arc<dyn LspAdapterDelegate>,
        adapter: Arc<CachedLspAdapter>,
    ) {
        let name = language_server.name();
        let server_id = language_server.server_id();
        language_server
            .on_notification::<lsp::notification::PublishDiagnostics, _>({
                let adapter = adapter.clone();
                let this = lsp_store.clone();
                move |mut params, cx| {
                    let adapter = adapter.clone();
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            adapter.process_diagnostics(&mut params, server_id);

                            this.merge_lsp_diagnostics(
                                DiagnosticSourceKind::Pushed,
                                vec![DocumentDiagnosticsUpdate {
                                    server_id,
                                    diagnostics: params,
                                    result_id: None,
                                    disk_based_sources: Cow::Borrowed(
                                        &adapter.disk_based_diagnostic_sources,
                                    ),
                                    registration_id: None,
                                }],
                                |_, diagnostic, _cx| match diagnostic.source_kind {
                                    DiagnosticSourceKind::Other | DiagnosticSourceKind::Pushed => {
                                        adapter.retain_old_diagnostic(diagnostic)
                                    }
                                    DiagnosticSourceKind::Pulled => true,
                                },
                                cx,
                            )
                            .log_err();
                        });
                    }
                }
            })
            .detach();
        language_server
            .on_request::<lsp::request::WorkspaceConfiguration, _, _>({
                let adapter = adapter.adapter.clone();
                let delegate = delegate.clone();
                let this = lsp_store.clone();
                move |params, cx| {
                    let adapter = adapter.clone();
                    let delegate = delegate.clone();
                    let this = this.clone();
                    let mut cx = cx.clone();
                    async move {
                        let toolchain_for_id = this
                            .update(&mut cx, |this, _| {
                                this.as_local()?.language_server_ids.iter().find_map(
                                    |(seed, value)| {
                                        (value.id == server_id).then(|| seed.toolchain.clone())
                                    },
                                )
                            })?
                            .context("Expected the LSP store to be in a local mode")?;

                        let mut scope_uri_to_workspace_config = BTreeMap::new();
                        for item in &params.items {
                            let scope_uri = item.scope_uri.clone();
                            let std::collections::btree_map::Entry::Vacant(new_scope_uri) =
                                scope_uri_to_workspace_config.entry(scope_uri.clone())
                            else {
                                // We've already queried workspace configuration of this URI.
                                continue;
                            };
                            let workspace_config = Self::workspace_configuration_for_adapter(
                                adapter.clone(),
                                &delegate,
                                toolchain_for_id.clone(),
                                scope_uri,
                                &mut cx,
                            )
                            .await?;
                            new_scope_uri.insert(workspace_config);
                        }

                        Ok(params
                            .items
                            .into_iter()
                            .filter_map(|item| {
                                let workspace_config =
                                    scope_uri_to_workspace_config.get(&item.scope_uri)?;
                                if let Some(section) = &item.section {
                                    Some(
                                        workspace_config
                                            .get(section)
                                            .cloned()
                                            .unwrap_or(serde_json::Value::Null),
                                    )
                                } else {
                                    Some(workspace_config.clone())
                                }
                            })
                            .collect())
                    }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::WorkspaceFoldersRequest, _, _>({
                let this = lsp_store.clone();
                move |_, cx| {
                    let this = this.clone();
                    let cx = cx.clone();
                    async move {
                        let Some(server) =
                            this.read_with(&cx, |this, _| this.language_server_for_id(server_id))?
                        else {
                            return Ok(None);
                        };
                        let root = server.workspace_folders();
                        Ok(Some(
                            root.into_iter()
                                .map(|uri| WorkspaceFolder {
                                    uri,
                                    name: Default::default(),
                                })
                                .collect(),
                        ))
                    }
                }
            })
            .detach();
        // Even though we don't have handling for these requests, respond to them to
        // avoid stalling any language server like `gopls` which waits for a response
        // to these requests when initializing.
        language_server
            .on_request::<lsp::request::WorkDoneProgressCreate, _, _>({
                let this = lsp_store.clone();
                move |params, cx| {
                    let this = this.clone();
                    let mut cx = cx.clone();
                    async move {
                        this.update(&mut cx, |this, _| {
                            if let Some(status) = this.language_server_statuses.get_mut(&server_id)
                            {
                                status
                                    .progress_tokens
                                    .insert(ProgressToken::from_lsp(params.token));
                            }
                        })?;

                        Ok(())
                    }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::RegisterCapability, _, _>({
                let lsp_store = lsp_store.clone();
                move |params, cx| {
                    let lsp_store = lsp_store.clone();
                    let mut cx = cx.clone();
                    async move {
                        lsp_store
                            .update(&mut cx, |lsp_store, cx| {
                                if lsp_store.as_local().is_some() {
                                    match lsp_store
                                        .register_server_capabilities(server_id, params, cx)
                                    {
                                        Ok(()) => {}
                                        Err(e) => {
                                            log::error!(
                                                "Failed to register server capabilities: {e:#}"
                                            );
                                        }
                                    };
                                }
                            })
                            .ok();
                        Ok(())
                    }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::UnregisterCapability, _, _>({
                let lsp_store = lsp_store.clone();
                move |params, cx| {
                    let lsp_store = lsp_store.clone();
                    let mut cx = cx.clone();
                    async move {
                        lsp_store
                            .update(&mut cx, |lsp_store, cx| {
                                if lsp_store.as_local().is_some() {
                                    match lsp_store
                                        .unregister_server_capabilities(server_id, params, cx)
                                    {
                                        Ok(()) => {}
                                        Err(e) => {
                                            log::error!(
                                                "Failed to unregister server capabilities: {e:#}"
                                            );
                                        }
                                    }
                                }
                            })
                            .ok();
                        Ok(())
                    }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::ApplyWorkspaceEdit, _, _>({
                let this = lsp_store.clone();
                move |params, cx| {
                    let mut cx = cx.clone();
                    let this = this.clone();
                    async move {
                        LocalLspStore::on_lsp_workspace_edit(
                            this.clone(),
                            params,
                            server_id,
                            &mut cx,
                        )
                        .await
                    }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::InlayHintRefreshRequest, _, _>({
                let lsp_store = lsp_store.clone();
                let request_id = Arc::new(AtomicUsize::new(0));
                move |(), cx| {
                    let lsp_store = lsp_store.clone();
                    let request_id = request_id.clone();
                    let mut cx = cx.clone();
                    async move {
                        lsp_store
                            .update(&mut cx, |lsp_store, cx| {
                                let request_id =
                                    Some(request_id.fetch_add(1, atomic::Ordering::AcqRel));
                                cx.emit(LspStoreEvent::RefreshInlayHints {
                                    server_id,
                                    request_id,
                                });
                                lsp_store
                                    .downstream_client
                                    .as_ref()
                                    .map(|(client, project_id)| {
                                        client.send(proto::RefreshInlayHints {
                                            project_id: *project_id,
                                            server_id: server_id.to_proto(),
                                            request_id: request_id.map(|id| id as u64),
                                        })
                                    })
                            })?
                            .transpose()?;
                        Ok(())
                    }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::CodeLensRefresh, _, _>({
                let lsp_store = lsp_store.clone();
                move |(), cx| {
                    let result = lsp_store.update(cx, |lsp_store, cx| {
                        lsp_store.refresh_code_lens(cx);
                    });
                    async move { result }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::SemanticTokensRefresh, _, _>({
                let lsp_store = lsp_store.clone();
                let request_id = Arc::new(AtomicUsize::new(0));
                move |(), cx| {
                    let lsp_store = lsp_store.clone();
                    let request_id = request_id.clone();
                    let mut cx = cx.clone();
                    async move {
                        lsp_store.update(&mut cx, |lsp_store, cx| {
                            let request_id =
                                Some(request_id.fetch_add(1, atomic::Ordering::AcqRel));
                            lsp_store.refresh_semantic_tokens(server_id, request_id, cx);
                        })?;
                        Ok(())
                    }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::WorkspaceDiagnosticRefresh, _, _>({
                let this = lsp_store.clone();
                move |(), cx| {
                    let this = this.clone();
                    let mut cx = cx.clone();
                    async move {
                        this.update(&mut cx, |lsp_store, cx| {
                            lsp_store.pull_workspace_diagnostics(server_id);
                            lsp_store
                                .downstream_client
                                .as_ref()
                                .map(|(client, project_id)| {
                                    client.send(proto::PullWorkspaceDiagnostics {
                                        project_id: *project_id,
                                        server_id: server_id.to_proto(),
                                    })
                                })
                                .transpose()?;
                            anyhow::Ok(
                                lsp_store.pull_document_diagnostics_for_server(server_id, None, cx),
                            )
                        })??
                        .await;
                        Ok(())
                    }
                }
            })
            .detach();

        language_server
            .on_request::<lsp::request::ShowMessageRequest, _, _>({
                let this = lsp_store.clone();
                let name = name.to_string();
                let adapter = adapter.clone();
                move |params, cx| {
                    let this = this.clone();
                    let name = name.to_string();
                    let adapter = adapter.clone();
                    let mut cx = cx.clone();
                    async move {
                        let actions = params.actions.unwrap_or_default();
                        let message = params.message.clone();
                        let (tx, rx) = async_channel::bounded::<MessageActionItem>(1);
                        let level = match params.typ {
                            lsp::MessageType::ERROR => PromptLevel::Critical,
                            lsp::MessageType::WARNING => PromptLevel::Warning,
                            _ => PromptLevel::Info,
                        };
                        let request = LanguageServerPromptRequest::new(
                            level,
                            params.message,
                            actions,
                            name.clone(),
                            tx,
                        );

                        let did_update = this
                            .update(&mut cx, |_, cx| {
                                cx.emit(LspStoreEvent::LanguageServerPrompt(request));
                            })
                            .is_ok();
                        if did_update {
                            let response = rx.recv().await.ok();
                            if let Some(ref selected_action) = response {
                                let context = language::PromptResponseContext {
                                    message,
                                    selected_action: selected_action.clone(),
                                };
                                adapter.process_prompt_response(&context, &mut cx)
                            }

                            Ok(response)
                        } else {
                            Ok(None)
                        }
                    }
                }
            })
            .detach();
        language_server
            .on_notification::<lsp::notification::ShowMessage, _>({
                let this = lsp_store.clone();
                let name = name.to_string();
                move |params, cx| {
                    let this = this.clone();
                    let name = name.to_string();
                    let mut cx = cx.clone();

                    let (tx, _) = async_channel::bounded(1);
                    let level = match params.typ {
                        lsp::MessageType::ERROR => PromptLevel::Critical,
                        lsp::MessageType::WARNING => PromptLevel::Warning,
                        _ => PromptLevel::Info,
                    };
                    let request =
                        LanguageServerPromptRequest::new(level, params.message, vec![], name, tx);

                    let _ = this.update(&mut cx, |_, cx| {
                        cx.emit(LspStoreEvent::LanguageServerPrompt(request));
                    });
                }
            })
            .detach();

        let disk_based_diagnostics_progress_token =
            adapter.disk_based_diagnostics_progress_token.clone();

        language_server
            .on_notification::<lsp::notification::Progress, _>({
                let this = lsp_store.clone();
                move |params, cx| {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.on_lsp_progress(
                                params,
                                server_id,
                                disk_based_diagnostics_progress_token.clone(),
                                cx,
                            );
                        });
                    }
                }
            })
            .detach();

        language_server
            .on_notification::<lsp::notification::LogMessage, _>({
                let this = lsp_store.clone();
                move |params, cx| {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |_, cx| {
                            cx.emit(LspStoreEvent::LanguageServerLog(
                                server_id,
                                LanguageServerLogType::Log(params.typ),
                                params.message,
                            ));
                        });
                    }
                }
            })
            .detach();

        language_server
            .on_notification::<lsp::notification::LogTrace, _>({
                let this = lsp_store.clone();
                move |params, cx| {
                    let mut cx = cx.clone();
                    if let Some(this) = this.upgrade() {
                        this.update(&mut cx, |_, cx| {
                            cx.emit(LspStoreEvent::LanguageServerLog(
                                server_id,
                                LanguageServerLogType::Trace {
                                    verbose_info: params.verbose,
                                },
                                params.message,
                            ));
                        });
                    }
                }
            })
            .detach();

        vue_language_server_ext::register_requests(lsp_store.clone(), language_server);
        json_language_server_ext::register_requests(lsp_store.clone(), language_server);
        rust_analyzer_ext::register_notifications(lsp_store.clone(), language_server);
        clangd_ext::register_notifications(lsp_store, language_server, adapter);
    }
}
