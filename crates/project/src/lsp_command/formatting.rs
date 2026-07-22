use super::*;

impl OnTypeFormatting {
    pub fn supports_on_type_formatting(trigger: &str, capabilities: &ServerCapabilities) -> bool {
        let Some(on_type_formatting_options) = &capabilities.document_on_type_formatting_provider
        else {
            return false;
        };
        on_type_formatting_options
            .first_trigger_character
            .contains(trigger)
            || on_type_formatting_options
                .more_trigger_character
                .iter()
                .flatten()
                .any(|chars| chars.contains(trigger))
    }
}

#[async_trait(?Send)]
impl LspCommand for OnTypeFormatting {
    type Response = Option<Transaction>;
    type LspRequest = lsp::request::OnTypeFormatting;
    type ProtoRequest = proto::OnTypeFormatting;

    fn display_name(&self) -> &str {
        "Formatting on typing"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        Self::supports_on_type_formatting(&self.trigger, &capabilities.server_capabilities)
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::DocumentOnTypeFormattingParams> {
        Ok(lsp::DocumentOnTypeFormattingParams {
            text_document_position: make_lsp_text_document_position(path, self.position)?,
            ch: self.trigger.clone(),
            options: self.options.clone(),
        })
    }

    async fn response_from_lsp(
        self,
        message: Option<Vec<lsp::TextEdit>>,
        lsp_store: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        mut cx: AsyncApp,
    ) -> Result<Option<Transaction>> {
        if let Some(edits) = message {
            let (lsp_adapter, lsp_server) =
                language_server_for_buffer(&lsp_store, &buffer, server_id, &mut cx)?;
            LocalLspStore::deserialize_text_edits(
                lsp_store,
                buffer,
                edits,
                self.push_to_history,
                lsp_adapter,
                lsp_server,
                &mut cx,
            )
            .await
        } else {
            Ok(None)
        }
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::OnTypeFormatting {
        proto::OnTypeFormatting {
            project_id,
            buffer_id: buffer.remote_id().into(),
            position: Some(language::proto::serialize_anchor(
                &buffer.anchor_before(self.position),
            )),
            trigger: self.trigger.clone(),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::OnTypeFormatting,
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

        let options = buffer.update(&mut cx, |buffer, cx| {
            lsp_formatting_options(LanguageSettings::for_buffer(buffer, cx).as_ref())
        });

        Ok(Self {
            position: buffer.read_with(&cx, |buffer, _| position.to_point_utf16(buffer)),
            trigger: message.trigger.clone(),
            options,
            push_to_history: false,
        })
    }

    fn response_to_proto(
        response: Option<Transaction>,
        _: &mut LspStore,
        _: PeerId,
        _: &clock::Global,
        _: &mut App,
    ) -> proto::OnTypeFormattingResponse {
        proto::OnTypeFormattingResponse {
            transaction: response
                .map(|transaction| language::proto::serialize_transaction(&transaction)),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::OnTypeFormattingResponse,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> Result<Option<Transaction>> {
        let Some(transaction) = message.transaction else {
            return Ok(None);
        };
        Ok(Some(language::proto::deserialize_transaction(transaction)?))
    }

    fn buffer_id_from_proto(message: &proto::OnTypeFormatting) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
