use super::*;

impl LinkedEditingRange {
    pub fn check_server_capabilities(capabilities: ServerCapabilities) -> bool {
        let Some(linked_editing_options) = capabilities.linked_editing_range_provider else {
            return false;
        };
        if let LinkedEditingRangeServerCapabilities::Simple(false) = linked_editing_options {
            return false;
        }
        true
    }
}

#[async_trait(?Send)]
impl LspCommand for LinkedEditingRange {
    type Response = Vec<Range<Anchor>>;
    type LspRequest = lsp::request::LinkedEditingRange;
    type ProtoRequest = proto::LinkedEditingRange;

    fn display_name(&self) -> &str {
        "Linked editing range"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        Self::check_server_capabilities(capabilities.server_capabilities)
    }

    fn to_lsp(
        &self,
        path: &Path,
        buffer: &Buffer,
        _server: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::LinkedEditingRangeParams> {
        let position = self.position.to_point_utf16(&buffer.snapshot());
        Ok(lsp::LinkedEditingRangeParams {
            text_document_position_params: make_lsp_text_document_position(path, position)?,
            work_done_progress_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<lsp::LinkedEditingRanges>,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        _server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Vec<Range<Anchor>>> {
        if let Some(lsp::LinkedEditingRanges { mut ranges, .. }) = message {
            ranges.sort_by_key(|range| range.start);

            Ok(buffer.read_with(&cx, |buffer, _| {
                ranges
                    .into_iter()
                    .map(|range| {
                        let start =
                            buffer.clip_point_utf16(point_from_lsp(range.start), Bias::Left);
                        let end = buffer.clip_point_utf16(point_from_lsp(range.end), Bias::Left);
                        buffer.anchor_before(start)..buffer.anchor_after(end)
                    })
                    .collect()
            }))
        } else {
            Ok(vec![])
        }
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::LinkedEditingRange {
        proto::LinkedEditingRange {
            project_id,
            buffer_id: buffer.remote_id().to_proto(),
            position: Some(serialize_anchor(&self.position)),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::LinkedEditingRange,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let position = message.position.context("invalid position")?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        let position = deserialize_anchor(position).context("invalid position")?;
        buffer
            .update(&mut cx, |buffer, _| buffer.wait_for_anchors([position]))
            .await?;
        Ok(Self { position })
    }

    fn response_to_proto(
        response: Vec<Range<Anchor>>,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::LinkedEditingRangeResponse {
        proto::LinkedEditingRangeResponse {
            items: response
                .into_iter()
                .map(|range| proto::AnchorRange {
                    start: Some(serialize_anchor(&range.start)),
                    end: Some(serialize_anchor(&range.end)),
                })
                .collect(),
            version: serialize_version(buffer_version),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::LinkedEditingRangeResponse,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Vec<Range<Anchor>>> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        let items: Vec<Range<Anchor>> = message
            .items
            .into_iter()
            .filter_map(|range| {
                let start = deserialize_anchor(range.start?)?;
                let end = deserialize_anchor(range.end?)?;
                Some(start..end)
            })
            .collect();
        for range in &items {
            buffer
                .update(&mut cx, |buffer, _| {
                    buffer.wait_for_anchors([range.start, range.end])
                })
                .await?;
        }
        Ok(items)
    }

    fn buffer_id_from_proto(message: &proto::LinkedEditingRange) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
