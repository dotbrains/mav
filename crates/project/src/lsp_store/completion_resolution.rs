use super::*;

impl LspStore {
    pub fn resolve_completions(
        &self,
        buffer: Entity<Buffer>,
        completion_indices: Vec<usize>,
        completions: Rc<RefCell<Box<[Completion]>>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let client = self.upstream_client();
        let buffer_id = buffer.read(cx).remote_id();
        let buffer_snapshot = buffer.read(cx).snapshot();

        if !self.check_if_capable_for_proto_request(
            &buffer,
            GetCompletions::can_resolve_completions,
            cx,
        ) {
            return Task::ready(Ok(false));
        }
        cx.spawn(async move |lsp_store, cx| {
            let request_timeout = cx.update(|app| {
                ProjectSettings::get_global(app)
                    .global_lsp_settings
                    .get_request_timeout()
            });

            let mut did_resolve = false;
            if let Some((client, project_id)) = client {
                for completion_index in completion_indices {
                    let server_id = {
                        let completion = &completions.borrow()[completion_index];
                        completion.source.server_id()
                    };
                    if let Some(server_id) = server_id {
                        if Self::resolve_completion_remote(
                            project_id,
                            server_id,
                            buffer_id,
                            completions.clone(),
                            completion_index,
                            client.clone(),
                        )
                        .await
                        .log_err()
                        .is_some()
                        {
                            did_resolve = true;
                        }
                    } else {
                        resolve_word_completion(
                            &buffer_snapshot,
                            &mut completions.borrow_mut()[completion_index],
                        );
                    }
                }
            } else {
                for completion_index in completion_indices {
                    let server_id = {
                        let completion = &completions.borrow()[completion_index];
                        completion.source.server_id()
                    };
                    if let Some(server_id) = server_id {
                        let server_and_adapter = lsp_store
                            .read_with(cx, |lsp_store, _| {
                                let server = lsp_store.language_server_for_id(server_id)?;
                                let adapter =
                                    lsp_store.language_server_adapter_for_id(server.server_id())?;
                                Some((server, adapter))
                            })
                            .ok()
                            .flatten();
                        let Some((server, adapter)) = server_and_adapter else {
                            continue;
                        };

                        let resolved = Self::resolve_completion_local(
                            server,
                            completions.clone(),
                            completion_index,
                            request_timeout,
                        )
                        .await
                        .log_err()
                        .is_some();
                        if resolved {
                            Self::regenerate_completion_labels(
                                adapter,
                                &buffer_snapshot,
                                completions.clone(),
                                completion_index,
                            )
                            .await
                            .log_err();
                            did_resolve = true;
                        }
                    } else {
                        resolve_word_completion(
                            &buffer_snapshot,
                            &mut completions.borrow_mut()[completion_index],
                        );
                    }
                }
            }

            Ok(did_resolve)
        })
    }

