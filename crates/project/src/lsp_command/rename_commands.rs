use super::*;

#[async_trait(?Send)]
impl LspCommand for PrepareRename {
    type Response = PrepareRenameResponse;
    type LspRequest = lsp::request::PrepareRenameRequest;
    type ProtoRequest = proto::PrepareRename;

    fn display_name(&self) -> &str {
        "Prepare rename"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .rename_provider
            .is_some_and(|capability| match capability {
                OneOf::Left(enabled) => enabled,
                OneOf::Right(options) => options.prepare_provider.unwrap_or(false),
            })
    }

    fn to_lsp_params_or_response(
        &self,
        path: &Path,
        buffer: &Buffer,
        language_server: &Arc<LanguageServer>,
        cx: &App,
    ) -> Result<LspParamsOrResponse<lsp::TextDocumentPositionParams, PrepareRenameResponse>> {
        let rename_provider = language_server
            .adapter_server_capabilities()
            .server_capabilities
            .rename_provider;
        match rename_provider {
            Some(lsp::OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                ..
            })) => Ok(LspParamsOrResponse::Params(self.to_lsp(
                path,
                buffer,
                language_server,
                cx,
            )?)),
            Some(lsp::OneOf::Right(_)) => Ok(LspParamsOrResponse::Response(
                PrepareRenameResponse::OnlyUnpreparedRenameSupported,
            )),
            Some(lsp::OneOf::Left(true)) => Ok(LspParamsOrResponse::Response(
                PrepareRenameResponse::OnlyUnpreparedRenameSupported,
            )),
            _ => anyhow::bail!("Rename not supported"),
        }
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::TextDocumentPositionParams> {
        make_lsp_text_document_position(path, self.position)
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::PrepareRenameResponse>,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        _: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<PrepareRenameResponse> {
        buffer.read_with(&cx, |buffer, _| match message {
            Some(lsp::PrepareRenameResponse::Range(range))
            | Some(lsp::PrepareRenameResponse::RangeWithPlaceholder { range, .. }) => {
                let Range { start, end } = range_from_lsp(range);
                if buffer.clip_point_utf16(start, Bias::Left) == start.0
                    && buffer.clip_point_utf16(end, Bias::Left) == end.0
                {
                    Ok(PrepareRenameResponse::Success(
                        buffer.anchor_after(start)..buffer.anchor_before(end),
                    ))
                } else {
                    Ok(PrepareRenameResponse::InvalidPosition)
                }
            }
            Some(lsp::PrepareRenameResponse::DefaultBehavior { .. }) => {
                let snapshot = buffer.snapshot();
                let (range, _) = snapshot.surrounding_word(self.position, None);
                let range = snapshot.anchor_after(range.start)..snapshot.anchor_before(range.end);
                Ok(PrepareRenameResponse::Success(range))
            }
            None => Ok(PrepareRenameResponse::InvalidPosition),
        })
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::PrepareRename {
        proto::PrepareRename {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::PrepareRename,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let position = message
            .position
            .and_then(deserialize_anchor)
            .context("invalid position")?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;

        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
        })
    }

    fn response_to_proto(
        response: PrepareRenameResponse,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::PrepareRenameResponse {
        match response {
            PrepareRenameResponse::Success(range) => proto::PrepareRenameResponse {
                can_rename: true,
                only_unprepared_rename_supported: false,
                start: Some(language::proto::serialize_anchor(&range.start)),
                end: Some(language::proto::serialize_anchor(&range.end)),
                version: serialize_version(buffer_version),
            },
            PrepareRenameResponse::OnlyUnpreparedRenameSupported => proto::PrepareRenameResponse {
                can_rename: false,
                only_unprepared_rename_supported: true,
                start: None,
                end: None,
                version: vec![],
            },
            PrepareRenameResponse::InvalidPosition => proto::PrepareRenameResponse {
                can_rename: false,
                only_unprepared_rename_supported: false,
                start: None,
                end: None,
                version: vec![],
            },
        }
    }

    async fn response_from_proto(
        self,
        message: proto::PrepareRenameResponse,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<PrepareRenameResponse> {
        if message.can_rename {
            buffer
                .update(&mut cx, |buffer, _| {
                    buffer.wait_for_version(deserialize_version(&message.version))
                })
                .await?;
            if let (Some(start), Some(end)) = (
                message.start.and_then(deserialize_anchor),
                message.end.and_then(deserialize_anchor),
            ) {
                Ok(PrepareRenameResponse::Success(start..end))
            } else {
                anyhow::bail!(
                    "Missing start or end position in remote project PrepareRenameResponse"
                );
            }
        } else if message.only_unprepared_rename_supported {
            Ok(PrepareRenameResponse::OnlyUnpreparedRenameSupported)
        } else {
            Ok(PrepareRenameResponse::InvalidPosition)
        }
    }

    fn buffer_id_from_proto(message: &proto::PrepareRename) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

#[async_trait(?Send)]
impl LspCommand for PerformRename {
    type Response = ProjectTransaction;
    type LspRequest = lsp::request::Rename;
    type ProtoRequest = proto::PerformRename;

    fn display_name(&self) -> &str {
        "Rename"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .rename_provider
            .is_some_and(|capability| match capability {
                OneOf::Left(enabled) => enabled,
                OneOf::Right(_) => true,
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::RenameParams> {
        Ok(lsp::RenameParams {
            text_document_position: make_lsp_text_document_position(path, self.position)?,
            new_name: self.new_name.clone(),
            work_done_progress_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::WorkspaceEdit>,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        mut cx: AsyncApp,
    ) -> Result<ProjectTransaction> {
        if let Some(edit) = message {
            let (_, lsp_server) =
                language_server_for_buffer(&lsp_store, &buffer, server_id, &mut cx)?;
            LocalLspStore::deserialize_workspace_edit(
                lsp_store,
                edit,
                self.push_to_history,
                lsp_server,
                &mut cx,
            )
            .await
        } else {
            Ok(ProjectTransaction::default())
        }
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::PerformRename {
        proto::PerformRename {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
            new_name: self.new_name.clone(),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::PerformRename,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let position = message
            .position
            .and_then(deserialize_anchor)
            .context("invalid position")?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
            new_name: message.new_name,
            push_to_history: false,
        })
    }

    fn response_to_proto(
        response: ProjectTransaction,
        lsp_store: &mut LspStore,
        peer_id: PeerId,
        _: &clock::Global,
        cx: &mut App,
    ) -> proto::PerformRenameResponse {
        let transaction = lsp_store.buffer_store().update(cx, |buffer_store, cx| {
            buffer_store.serialize_project_transaction_for_peer(response, peer_id, cx)
        });
        proto::PerformRenameResponse {
            transaction: Some(transaction),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::PerformRenameResponse,
        lsp_store: Entity<LspStore>,
        _: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<ProjectTransaction> {
        let message = message.transaction.context("missing transaction")?;
        lsp_store
            .update(&mut cx, |lsp_store, cx| {
                lsp_store.buffer_store().update(cx, |buffer_store, cx| {
                    buffer_store.deserialize_project_transaction(message, self.push_to_history, cx)
                })
            })
            .await
    }

    fn buffer_id_from_proto(message: &proto::PerformRename) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
