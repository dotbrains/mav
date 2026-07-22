use super::*;

impl LspCommand for GetSignatureHelp {
    type Response = Option<SignatureHelp>;
    type LspRequest = lsp::SignatureHelpRequest;
    type ProtoRequest = proto::GetSignatureHelp;

    fn display_name(&self) -> &str {
        "Get signature help"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .signature_help_provider
            .is_some()
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _cx: &App,
    ) -> Result<lsp::SignatureHelpParams> {
        Ok(lsp::SignatureHelpParams {
            text_document_position_params: make_lsp_text_document_position(path, self.position)?,
            context: None,
            work_done_progress_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::SignatureHelp>,
        lsp_store: Entity<LspStore>,
        _: Entity<Buffer>,
        id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Self::Response> {
        let Some(message) = message else {
            return Ok(None);
        };
        Ok(cx.update(|cx| {
            SignatureHelp::new(
                message,
                Some(lsp_store.read(cx).languages.clone()),
                Some(id),
                cx,
            )
        }))
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> Self::ProtoRequest {
        let offset = buffer.point_utf16_to_offset(self.position);
        proto::GetSignatureHelp {
            project_id,
            buffer_id: buffer.remote_id().to_proto(),
            position: Some(serialize_anchor(&buffer.anchor_after(offset))),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        payload: Self::ProtoRequest,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&payload.version))
            })
            .await
            .with_context(|| format!("waiting for version for buffer {}", buffer.entity_id()))?;
        let buffer_snapshot = buffer.read_with(&cx, |buffer, _| buffer.snapshot());
        Ok(Self {
            position: payload
                .position
                .and_then(deserialize_anchor)
                .context("invalid position")?
                .to_point_utf16(&buffer_snapshot),
        })
    }

    fn response_to_proto(
        response: Self::Response,
        _: &mut LspStore,
        _: PeerId,
        _: &Global,
        _: &mut App,
    ) -> proto::GetSignatureHelpResponse {
        proto::GetSignatureHelpResponse {
            signature_help: response
                .map(|signature_help| lsp_to_proto_signature(signature_help.original_data)),
        }
    }

    async fn response_from_proto(
        self,
        response: proto::GetSignatureHelpResponse,
        lsp_store: Entity<LspStore>,
        _: Entity<Buffer>,
        cx: AsyncApp,
    ) -> Result<Self::Response> {
        Ok(cx.update(|cx| {
            response
                .signature_help
                .map(proto_to_lsp_signature)
                .and_then(|signature| {
                    SignatureHelp::new(
                        signature,
                        Some(lsp_store.read(cx).languages.clone()),
                        None,
                        cx,
                    )
                })
        }))
    }

    fn buffer_id_from_proto(message: &Self::ProtoRequest) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