    pub(super) async fn resolve_completion_local(
        server: Arc<lsp::LanguageServer>,
        completions: Rc<RefCell<Box<[Completion]>>>,
        completion_index: usize,
        request_timeout: Duration,
    ) -> Result<()> {
        let server_id = server.server_id();
        if !GetCompletions::can_resolve_completions(&server.capabilities()) {
            return Ok(());
        }

        let request = {
            let completion = &completions.borrow()[completion_index];
            match &completion.source {
                CompletionSource::Lsp {
                    lsp_completion,
                    resolved,
                    server_id: completion_server_id,
                    ..
                } => {
                    if *resolved {
                        return Ok(());
                    }
                    anyhow::ensure!(
                        server_id == *completion_server_id,
                        "server_id mismatch, querying completion resolve for {server_id} but completion server id is {completion_server_id}"
                    );
                    server.request::<lsp::request::ResolveCompletionItem>(
                        *lsp_completion.clone(),
                        request_timeout,
                    )
                }
                CompletionSource::BufferWord { .. }
                | CompletionSource::Dap { .. }
                | CompletionSource::Custom => {
                    return Ok(());
                }
            }
        };
        let resolved_completion = request
            .await
            .into_response()
            .context("resolve completion")?;

        let mut completions = completions.borrow_mut();
        let completion = &mut completions[completion_index];
        if let CompletionSource::Lsp {
            lsp_completion,
            resolved,
            server_id: completion_server_id,
            ..
        } = &mut completion.source
        {
            if *resolved {
                return Ok(());
            }
            anyhow::ensure!(
                server_id == *completion_server_id,
                "server_id mismatch, applying completion resolve for {server_id} but completion server id is {completion_server_id}"
            );
            **lsp_completion = resolved_completion;
            *resolved = true;

            // We must not use any data such as sortText, filterText, insertText and textEdit to edit `Completion` since they are not supposed to change during resolve.
            // Refer: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_completion
            //
            // We still re-derive new_text here as a workaround for the specific
            // VS Code TypeScript completion resolve flow that vtsls wraps:
            // https://github.com/microsoft/vscode/blob/838b48504cd9a2338e2ca9e854da9cec990c4d57/extensions/typescript-language-features/src/languageFeatures/completions.ts#L218
            //
            // Some servers (e.g. vtsls with completeFunctionCalls) update
            // insertText/textEdit during resolve to add snippet content like
            // function call parentheses.
            //
            // vtsls resolve flow:
            //   https://github.com/yioneko/vtsls/blob/fecf52324a30e72dfab1537047556076720c1a5f/packages/service/src/service/completion.ts#L228-L244
            // vtsls converter (isSnippet / insertTextFormat):
            //   https://github.com/yioneko/vtsls/blob/28e075105d7711d635ebf8aefc971bb8e1d2fe65/packages/service/src/utils/converter.ts#L149-L200
            //
            // NB: We only update the text content here, NOT the replace/insert
            // ranges on `Completion`. Those ranges were converted to anchors from
            // the original response and stay valid across buffer edits. The LSP
            // ranges in the resolved text_edit are stale when completions are
            // cached across keystrokes (see #34094).
            let resolved_new_text = lsp_completion
                .text_edit
                .as_ref()
                .map(|edit| match edit {
                    lsp::CompletionTextEdit::Edit(e) => e.new_text.clone(),
                    lsp::CompletionTextEdit::InsertAndReplace(e) => e.new_text.clone(),
                })
                .or_else(|| lsp_completion.insert_text.clone());
            if let Some(mut resolved_new_text) = resolved_new_text {
                LineEnding::normalize(&mut resolved_new_text);
                completion.new_text = resolved_new_text;
            }
        }
        Ok(())
    }

    pub(super) async fn regenerate_completion_labels(
        adapter: Arc<CachedLspAdapter>,
        snapshot: &BufferSnapshot,
        completions: Rc<RefCell<Box<[Completion]>>>,
        completion_index: usize,
    ) -> Result<()> {
        let completion_item = completions.borrow()[completion_index]
            .source
            .lsp_completion(true)
            .map(Cow::into_owned);
        if let Some(lsp_documentation) = completion_item
            .as_ref()
            .and_then(|completion_item| completion_item.documentation.clone())
        {
            let mut completions = completions.borrow_mut();
            let completion = &mut completions[completion_index];
            completion.documentation = Some(lsp_documentation.into());
        } else {
            let mut completions = completions.borrow_mut();
            let completion = &mut completions[completion_index];
            completion.documentation = Some(CompletionDocumentation::Undocumented);
        }

        let mut new_label = match completion_item {
            Some(completion_item) => {
                // Some language servers always return `detail` lazily via resolve, regardless of
                // the resolvable properties Mav advertises. Regenerate labels here to handle this.
                // See: https://github.com/yioneko/vtsls/issues/213
                let language = snapshot.language();
                match language {
                    Some(language) => {
                        adapter
                            .labels_for_completions(
                                std::slice::from_ref(&completion_item),
                                language,
                            )
                            .await?
                    }
                    None => Vec::new(),
                }
                .pop()
                .flatten()
                .unwrap_or_else(|| {
                    CodeLabel::fallback_for_completion(
                        &completion_item,
                        language.map(|language| language.as_ref()),
                    )
                })
            }
            None => CodeLabel::plain(
                completions.borrow()[completion_index].new_text.clone(),
                None,
            ),
        };
        ensure_uniform_list_compatible_label(&mut new_label);

        let mut completions = completions.borrow_mut();
        let completion = &mut completions[completion_index];
        if completion.label.filter_text() == new_label.filter_text() {
            completion.label = new_label;
        } else {
            log::error!(
                "Resolved completion changed display label from {} to {}. \
                 Refusing to apply this because it changes the fuzzy match text from {} to {}",
                completion.label.text(),
                new_label.text(),
                completion.label.filter_text(),
                new_label.filter_text()
            );
        }

        Ok(())
    }

