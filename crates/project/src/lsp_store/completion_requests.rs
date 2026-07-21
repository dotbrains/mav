use super::*;

impl LspStore {
    #[inline(never)]
    pub fn completions(
        &self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        context: CompletionContext,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<CompletionResponse>>> {
        let language_registry = self.languages.clone();

        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let snapshot = buffer.read(cx).snapshot();
            let offset = position.to_offset(&snapshot);
            let scope = snapshot.language_scope_at(offset);
            let capable_lsps = self.all_capable_for_proto_request(
                buffer,
                |server_name, capabilities| {
                    capabilities.completion_provider.is_some()
                        && scope
                            .as_ref()
                            .map(|scope| scope.language_allowed(server_name))
                            .unwrap_or(true)
                },
                cx,
            );
            if capable_lsps.is_empty() {
                return Task::ready(Ok(Vec::new()));
            }

            let language = buffer.read(cx).language().cloned();

            let buffer = buffer.clone();

            cx.spawn(async move |this, cx| {
                let requests = join_all(
                    capable_lsps
                        .into_iter()
                        .map(|(id, server_name)| {
                            let request = GetCompletions {
                                position,
                                context: context.clone(),
                                server_id: Some(id),
                            };
                            let buffer = buffer.clone();
                            let language = language.clone();
                            let lsp_adapter = language.as_ref().and_then(|language| {
                                let adapters = language_registry.lsp_adapters(&language.name());
                                adapters
                                    .iter()
                                    .find(|adapter| adapter.name() == server_name)
                                    .or_else(|| adapters.first())
                                    .cloned()
                            });
                            let upstream_client = upstream_client.clone();
                            let response = this
                                .update(cx, |this, cx| {
                                    this.send_lsp_proto_request(
                                        buffer,
                                        upstream_client,
                                        project_id,
                                        request,
                                        cx,
                                    )
                                })
                                .log_err();
                            async move {
                                let response = response?.await.log_err()?;

                                let completions = populate_labels_for_completions(
                                    response.completions,
                                    language,
                                    lsp_adapter,
                                )
                                .await;

                                Some(CompletionResponse {
                                    completions,
                                    display_options: CompletionDisplayOptions::default(),
                                    is_incomplete: response.is_incomplete,
                                })
                            }
                        })
                        .collect::<Vec<_>>(),
                );
                Ok(requests.await.into_iter().flatten().collect::<Vec<_>>())
            })
        } else if let Some(local) = self.as_local() {
            let snapshot = buffer.read(cx).snapshot();
            let offset = position.to_offset(&snapshot);
            let scope = snapshot.language_scope_at(offset);
            let language = snapshot.language().cloned();
            let completion_settings = LanguageSettings::for_buffer(&buffer.read(cx), cx)
                .completions
                .clone();
            if !completion_settings.lsp {
                return Task::ready(Ok(Vec::new()));
            }

            let server_ids: Vec<_> = buffer.update(cx, |buffer, cx| {
                local
                    .language_servers_for_buffer(buffer, cx)
                    .filter(|(_, server)| server.capabilities().completion_provider.is_some())
                    .filter(|(adapter, _)| {
                        scope
                            .as_ref()
                            .map(|scope| scope.language_allowed(&adapter.name))
                            .unwrap_or(true)
                    })
                    .map(|(_, server)| server.server_id())
                    .collect()
            });

            let buffer = buffer.clone();
            let lsp_timeout = completion_settings.lsp_fetch_timeout_ms;
            let lsp_timeout = if lsp_timeout > 0 {
                Some(Duration::from_millis(lsp_timeout))
            } else {
                None
            };
            cx.spawn(async move |this,  cx| {
                let mut tasks = Vec::with_capacity(server_ids.len());
                this.update(cx, |lsp_store, cx| {
                    for server_id in server_ids {
                        let lsp_adapter = lsp_store.language_server_adapter_for_id(server_id);
                        let lsp_timeout = lsp_timeout
                            .map(|lsp_timeout| cx.background_executor().timer(lsp_timeout));
                        let mut timeout = cx.background_spawn(async move {
                            match lsp_timeout {
                                Some(lsp_timeout) => {
                                    lsp_timeout.await;
                                    true
                                },
                                None => false,
                            }
                        }).fuse();
                        let mut lsp_request = lsp_store.request_lsp(
                            buffer.clone(),
                            LanguageServerToQuery::Other(server_id),
                            GetCompletions {
                                position,
                                context: context.clone(),
                                server_id: Some(server_id),
                            },
                            cx,
                        ).fuse();
                        let new_task = cx.background_spawn(async move {
                            select_biased! {
                                response = lsp_request => anyhow::Ok(Some(response?)),
                                timeout_happened = timeout => {
                                    if timeout_happened {
                                        log::warn!("Fetching completions from server {server_id} timed out, timeout ms: {}", completion_settings.lsp_fetch_timeout_ms);
                                        Ok(None)
                                    } else {
                                        let completions = lsp_request.await?;
                                        Ok(Some(completions))
                                    }
                                },
                            }
                        });
                        tasks.push((lsp_adapter, new_task));
                    }
                })?;

                let futures = tasks.into_iter().map(async |(lsp_adapter, task)| {
                    let completion_response = task.await.ok()??;
                    let completions = populate_labels_for_completions(
                            completion_response.completions,
                            language.clone(),
                            lsp_adapter,
                        )
                        .await;
                    Some(CompletionResponse {
                        completions,
                        display_options: CompletionDisplayOptions::default(),
                        is_incomplete: completion_response.is_incomplete,
                    })
                });

                let responses: Vec<Option<CompletionResponse>> = join_all(futures).await;

                Ok(responses.into_iter().flatten().collect())
            })
        } else {
            Task::ready(Err(anyhow!("No upstream client or local language server")))
        }
    }

