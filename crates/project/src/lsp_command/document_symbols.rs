use super::*;

impl LspCommand for GetDocumentSymbols {
    type Response = Vec<DocumentSymbol>;
    type LspRequest = lsp::request::DocumentSymbolRequest;
    type ProtoRequest = proto::GetDocumentSymbols;

    fn display_name(&self) -> &str {
        "Get document symbols"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .document_symbol_provider
            .is_some_and(|capability| match capability {
                OneOf::Left(supported) => supported,
                OneOf::Right(_options) => true,
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::DocumentSymbolParams> {
        Ok(lsp::DocumentSymbolParams {
            text_document: make_text_document_identifier(path)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        lsp_symbols: Option<lsp::DocumentSymbolResponse>,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: LanguageServerId,
        _: AsyncApp,
    ) -> Result<Vec<DocumentSymbol>> {
        let Some(lsp_symbols) = lsp_symbols else {
            return Ok(Vec::new());
        };

        let symbols = match lsp_symbols {
            lsp::DocumentSymbolResponse::Flat(symbol_information) => symbol_information
                .into_iter()
                .map(|lsp_symbol| DocumentSymbol {
                    name: lsp_symbol.name,
                    kind: lsp_symbol.kind,
                    range: range_from_lsp(lsp_symbol.location.range),
                    selection_range: range_from_lsp(lsp_symbol.location.range),
                    children: Vec::new(),
                })
                .collect(),
            lsp::DocumentSymbolResponse::Nested(nested_responses) => {
                fn convert_symbol(lsp_symbol: lsp::DocumentSymbol) -> DocumentSymbol {
                    DocumentSymbol {
                        name: lsp_symbol.name,
                        kind: lsp_symbol.kind,
                        range: range_from_lsp(lsp_symbol.range),
                        selection_range: range_from_lsp(lsp_symbol.selection_range),
                        children: lsp_symbol
                            .children
                            .map(|children| {
                                children.into_iter().map(convert_symbol).collect::<Vec<_>>()
                            })
                            .unwrap_or_default(),
                    }
                }
                nested_responses.into_iter().map(convert_symbol).collect()
            }
        };
        Ok(symbols)
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetDocumentSymbols {
        proto::GetDocumentSymbols {
            project_id,
            buffer_id: buffer.remote_id().into(),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::GetDocumentSymbols,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        Ok(Self)
    }

    fn response_to_proto(
        response: Vec<DocumentSymbol>,
        _: &mut LspStore,
        _: PeerId,
        _: &clock::Global,
        _: &mut App,
    ) -> proto::GetDocumentSymbolsResponse {
        let symbols = response
            .into_iter()
            .map(|symbol| {
                fn convert_symbol_to_proto(symbol: DocumentSymbol) -> proto::DocumentSymbol {
                    proto::DocumentSymbol {
                        name: symbol.name.clone(),
                        kind: unsafe { mem::transmute::<lsp::SymbolKind, i32>(symbol.kind) },
                        start: Some(proto::PointUtf16 {
                            row: symbol.range.start.0.row,
                            column: symbol.range.start.0.column,
                        }),
                        end: Some(proto::PointUtf16 {
                            row: symbol.range.end.0.row,
                            column: symbol.range.end.0.column,
                        }),
                        selection_start: Some(proto::PointUtf16 {
                            row: symbol.selection_range.start.0.row,
                            column: symbol.selection_range.start.0.column,
                        }),
                        selection_end: Some(proto::PointUtf16 {
                            row: symbol.selection_range.end.0.row,
                            column: symbol.selection_range.end.0.column,
                        }),
                        children: symbol
                            .children
                            .into_iter()
                            .map(convert_symbol_to_proto)
                            .collect(),
                    }
                }
                convert_symbol_to_proto(symbol)
            })
            .collect::<Vec<_>>();

        proto::GetDocumentSymbolsResponse { symbols }
    }

    async fn response_from_proto(
        self,
        message: proto::GetDocumentSymbolsResponse,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> Result<Vec<DocumentSymbol>> {
        let mut symbols = Vec::with_capacity(message.symbols.len());
        for serialized_symbol in message.symbols {
            fn deserialize_symbol_with_children(
                serialized_symbol: proto::DocumentSymbol,
            ) -> Result<DocumentSymbol> {
                let kind =
                    unsafe { mem::transmute::<i32, lsp::SymbolKind>(serialized_symbol.kind) };

                let start = serialized_symbol.start.context("invalid start")?;
                let end = serialized_symbol.end.context("invalid end")?;

                let selection_start = serialized_symbol
                    .selection_start
                    .context("invalid selection start")?;
                let selection_end = serialized_symbol
                    .selection_end
                    .context("invalid selection end")?;

                Ok(DocumentSymbol {
                    name: serialized_symbol.name,
                    kind,
                    range: Unclipped(PointUtf16::new(start.row, start.column))
                        ..Unclipped(PointUtf16::new(end.row, end.column)),
                    selection_range: Unclipped(PointUtf16::new(
                        selection_start.row,
                        selection_start.column,
                    ))
                        ..Unclipped(PointUtf16::new(selection_end.row, selection_end.column)),
                    children: serialized_symbol
                        .children
                        .into_iter()
                        .filter_map(|symbol| deserialize_symbol_with_children(symbol).ok())
                        .collect::<Vec<_>>(),
                })
            }

            symbols.push(deserialize_symbol_with_children(serialized_symbol)?);
        }

        Ok(symbols)
    }

    fn buffer_id_from_proto(message: &proto::GetDocumentSymbols) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