    pub(super) async fn resolve_completion_remote(
        project_id: u64,
        server_id: LanguageServerId,
        buffer_id: BufferId,
        completions: Rc<RefCell<Box<[Completion]>>>,
        completion_index: usize,
        client: AnyProtoClient,
    ) -> Result<()> {
        let lsp_completion = {
            let completion = &completions.borrow()[completion_index];
            match &completion.source {
                CompletionSource::Lsp {
                    lsp_completion,
                    resolved,
                    server_id: completion_server_id,
                    ..
                } => {
                    anyhow::ensure!(
                        server_id == *completion_server_id,
                        "remote server_id mismatch, querying completion resolve for {server_id} but completion server id is {completion_server_id}"
                    );
                    if *resolved {
                        return Ok(());
                    }
                    serde_json::to_string(lsp_completion).unwrap().into_bytes()
                }
                CompletionSource::Custom
                | CompletionSource::Dap { .. }
                | CompletionSource::BufferWord { .. } => {
                    return Ok(());
                }
            }
        };
        let request = proto::ResolveCompletionDocumentation {
            project_id,
            language_server_id: server_id.0 as u64,
            lsp_completion,
            buffer_id: buffer_id.into(),
        };

        let response = client
            .request(request)
            .await
            .context("completion documentation resolve proto request")?;
        let resolved_lsp_completion = serde_json::from_slice(&response.lsp_completion)?;

        let documentation = if response.documentation.is_empty() {
            CompletionDocumentation::Undocumented
        } else if response.documentation_is_markdown {
            CompletionDocumentation::MultiLineMarkdown(response.documentation.into())
        } else if response.documentation.lines().count() <= 1 {
            CompletionDocumentation::SingleLine(response.documentation.into())
        } else {
            CompletionDocumentation::MultiLinePlainText(response.documentation.into())
        };

        let mut completions = completions.borrow_mut();
        let completion = &mut completions[completion_index];
        completion.documentation = Some(documentation);
        if let CompletionSource::Lsp {
            insert_range,
            lsp_completion,
            resolved,
            server_id: completion_server_id,
            lsp_defaults: _,
        } = &mut completion.source
        {
            let completion_insert_range = response
                .old_insert_start
                .and_then(deserialize_anchor)
                .zip(response.old_insert_end.and_then(deserialize_anchor));
            *insert_range = completion_insert_range.map(|(start, end)| start..end);

            if *resolved {
                return Ok(());
            }
            anyhow::ensure!(
                server_id == *completion_server_id,
                "remote server_id mismatch, applying completion resolve for {server_id} but completion server id is {completion_server_id}"
            );
            **lsp_completion = resolved_lsp_completion;
            *resolved = true;
        }

        let replace_range = response
            .old_replace_start
            .and_then(deserialize_anchor)
            .zip(response.old_replace_end.and_then(deserialize_anchor));
        if let Some((old_replace_start, old_replace_end)) = replace_range
            && !response.new_text.is_empty()
        {
            completion.new_text = response.new_text;
            completion.replace_range = old_replace_start..old_replace_end;
        }

        Ok(())
    }
}
