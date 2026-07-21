use super::*;

impl LspStore {
    pub(super) async fn handle_start_language_server(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::StartLanguageServer>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let server = envelope.payload.server.context("invalid server")?;
        let server_capabilities =
            serde_json::from_str::<lsp::ServerCapabilities>(&envelope.payload.capabilities)
                .with_context(|| {
                    format!(
                        "incorrect server capabilities {}",
                        envelope.payload.capabilities
                    )
                })?;
        lsp_store.update(&mut cx, |lsp_store, cx| {
            let server_id = LanguageServerId(server.id as usize);
            let server_name = LanguageServerName::from_proto(server.name.clone());
            let language_name = server.language_name.map(LanguageName::from_proto);
            lsp_store
                .lsp_server_capabilities
                .insert(server_id, server_capabilities);

            if let Some(ref lang_name) = language_name {
                lsp_store.try_register_remote_adapter_locally(&server_name, lang_name);
            }

            lsp_store.language_server_statuses.insert(
                server_id,
                LanguageServerStatus {
                    name: server_name.clone(),
                    language_name,
                    server_version: None,
                    server_readable_version: None,
                    pending_work: Default::default(),
                    has_pending_diagnostic_updates: false,
                    progress_tokens: Default::default(),
                    worktree: server.worktree_id.map(WorktreeId::from_proto),
                    binary: None,
                    configuration: None,
                    workspace_folders: BTreeSet::new(),
                    process_id: None,
                },
            );
            cx.emit(LspStoreEvent::LanguageServerAdded(
                server_id,
                server_name,
                server.worktree_id.map(WorktreeId::from_proto),
            ));
            cx.notify();
        });
        Ok(())
    }

    pub(super) async fn handle_update_language_server(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateLanguageServer>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        lsp_store.update(&mut cx, |lsp_store, cx| {
            let language_server_id = LanguageServerId(envelope.payload.language_server_id as usize);

            match envelope.payload.variant.context("invalid variant")? {
                proto::update_language_server::Variant::WorkStart(payload) => {
                    lsp_store.on_lsp_work_start(
                        language_server_id,
                        ProgressToken::from_proto(payload.token.context("missing progress token")?)
                            .context("invalid progress token value")?,
                        LanguageServerProgress {
                            title: payload.title,
                            is_disk_based_diagnostics_progress: false,
                            is_cancellable: payload.is_cancellable.unwrap_or(false),
                            message: payload.message,
                            percentage: payload.percentage.map(|p| p as usize),
                            last_update_at: cx.background_executor().now(),
                        },
                        cx,
                    );
                }
                proto::update_language_server::Variant::WorkProgress(payload) => {
                    lsp_store.on_lsp_work_progress(
                        language_server_id,
                        ProgressToken::from_proto(payload.token.context("missing progress token")?)
                            .context("invalid progress token value")?,
                        LanguageServerProgress {
                            title: None,
                            is_disk_based_diagnostics_progress: false,
                            is_cancellable: payload.is_cancellable.unwrap_or(false),
                            message: payload.message,
                            percentage: payload.percentage.map(|p| p as usize),
                            last_update_at: cx.background_executor().now(),
                        },
                        cx,
                    );
                }

                proto::update_language_server::Variant::WorkEnd(payload) => {
                    lsp_store.on_lsp_work_end(
                        language_server_id,
                        ProgressToken::from_proto(payload.token.context("missing progress token")?)
                            .context("invalid progress token value")?,
                        cx,
                    );
                }

                proto::update_language_server::Variant::DiskBasedDiagnosticsUpdating(_) => {
                    lsp_store.disk_based_diagnostics_started(language_server_id, cx);
                }

                proto::update_language_server::Variant::DiskBasedDiagnosticsUpdated(_) => {
                    lsp_store.disk_based_diagnostics_finished(language_server_id, cx)
                }

                proto::update_language_server::Variant::Removed(_) => {
                    lsp_store
                        .language_server_statuses
                        .remove(&language_server_id);
                    lsp_store.cleanup_lsp_data(language_server_id);
                    cx.emit(LspStoreEvent::LanguageServerRemoved(language_server_id));
                    cx.notify();
                }

                non_lsp @ proto::update_language_server::Variant::StatusUpdate(_)
                | non_lsp @ proto::update_language_server::Variant::RegisteredForBuffer(_)
                | non_lsp @ proto::update_language_server::Variant::MetadataUpdated(_) => {
                    cx.emit(LspStoreEvent::LanguageServerUpdate {
                        language_server_id,
                        name: envelope
                            .payload
                            .server_name
                            .map(SharedString::new)
                            .map(LanguageServerName),
                        message: non_lsp,
                    });
                }
            }

            Ok(())
        })
    }

    pub(super) async fn handle_language_server_log(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::LanguageServerLog>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let language_server_id = LanguageServerId(envelope.payload.language_server_id as usize);
        let log_type = envelope
            .payload
            .log_type
            .map(LanguageServerLogType::from_proto)
            .context("invalid language server log type")?;

        let message = envelope.payload.message;

        this.update(&mut cx, |_, cx| {
            cx.emit(LspStoreEvent::LanguageServerLog(
                language_server_id,
                log_type,
                message,
            ));
        });
        Ok(())
    }

    pub(super) async fn handle_lsp_ext_cancel_flycheck(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::LspExtCancelFlycheck>,
        cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let server_id = LanguageServerId(envelope.payload.language_server_id as usize);
        let task = lsp_store.read_with(&cx, |lsp_store, _| {
            if let Some(server) = lsp_store.language_server_for_id(server_id) {
                Some(server.notify::<lsp_store::lsp_ext_command::LspExtCancelFlycheck>(()))
            } else {
                None
            }
        });
        if let Some(task) = task {
            task.context("handling lsp ext cancel flycheck")?;
        }

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_lsp_ext_run_flycheck(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::LspExtRunFlycheck>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let server_id = LanguageServerId(envelope.payload.language_server_id as usize);
        lsp_store.update(&mut cx, |lsp_store, cx| {
            if let Some(server) = lsp_store.language_server_for_id(server_id) {
                let text_document = if envelope.payload.current_file_only {
                    let buffer_id = envelope
                        .payload
                        .buffer_id
                        .map(|id| BufferId::new(id))
                        .transpose()?;
                    buffer_id
                        .and_then(|buffer_id| {
                            lsp_store
                                .buffer_store()
                                .read(cx)
                                .get(buffer_id)
                                .and_then(|buffer| {
                                    Some(buffer.read(cx).file()?.as_local()?.abs_path(cx))
                                })
                                .map(|path| make_text_document_identifier(&path))
                        })
                        .transpose()?
                } else {
                    None
                };
                server.notify::<lsp_store::lsp_ext_command::LspExtRunFlycheck>(
                    lsp_store::lsp_ext_command::RunFlycheckParams { text_document },
                )?;
            }
            anyhow::Ok(())
        })?;

        Ok(proto::Ack {})
    }

    pub(super) async fn handle_lsp_ext_clear_flycheck(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::LspExtClearFlycheck>,
        cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let server_id = LanguageServerId(envelope.payload.language_server_id as usize);
        lsp_store.read_with(&cx, |lsp_store, _| {
            if let Some(server) = lsp_store.language_server_for_id(server_id) {
                Some(server.notify::<lsp_store::lsp_ext_command::LspExtClearFlycheck>(()))
            } else {
                None
            }
        });

        Ok(proto::Ack {})
    }
}
