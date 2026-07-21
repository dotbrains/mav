use super::*;

impl LspStore {
    pub(super) async fn handle_lsp_query(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::LspQuery>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        use proto::lsp_query::Request;
        let sender_id = envelope.original_sender_id().unwrap_or_default();
        let lsp_query = envelope.payload;
        let lsp_request_id = LspRequestId(lsp_query.lsp_request_id);
        let server_id = lsp_query.server_id.map(LanguageServerId::from_proto);
        match lsp_query.request.context("invalid LSP query request")? {
            Request::GetReferences(get_references) => {
                let position = get_references.position.clone().and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetReferences>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_references,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDocumentColor(get_document_color) => {
                Self::query_lsp_locally::<GetDocumentColor>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_document_color,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetFoldingRanges(get_folding_ranges) => {
                Self::query_lsp_locally::<GetFoldingRanges>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_folding_ranges,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDocumentSymbols(get_document_symbols) => {
                Self::query_lsp_locally::<GetDocumentSymbols>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_document_symbols,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDocumentLinks(get_document_links) => {
                let (buffer_version, buffer) = Self::wait_for_buffer_version::<GetDocumentLinks>(
                    &lsp_store,
                    &get_document_links,
                    &mut cx,
                )
                .await?;
                lsp_store.update(&mut cx, |lsp_store, cx| {
                    let document_links_task = lsp_store.fetch_document_links(&buffer, cx);
                    let fetch_task = cx.background_spawn(async move {
                        document_links_task
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(server_id, links)| {
                                (server_id, links.into_values().collect::<Vec<_>>())
                            })
                            .collect()
                    });
                    lsp_store.serve_lsp_query::<GetDocumentLinks>(
                        server_id,
                        sender_id,
                        lsp_request_id,
                        &buffer,
                        buffer_version,
                        fetch_task,
                        cx,
                    );
                });
            }
            Request::GetHover(get_hover) => {
                let position = get_hover.position.clone().and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetHover>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_hover,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetCodeActions(get_code_actions) => {
                Self::query_lsp_locally::<GetCodeActions>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_code_actions,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetSignatureHelp(get_signature_help) => {
                let position = get_signature_help
                    .position
                    .clone()
                    .and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetSignatureHelp>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_signature_help,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetCodeLens(get_code_lens) => {
                Self::query_lsp_locally::<GetCodeLens>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_code_lens,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDefinition(get_definition) => {
                let position = get_definition.position.clone().and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetDefinitions>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_definition,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDeclaration(get_declaration) => {
                let position = get_declaration
                    .position
                    .clone()
                    .and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetDeclarations>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_declaration,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetTypeDefinition(get_type_definition) => {
                let position = get_type_definition
                    .position
                    .clone()
                    .and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetTypeDefinitions>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_type_definition,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetImplementation(get_implementation) => {
                let position = get_implementation
                    .position
                    .clone()
                    .and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetImplementations>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_implementation,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::InlayHints(inlay_hints) => {
                let query_start = inlay_hints
                    .start
                    .clone()
                    .and_then(deserialize_anchor)
                    .context("invalid inlay hints range start")?;
                let query_end = inlay_hints
                    .end
                    .clone()
                    .and_then(deserialize_anchor)
                    .context("invalid inlay hints range end")?;
                Self::deduplicate_range_based_lsp_requests::<InlayHints>(
                    &lsp_store,
                    server_id,
                    lsp_request_id,
                    &inlay_hints,
                    query_start..query_end,
                    &mut cx,
                )
                .await
                .context("preparing inlay hints request")?;
                Self::query_lsp_locally::<InlayHints>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    inlay_hints,
                    None,
                    &mut cx,
                )
                .await
                .context("querying for inlay hints")?
            }
            //////////////////////////////
            // Below are LSP queries that need to fetch more data,
            // hence cannot just proxy the request to language server with `query_lsp_locally`.
            Request::GetDocumentDiagnostics(get_document_diagnostics) => {
                let (_, buffer) = Self::wait_for_buffer_version::<GetDocumentDiagnostics>(
                    &lsp_store,
                    &get_document_diagnostics,
                    &mut cx,
                )
                .await?;
                lsp_store.update(&mut cx, |lsp_store, cx| {
                    let lsp_data = lsp_store.latest_lsp_data(&buffer, cx);
                    let key = LspKey {
                        request_type: TypeId::of::<GetDocumentDiagnostics>(),
                        server_queried: server_id,
                    };
                    if <GetDocumentDiagnostics as LspCommand>::ProtoRequest::stop_previous_requests(
                    ) {
                        if let Some(lsp_requests) = lsp_data.lsp_requests.get_mut(&key) {
                            lsp_requests.clear();
                        };
                    }

                    lsp_data.lsp_requests.entry(key).or_default().insert(
                        lsp_request_id,
                        cx.spawn(async move |lsp_store, cx| {
                            let diagnostics_pull = lsp_store
                                .update(cx, |lsp_store, cx| {
                                    lsp_store.pull_diagnostics_for_buffer(buffer, cx)
                                })
                                .ok();
                            if let Some(diagnostics_pull) = diagnostics_pull {
                                match diagnostics_pull.await {
                                    Ok(()) => {}
                                    Err(e) => log::error!("Failed to pull diagnostics: {e:#}"),
                                };
                            }
                        }),
                    );
                });
            }
            Request::SemanticTokens(semantic_tokens) => {
                let (buffer_version, buffer) = Self::wait_for_buffer_version::<SemanticTokensFull>(
                    &lsp_store,
                    &semantic_tokens,
                    &mut cx,
                )
                .await?;
                let for_server = semantic_tokens.for_server.map(LanguageServerId::from_proto);
                lsp_store.update(&mut cx, |lsp_store, cx| {
                    let semantic_tokens_task =
                        lsp_store.fetch_semantic_tokens_for_buffer(&buffer, for_server, cx);
                    lsp_store.serve_lsp_query::<SemanticTokensFull>(
                        server_id,
                        sender_id,
                        lsp_request_id,
                        &buffer,
                        buffer_version,
                        cx.background_spawn(async move {
                            semantic_tokens_task.await.unwrap_or_default()
                        }),
                        cx,
                    );
                });
            }
        }
        Ok(proto::Ack {})
    }

    pub(super) async fn handle_lsp_query_response(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::LspQueryResponse>,
        cx: AsyncApp,
    ) -> Result<()> {
        lsp_store.read_with(&cx, |lsp_store, _| {
            if let Some((upstream_client, _)) = lsp_store.upstream_client() {
                upstream_client.handle_lsp_response(envelope.clone());
            }
        });
        Ok(())
    }
}
