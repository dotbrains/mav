use super::*;

impl Copilot {
    pub fn language_server(&self) -> Option<&Arc<LanguageServer>> {
        if let CopilotServer::Running(server) = &self.server {
            Some(&server.lsp)
        } else {
            None
        }
    }

    pub fn register_buffer(&mut self, buffer: &Entity<Buffer>, cx: &mut Context<Self>) {
        let weak_buffer = buffer.downgrade();
        self.buffers.insert(weak_buffer.clone());

        if let CopilotServer::Running(RunningCopilotServer {
            lsp: server,
            sign_in_status: status,
            registered_buffers,
            ..
        }) = &mut self.server
        {
            if !matches!(status, SignInStatus::Authorized) {
                return;
            }

            let entry = registered_buffers.entry(buffer.entity_id());
            if let Entry::Vacant(e) = entry {
                let Ok(uri) = uri_for_buffer(buffer, cx) else {
                    return;
                };
                let language_id = id_for_language(buffer.read(cx).language());
                let snapshot = buffer.read(cx).snapshot();
                server
                    .notify::<lsp::notification::DidOpenTextDocument>(
                        lsp::DidOpenTextDocumentParams {
                            text_document: lsp::TextDocumentItem {
                                uri: uri.clone(),
                                language_id: language_id.clone(),
                                version: 0,
                                text: snapshot.text(),
                            },
                        },
                    )
                    .ok();

                e.insert(RegisteredBuffer {
                    uri,
                    language_id,
                    snapshot,
                    snapshot_version: 0,
                    pending_buffer_change: Task::ready(Some(())),
                    _subscriptions: [
                        cx.subscribe(buffer, |this, buffer, event, cx| {
                            this.handle_buffer_event(buffer, event, cx).log_err();
                        }),
                        cx.observe_release(buffer, move |this, _buffer, _cx| {
                            this.buffers.remove(&weak_buffer);
                            this.unregister_buffer(&weak_buffer);
                        }),
                    ],
                });
            }
        }
    }