    pub fn apply_additional_edits_for_completion(
        &self,
        buffer_handle: Entity<Buffer>,
        completions: Rc<RefCell<Box<[Completion]>>>,
        completion_index: usize,
        push_to_history: bool,
        all_commit_ranges: Vec<Range<language::Anchor>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        if let Some((client, project_id)) = self.upstream_client() {
            let buffer = buffer_handle.read(cx);
            let buffer_id = buffer.remote_id();
            cx.spawn(async move |_, cx| {
                let request = {
                    let completion = completions.borrow()[completion_index].clone();
                    proto::ApplyCompletionAdditionalEdits {
                        project_id,
                        buffer_id: buffer_id.into(),
                        completion: Some(Self::serialize_completion(&CoreCompletion {
                            replace_range: completion.replace_range,
                            new_text: completion.new_text,
                            source: completion.source,
                        })),
                        all_commit_ranges: all_commit_ranges
                            .iter()
                            .cloned()
                            .map(language::proto::serialize_anchor_range)
                            .collect(),
                    }
                };

                let Some(transaction) = client.request(request).await?.transaction else {
                    return Ok(None);
                };

                let transaction = language::proto::deserialize_transaction(transaction)?;
                buffer_handle
                    .update(cx, |buffer, _| {
                        buffer.wait_for_edits(transaction.edit_ids.iter().copied())
                    })
                    .await?;
                if push_to_history {
                    buffer_handle.update(cx, |buffer, _| {
                        buffer.push_transaction(transaction.clone(), Instant::now());
                        buffer.finalize_last_transaction();
                    });
                }
                Ok(Some(transaction))
            })
        } else {
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();

            let Some(server) = buffer_handle.update(cx, |buffer, cx| {
                let completion = &completions.borrow()[completion_index];
                let server_id = completion.source.server_id()?;
                Some(
                    self.language_server_for_local_buffer(buffer, server_id, cx)?
                        .1
                        .clone(),
                )
            }) else {
                return Task::ready(Ok(None));
            };

            cx.spawn(async move |this, cx| {
                Self::resolve_completion_local(
                    server.clone(),
                    completions.clone(),
                    completion_index,
                    request_timeout,
                )
                .await
                .context("resolving completion")?;
                let completion = completions.borrow()[completion_index].clone();
                let additional_text_edits = completion
                    .source
                    .lsp_completion(true)
                    .as_ref()
                    .and_then(|lsp_completion| lsp_completion.additional_text_edits.clone());
                if let Some(edits) = additional_text_edits {
                    let edits = this
                        .update(cx, |this, cx| {
                            this.as_local_mut().unwrap().edits_from_lsp(
                                &buffer_handle,
                                edits,
                                server.server_id(),
                                None,
                                cx,
                            )
                        })?
                        .await?;

                    buffer_handle.update(cx, |buffer, cx| {
                        buffer.finalize_last_transaction();
                        buffer.start_transaction();

                        for (range, text) in edits {
                            let primary = &completion.replace_range;

                            // Special case: if both ranges start at the very beginning of the file (line 0, column 0),
                            // and the primary completion is just an insertion (empty range), then this is likely
                            // an auto-import scenario and should not be considered overlapping
                            // https://github.com/mav-industries/mav/issues/26136
                            let is_file_start_auto_import = {
                                let snapshot = buffer.snapshot();
                                let primary_start_point = primary.start.to_point(&snapshot);
                                let range_start_point = range.start.to_point(&snapshot);

                                let result = primary_start_point.row == 0
                                    && primary_start_point.column == 0
                                    && range_start_point.row == 0
                                    && range_start_point.column == 0;

                                result
                            };

                            let has_overlap = if is_file_start_auto_import {
                                false
                            } else {
                                all_commit_ranges.iter().any(|commit_range| {
                                    let start_within =
                                        commit_range.start.cmp(&range.start, buffer).is_le()
                                            && commit_range.end.cmp(&range.start, buffer).is_ge();
                                    let end_within =
                                        range.start.cmp(&commit_range.end, buffer).is_le()
                                            && range.end.cmp(&commit_range.end, buffer).is_ge();
                                    start_within || end_within
                                })
                            };

                            //Skip additional edits which overlap with the primary completion edit
                            //https://github.com/mav-industries/mav/pull/1871
                            if !has_overlap {
                                buffer.edit([(range, text)], None, cx);
                            }
                        }

                        let transaction = if buffer.end_transaction(cx).is_some() {
                            let transaction = buffer.finalize_last_transaction().unwrap().clone();
                            if !push_to_history {
                                buffer.forget_transaction(transaction.id);
                            }
                            Some(transaction)
                        } else {
                            None
                        };
                        Ok(transaction)
                    })
                } else {
                    Ok(None)
                }
            })
        }
    }
}
