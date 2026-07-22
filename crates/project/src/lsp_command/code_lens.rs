use super::*;

impl LspCommand for GetCodeLens {
    type Response = Vec<CodeAction>;
    type LspRequest = lsp::CodeLensRequest;
    type ProtoRequest = proto::GetCodeLens;

    fn display_name(&self) -> &str {
        "Code Lens"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        capabilities
            .server_capabilities
            .code_lens_provider
            .is_some()
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::CodeLensParams> {
        Ok(lsp::CodeLensParams {
            text_document: lsp::TextDocumentIdentifier {
                uri: file_path_to_lsp_url(path)?,
            },
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<Vec<lsp::CodeLens>>,
        _lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> anyhow::Result<Vec<CodeAction>> {
        let snapshot = buffer.read_with(&cx, |buffer, _| buffer.snapshot());
        let code_lenses = message.unwrap_or_default();

        Ok(code_lenses
            .into_iter()
            .map(|code_lens| {
                let code_lens_range = range_from_lsp(code_lens.range);
                let start = snapshot.clip_point_utf16(code_lens_range.start, Bias::Left);
                let end = snapshot.clip_point_utf16(code_lens_range.end, Bias::Right);
                let range = snapshot.anchor_before(start)..snapshot.anchor_after(end);
                let resolved = code_lens.command.is_some();
                CodeAction {
                    server_id,
                    range,
                    lsp_action: LspAction::CodeLens(code_lens),
                    resolved,
                }
            })
            .collect())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetCodeLens {
        proto::GetCodeLens {
            project_id,
            buffer_id: buffer.remote_id().into(),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::GetCodeLens,
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
        response: Vec<CodeAction>,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::GetCodeLensResponse {
        proto::GetCodeLensResponse {
            lens_actions: response
                .iter()
                .map(LspStore::serialize_code_action)
                .collect(),
            version: serialize_version(buffer_version),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::GetCodeLensResponse,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> anyhow::Result<Vec<CodeAction>> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        message
            .lens_actions
            .into_iter()
            .map(LspStore::deserialize_code_action)
            .collect::<Result<Vec<_>>>()
            .context("deserializing proto code lens response")
    }

    fn buffer_id_from_proto(message: &proto::GetCodeLens) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