    fn handle_buffer_event(
        &mut self,
        buffer: Entity<Buffer>,
        event: &language::BufferEvent,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if let Ok(server) = self.server.as_running()
            && let Some(registered_buffer) = server.registered_buffers.get_mut(&buffer.entity_id())
        {
            match event {
                language::BufferEvent::Edited { .. } => {
                    drop(registered_buffer.report_changes(&buffer, cx));
                }
                language::BufferEvent::Saved => {
                    server
                        .lsp
                        .notify::<lsp::notification::DidSaveTextDocument>(
                            lsp::DidSaveTextDocumentParams {
                                text_document: lsp::TextDocumentIdentifier::new(
                                    registered_buffer.uri.clone(),
                                ),
                                text: None,
                            },
                        )
                        .ok();
                }
                language::BufferEvent::FileHandleChanged
                | language::BufferEvent::LanguageChanged(_) => {
                    let new_language_id = id_for_language(buffer.read(cx).language());
                    let Ok(new_uri) = uri_for_buffer(&buffer, cx) else {
                        return Ok(());
                    };
                    if new_uri != registered_buffer.uri
                        || new_language_id != registered_buffer.language_id
                    {
                        let old_uri = mem::replace(&mut registered_buffer.uri, new_uri);
                        registered_buffer.language_id = new_language_id;
                        server
                            .lsp
                            .notify::<lsp::notification::DidCloseTextDocument>(
                                lsp::DidCloseTextDocumentParams {
                                    text_document: lsp::TextDocumentIdentifier::new(old_uri),
                                },
                            )
                            .ok();
                        server
                            .lsp
                            .notify::<lsp::notification::DidOpenTextDocument>(
                                lsp::DidOpenTextDocumentParams {
                                    text_document: lsp::TextDocumentItem::new(
                                        registered_buffer.uri.clone(),
                                        registered_buffer.language_id.clone(),
                                        registered_buffer.snapshot_version,
                                        registered_buffer.snapshot.text(),
                                    ),
                                },
                            )
                            .ok();
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub(super) fn unregister_buffer(&mut self, buffer: &WeakEntity<Buffer>) {
        if let Ok(server) = self.server.as_running()
            && let Some(buffer) = server.registered_buffers.remove(&buffer.entity_id())
        {
            server
                .lsp
                .notify::<lsp::notification::DidCloseTextDocument>(
                    lsp::DidCloseTextDocumentParams {
                        text_document: lsp::TextDocumentIdentifier::new(buffer.uri),
                    },
                )
                .ok();
        }
    }

    pub(crate) fn completions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: Anchor,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<CopilotEditPrediction>>> {
        self.register_buffer(buffer, cx);

        let server = match self.server.as_authenticated() {
            Ok(server) => server,
            Err(error) => return Task::ready(Err(error)),
        };
        let buffer_entity = buffer.clone();
        let lsp = server.lsp.clone();
        let Some(registered_buffer) = server.registered_buffers.get_mut(&buffer.entity_id()) else {
            return Task::ready(Err(anyhow::anyhow!("buffer not registered")));
        };
        let pending_snapshot = registered_buffer.report_changes(buffer, cx);
        let buffer = buffer.read(cx);
        let uri = registered_buffer.uri.clone();
        let position = position.to_point_utf16(buffer);
        let snapshot = buffer.snapshot();
        let settings = snapshot.settings_at(0, cx);
        let tab_size = settings.tab_size.get();
        let hard_tabs = settings.hard_tabs;
        drop(settings);

        let request_timeout = ProjectSettings::get_global(cx)
            .global_lsp_settings
            .get_request_timeout();

        let nes_enabled = AllLanguageSettings::get_global(cx)
            .edit_predictions
            .copilot
            .enable_next_edit_suggestions
            .unwrap_or(true);

        cx.background_spawn(async move {
            let (version, snapshot) = pending_snapshot.await?;
            let lsp_position = point_to_lsp(position);

            let nes_fut = if nes_enabled {
                lsp.request::<NextEditSuggestions>(
                    request::NextEditSuggestionsParams {
                        text_document: lsp::VersionedTextDocumentIdentifier {
                            uri: uri.clone(),
                            version,
                        },
                        position: lsp_position,
                    },
                    request_timeout,
                )
                .map(|resp| {
                    resp.into_response()
                        .ok()
                        .map(|result| {
                            result
                                .edits
                                .into_iter()
                                .map(|completion| {
                                    let start = snapshot.clip_point_utf16(
                                        point_from_lsp(completion.range.start),
                                        Bias::Left,
                                    );
                                    let end = snapshot.clip_point_utf16(
                                        point_from_lsp(completion.range.end),
                                        Bias::Left,
                                    );
                                    CopilotEditPrediction {
                                        buffer: buffer_entity.clone(),
                                        range: snapshot.anchor_before(start)
                                            ..snapshot.anchor_after(end),
                                        text: completion.text,
                                        command: completion.command,
                                        snapshot: snapshot.clone(),
                                        source: CompletionSource::NextEditSuggestion,
                                    }
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .left_future()
                .fuse()
            } else {
                future::ready(Vec::<CopilotEditPrediction>::new())
                    .right_future()
                    .fuse()
            };

            let inline_fut = lsp
                .request::<InlineCompletions>(
                    request::InlineCompletionsParams {
                        text_document: lsp::VersionedTextDocumentIdentifier {
                            uri: uri.clone(),
                            version,
                        },
                        position: lsp_position,
                        context: InlineCompletionContext {
                            trigger_kind: InlineCompletionTriggerKind::Automatic,
                        },
                        formatting_options: Some(FormattingOptions {
                            tab_size,
                            insert_spaces: !hard_tabs,
                        }),
                    },
                    request_timeout,
                )
                .map(|resp| {
                    resp.into_response()
                        .ok()
                        .map(|result| {
                            result
                                .items
                                .into_iter()
                                .map(|item| {
                                    let start = snapshot.clip_point_utf16(
                                        point_from_lsp(item.range.start),
                                        Bias::Left,
                                    );
                                    let end = snapshot.clip_point_utf16(
                                        point_from_lsp(item.range.end),
                                        Bias::Left,
                                    );
                                    CopilotEditPrediction {
                                        buffer: buffer_entity.clone(),
                                        range: snapshot.anchor_before(start)
                                            ..snapshot.anchor_after(end),
                                        text: item.insert_text,
                                        command: item.command,
                                        snapshot: snapshot.clone(),
                                        source: CompletionSource::InlineCompletion,
                                    }
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .fuse();

            futures::pin_mut!(nes_fut, inline_fut);

            let mut nes_result: Option<Vec<CopilotEditPrediction>> = None;
            let mut inline_result: Option<Vec<CopilotEditPrediction>> = None;

            loop {
                select_biased! {
                    nes = nes_fut => {
                        if !nes.is_empty() {
                            return Ok(nes);
                        }
                        nes_result = Some(nes);
                    }
                    inline = inline_fut => {
                        if !inline.is_empty() {
                            return Ok(inline);
                        }
                        inline_result = Some(inline);
                    }
                    complete => break,
                }

                if let (Some(nes), Some(inline)) = (&nes_result, &inline_result) {
                    return if !nes.is_empty() {
                        Ok(nes.clone())
                    } else {
                        Ok(inline.clone())
                    };
                }
            }

            Ok(nes_result.or(inline_result).unwrap_or_default())
        })
    }

    pub(crate) fn accept_completion(
        &mut self,
        completion: &CopilotEditPrediction,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let server = match self.server.as_authenticated() {
            Ok(server) => server,
            Err(error) => return Task::ready(Err(error)),
        };
        if let Some(command) = &completion.command {
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();

            let request = server.lsp.request::<lsp::ExecuteCommand>(
                lsp::ExecuteCommandParams {
                    command: command.command.clone(),
                    arguments: command.arguments.clone().unwrap_or_default(),
                    ..Default::default()
                },
                request_timeout,
            );
            cx.background_spawn(async move {
                request
                    .await
                    .into_response()
                    .context("copilot: notify accepted")?;
                Ok(())
            })
        } else {
            Task::ready(Ok(()))
        }
    }
}
