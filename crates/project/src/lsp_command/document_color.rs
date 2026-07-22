use super::*;

impl LspCommand for GetDocumentColor {
    type Response = Vec<DocumentColor>;
    type LspRequest = lsp::request::DocumentColor;
    type ProtoRequest = proto::GetDocumentColor;

    fn display_name(&self) -> &str {
        "Document color"
    }

    fn check_capabilities(&self, server_capabilities: AdapterServerCapabilities) -> bool {
        server_capabilities
            .server_capabilities
            .color_provider
            .as_ref()
            .is_some_and(|capability| match capability {
                lsp::ColorProviderCapability::Simple(supported) => *supported,
                lsp::ColorProviderCapability::ColorProvider(..) => true,
                lsp::ColorProviderCapability::Options(..) => true,
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::DocumentColorParams> {
        Ok(lsp::DocumentColorParams {
            text_document: make_text_document_identifier(path)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Vec<lsp::ColorInformation>,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: LanguageServerId,
        _: AsyncApp,
    ) -> Result<Self::Response> {
        Ok(message
            .into_iter()
            .map(|color| DocumentColor {
                lsp_range: color.range,
                color: color.color,
                resolved: false,
                color_presentations: Vec::new(),
            })
            .collect())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> Self::ProtoRequest {
        proto::GetDocumentColor {
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
        Ok(Self {})
    }

    fn response_to_proto(
        response: Self::Response,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::GetDocumentColorResponse {
        proto::GetDocumentColorResponse {
            colors: response
                .into_iter()
                .map(|color| {
                    let start = point_from_lsp(color.lsp_range.start).0;
                    let end = point_from_lsp(color.lsp_range.end).0;
                    proto::ColorInformation {
                        red: color.color.red,
                        green: color.color.green,
                        blue: color.color.blue,
                        alpha: color.color.alpha,
                        lsp_range_start: Some(proto::PointUtf16 {
                            row: start.row,
                            column: start.column,
                        }),
                        lsp_range_end: Some(proto::PointUtf16 {
                            row: end.row,
                            column: end.column,
                        }),
                    }
                })
                .collect(),
            version: serialize_version(buffer_version),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::GetDocumentColorResponse,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> Result<Self::Response> {
        Ok(message
            .colors
            .into_iter()
            .filter_map(|color| {
                let start = color.lsp_range_start?;
                let start = PointUtf16::new(start.row, start.column);
                let end = color.lsp_range_end?;
                let end = PointUtf16::new(end.row, end.column);
                Some(DocumentColor {
                    resolved: false,
                    color_presentations: Vec::new(),
                    lsp_range: lsp::Range {
                        start: point_to_lsp(start),
                        end: point_to_lsp(end),
                    },
                    color: lsp::Color {
                        red: color.red,
                        green: color.green,
                        blue: color.blue,
                        alpha: color.alpha,
                    },
                })
            })
            .collect())
    }

    fn buffer_id_from_proto(message: &Self::ProtoRequest) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
