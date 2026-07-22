use super::*;

impl LspCommand for GetFoldingRanges {
    type Response = Vec<LspFoldingRange>;
    type LspRequest = lsp::request::FoldingRangeRequest;
    type ProtoRequest = proto::GetFoldingRanges;

    fn display_name(&self) -> &str {
        "Folding ranges"
    }

    fn check_capabilities(&self, server_capabilities: AdapterServerCapabilities) -> bool {
        server_capabilities
            .server_capabilities
            .folding_range_provider
            .as_ref()
            .is_some_and(|capability| match capability {
                lsp::FoldingRangeProviderCapability::Simple(supported) => *supported,
                lsp::FoldingRangeProviderCapability::FoldingProvider(..)
                | lsp::FoldingRangeProviderCapability::Options(..) => true,
            })
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::FoldingRangeParams> {
        Ok(lsp::FoldingRangeParams {
            text_document: make_text_document_identifier(path)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<Vec<lsp::FoldingRange>>,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        _: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Self::Response> {
        let snapshot = buffer.read_with(&cx, |buffer, _| buffer.snapshot());
        let max_point = snapshot.max_point_utf16();
        Ok(message
            .unwrap_or_default()
            .into_iter()
            .filter(|range| range.start_line < range.end_line)
            .filter(|range| range.start_line <= max_point.row && range.end_line <= max_point.row)
            .map(|folding_range| {
                let start_col = folding_range.start_character.unwrap_or(u32::MAX);
                let end_col = folding_range.end_character.unwrap_or(u32::MAX);
                let start = snapshot.clip_point_utf16(
                    Unclipped(PointUtf16::new(folding_range.start_line, start_col)),
                    Bias::Right,
                );
                let end = snapshot.clip_point_utf16(
                    Unclipped(PointUtf16::new(folding_range.end_line, end_col)),
                    Bias::Left,
                );
                let start = snapshot.anchor_after(start);
                let end = snapshot.anchor_before(end);
                let collapsed_text = folding_range
                    .collapsed_text
                    .filter(|t| !t.is_empty())
                    .map(|t| SharedString::from(crate::lsp_store::collapse_newlines(&t, " ")));
                LspFoldingRange {
                    range: start..end,
                    collapsed_text,
                }
            })
            .collect())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> Self::ProtoRequest {
        proto::GetFoldingRanges {
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
    ) -> proto::GetFoldingRangesResponse {
        let mut ranges = Vec::with_capacity(response.len());
        let mut collapsed_texts = Vec::with_capacity(response.len());
        for folding_range in response {
            ranges.push(serialize_anchor_range(folding_range.range));
            collapsed_texts.push(
                folding_range
                    .collapsed_text
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
            );
        }
        proto::GetFoldingRangesResponse {
            ranges,
            collapsed_texts,
            version: serialize_version(buffer_version),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::GetFoldingRangesResponse,
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
            .ranges
            .into_iter()
            .zip(
                message
                    .collapsed_texts
                    .into_iter()
                    .map(Some)
                    .chain(std::iter::repeat(None)),
            )
            .map(|(range, collapsed_text)| {
                Ok(LspFoldingRange {
                    range: deserialize_anchor_range(range)?,
                    collapsed_text: collapsed_text
                        .filter(|t| !t.is_empty())
                        .map(SharedString::from),
                })
            })
            .collect()
    }

    fn buffer_id_from_proto(message: &Self::ProtoRequest) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
