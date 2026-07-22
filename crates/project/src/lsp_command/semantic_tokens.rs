use super::*;

impl LspCommand for SemanticTokensFull {
    type Response = SemanticTokensResponse;
    type LspRequest = lsp::SemanticTokensFullRequest;
    type ProtoRequest = proto::SemanticTokens;

    fn display_name(&self) -> &str {
        "Semantic tokens full"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .semantic_tokens_provider
            .as_ref()
            .is_some_and(|semantic_tokens_provider| {
                let options = match semantic_tokens_provider {
                    lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(opts) => opts,
                    lsp::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        opts,
                    ) => &opts.semantic_tokens_options,
                };

                match options.full {
                    Some(lsp::SemanticTokensFullOptions::Bool(is_supported)) => is_supported,
                    Some(lsp::SemanticTokensFullOptions::Delta { .. }) => true,
                    None => false,
                }
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::SemanticTokensParams> {
        Ok(lsp::SemanticTokensParams {
            text_document: lsp::TextDocumentIdentifier {
                uri: file_path_to_lsp_url(path)?,
            },
            partial_result_params: Default::default(),
            work_done_progress_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::SemanticTokensResult>,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: LanguageServerId,
        _: AsyncApp,
    ) -> anyhow::Result<SemanticTokensResponse> {
        match message {
            Some(lsp::SemanticTokensResult::Tokens(tokens)) => Ok(SemanticTokensResponse::Full {
                data: tokens.data,
                result_id: tokens.result_id.map(SharedString::new),
            }),
            Some(lsp::SemanticTokensResult::Partial(_)) => {
                anyhow::bail!(
                    "Unexpected semantic tokens response with partial result for inlay hints"
                )
            }
            None => Ok(Default::default()),
        }
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::SemanticTokens {
        proto::SemanticTokens {
            project_id,
            buffer_id: buffer.remote_id().into(),
            version: serialize_version(&buffer.version()),
            for_server: self.for_server.map(|id| id.to_proto()),
        }
    }

    async fn from_proto(
        message: proto::SemanticTokens,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;

        Ok(Self {
            for_server: message
                .for_server
                .map(|id| LanguageServerId::from_proto(id)),
        })
    }

    fn response_to_proto(
        response: SemanticTokensResponse,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::SemanticTokensResponse {
        match response {
            SemanticTokensResponse::Full { data, result_id } => proto::SemanticTokensResponse {
                data,
                edits: Vec::new(),
                result_id: result_id.map(|s| s.to_string()),
                version: serialize_version(buffer_version),
            },
            SemanticTokensResponse::Delta { edits, result_id } => proto::SemanticTokensResponse {
                data: Vec::new(),
                edits: edits
                    .into_iter()
                    .map(|edit| proto::SemanticTokensEdit {
                        start: edit.start,
                        delete_count: edit.delete_count,
                        data: edit.data,
                    })
                    .collect(),
                result_id: result_id.map(|s| s.to_string()),
                version: serialize_version(buffer_version),
            },
        }
    }

    async fn response_from_proto(
        self,
        message: proto::SemanticTokensResponse,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> anyhow::Result<SemanticTokensResponse> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;

        Ok(SemanticTokensResponse::Full {
            data: message.data,
            result_id: message.result_id.map(SharedString::new),
        })
    }

    fn buffer_id_from_proto(message: &proto::SemanticTokens) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

impl LspCommand for SemanticTokensDelta {
    type Response = SemanticTokensResponse;
    type LspRequest = lsp::SemanticTokensFullDeltaRequest;
    type ProtoRequest = proto::SemanticTokens;

    fn display_name(&self) -> &str {
        "Semantic tokens delta"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .semantic_tokens_provider
            .as_ref()
            .is_some_and(|semantic_tokens_provider| {
                let options = match semantic_tokens_provider {
                    lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(opts) => opts,
                    lsp::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        opts,
                    ) => &opts.semantic_tokens_options,
                };

                match options.full {
                    Some(lsp::SemanticTokensFullOptions::Delta { delta }) => delta.unwrap_or(false),
                    // `full: true` (instead of `full: { delta: true }`) means no support for delta.
                    _ => false,
                }
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::SemanticTokensDeltaParams> {
        Ok(lsp::SemanticTokensDeltaParams {
            text_document: lsp::TextDocumentIdentifier {
                uri: file_path_to_lsp_url(path)?,
            },
            previous_result_id: self.previous_result_id.clone().map(|s| s.to_string()),
            partial_result_params: Default::default(),
            work_done_progress_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::SemanticTokensFullDeltaResult>,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: LanguageServerId,
        _: AsyncApp,
    ) -> anyhow::Result<SemanticTokensResponse> {
        match message {
            Some(lsp::SemanticTokensFullDeltaResult::Tokens(tokens)) => {
                Ok(SemanticTokensResponse::Full {
                    data: tokens.data,
                    result_id: tokens.result_id.map(SharedString::new),
                })
            }
            Some(lsp::SemanticTokensFullDeltaResult::TokensDelta(delta)) => {
                Ok(SemanticTokensResponse::Delta {
                    edits: delta
                        .edits
                        .into_iter()
                        .map(|e| SemanticTokensEdit {
                            start: e.start,
                            delete_count: e.delete_count,
                            data: e.data.unwrap_or_default(),
                        })
                        .collect(),
                    result_id: delta.result_id.map(SharedString::new),
                })
            }
            Some(lsp::SemanticTokensFullDeltaResult::PartialTokensDelta { .. }) => {
                anyhow::bail!(
                    "Unexpected semantic tokens response with partial result for inlay hints"
                )
            }
            None => Ok(Default::default()),
        }
    }

    fn to_proto(&self, _: u64, _: &Buffer) -> proto::SemanticTokens {
        unimplemented!("Delta requests are never initialted on the remote client side")
    }

    async fn from_proto(
        _: proto::SemanticTokens,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> Result<Self> {
        unimplemented!("Delta requests are never initialted on the remote client side")
    }

    fn response_to_proto(
        response: SemanticTokensResponse,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::SemanticTokensResponse {
        match response {
            SemanticTokensResponse::Full { data, result_id } => proto::SemanticTokensResponse {
                data,
                edits: Vec::new(),
                result_id: result_id.map(|s| s.to_string()),
                version: serialize_version(buffer_version),
            },
            SemanticTokensResponse::Delta { edits, result_id } => proto::SemanticTokensResponse {
                data: Vec::new(),
                edits: edits
                    .into_iter()
                    .map(|edit| proto::SemanticTokensEdit {
                        start: edit.start,
                        delete_count: edit.delete_count,
                        data: edit.data,
                    })
                    .collect(),
                result_id: result_id.map(|s| s.to_string()),
                version: serialize_version(buffer_version),
            },
        }
    }

    async fn response_from_proto(
        self,
        message: proto::SemanticTokensResponse,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> anyhow::Result<SemanticTokensResponse> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;

        Ok(SemanticTokensResponse::Full {
            data: message.data,
            result_id: message.result_id.map(SharedString::new),
        })
    }

    fn buffer_id_from_proto(message: &proto::SemanticTokens) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
