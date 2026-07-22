use super::*;

impl LspCommand for GetDocumentLinks {
    type Response = Vec<LspDocumentLink>;
    type LspRequest = lsp::request::DocumentLinkRequest;
    type ProtoRequest = proto::GetDocumentLinks;

    fn display_name(&self) -> &str {
        "Document links"
    }

    fn check_capabilities(&self, server_capabilities: AdapterServerCapabilities) -> bool {
        server_capabilities
            .server_capabilities
            .document_link_provider
            .is_some()
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::DocumentLinkParams> {
        Ok(lsp::DocumentLinkParams {
            text_document: make_text_document_identifier(path)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<Vec<lsp::DocumentLink>>,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        _: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Self::Response> {
        let snapshot = buffer.read_with(&cx, |buffer, _| buffer.snapshot());
        Ok(message
            .unwrap_or_default()
            .into_iter()
            .map(|link| {
                let start = snapshot.clip_point_utf16(
                    Unclipped(PointUtf16::new(
                        link.range.start.line,
                        link.range.start.character,
                    )),
                    Bias::Left,
                );
                let end = snapshot.clip_point_utf16(
                    Unclipped(PointUtf16::new(
                        link.range.end.line,
                        link.range.end.character,
                    )),
                    Bias::Right,
                );
                LspDocumentLink {
                    range: snapshot.anchor_after(start)..snapshot.anchor_before(end),
                    target: link.target.map(|url| url.to_string().into()),
                    tooltip: link.tooltip.map(SharedString::from),
                    data: link.data,
                    resolved: false,
                }
            })
            .collect())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> Self::ProtoRequest {
        proto::GetDocumentLinks {
            project_id,
            buffer_id: buffer.remote_id().to_proto(),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        _: Self::ProtoRequest,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> Result<Self> {
        Ok(Self)
    }

    fn response_to_proto(
        response: Self::Response,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::GetDocumentLinksResponse {
        proto::GetDocumentLinksResponse {
            links: response
                .into_iter()
                .map(|link| proto::DocumentLinkProto {
                    range: Some(serialize_anchor_range(link.range)),
                    target: link.target.map(String::from),
                    tooltip: link.tooltip.map(String::from),
                    data: link
                        .data
                        .map(|d| serde_json::to_string(&d).unwrap_or_default()),
                })
                .collect(),
            version: serialize_version(buffer_version),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::GetDocumentLinksResponse,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self::Response> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        message
            .links
            .into_iter()
            .map(|link| {
                Ok(LspDocumentLink {
                    range: deserialize_anchor_range(link.range.context("missing range")?)?,
                    target: link.target.map(SharedString::from),
                    tooltip: link.tooltip.map(SharedString::from),
                    data: link.data.and_then(|d| serde_json::from_str(&d).ok()),
                    resolved: false,
                })
            })
            .collect()
    }

    fn buffer_id_from_proto(message: &Self::ProtoRequest) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
