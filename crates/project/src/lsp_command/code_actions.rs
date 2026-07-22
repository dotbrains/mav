use super::*;

use lsp::{CodeActionKind, CodeActionOptions};

fn code_action_kind_matches(requested: &lsp::CodeActionKind, actual: &lsp::CodeActionKind) -> bool {
    let requested_str = requested.as_str();
    let actual_str = actual.as_str();

    // Exact match or hierarchical match
    actual_str == requested_str
        || actual_str
            .strip_prefix(requested_str)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

impl LspCommand for GetCodeActions {
    type Response = Vec<CodeAction>;
    type LspRequest = lsp::request::CodeActionRequest;
    type ProtoRequest = proto::GetCodeActions;

    fn display_name(&self) -> &str {
        "Get code actions"
    }

    fn check_capabilities(&self, capabilities: AdapterServerCapabilities) -> bool {
        match &capabilities.server_capabilities.code_action_provider {
            None => false,
            Some(lsp::CodeActionProviderCapability::Simple(false)) => false,
            _ => {
                // If we do know that we want specific code actions AND we know that
                // the server only supports specific code actions, then we want to filter
                // down to the ones that are supported.
                if let Some((requested, supported)) = self
                    .kinds
                    .as_ref()
                    .zip(Self::supported_code_action_kinds(capabilities))
                {
                    requested.iter().any(|requested_kind| {
                        supported.iter().any(|supported_kind| {
                            code_action_kind_matches(requested_kind, supported_kind)
                        })
                    })
                } else {
                    true
                }
            }
        }
    }

    fn to_lsp(
        &self,
        path: &Path,
        buffer: &Buffer,
        language_server: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::CodeActionParams> {
        let mut relevant_diagnostics = Vec::new();
        for entry in buffer
            .snapshot()
            .diagnostics_in_range::<_, language::PointUtf16>(self.range.clone(), false)
        {
            relevant_diagnostics.push(entry.to_lsp_diagnostic_stub()?);
        }

        let only = if let Some(requested) = &self.kinds {
            if let Some(supported_kinds) =
                Self::supported_code_action_kinds(language_server.adapter_server_capabilities())
            {
                let filtered = requested
                    .iter()
                    .filter(|requested_kind| {
                        supported_kinds.iter().any(|supported_kind| {
                            code_action_kind_matches(requested_kind, supported_kind)
                        })
                    })
                    .cloned()
                    .collect();
                Some(filtered)
            } else {
                Some(requested.clone())
            }
        } else {
            None
        };

        Ok(lsp::CodeActionParams {
            text_document: make_text_document_identifier(path)?,
            range: range_to_lsp(self.range.to_point_utf16(buffer))?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: lsp::CodeActionContext {
                diagnostics: relevant_diagnostics,
                only,
                ..lsp::CodeActionContext::default()
            },
        })
    }

    async fn response_from_lsp(
        self,
        actions: Option<lsp::CodeActionResponse>,
        lsp_store: Entity<LspStore>,
        _: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Vec<CodeAction>> {
        let requested_kinds = self.kinds.as_ref();

        let language_server = cx.update(|cx| {
            lsp_store
                .read(cx)
                .language_server_for_id(server_id)
                .with_context(|| {
                    format!("Missing the language server that just returned a response {server_id}")
                })
        })?;

        let server_capabilities = language_server.capabilities();
        let available_commands = server_capabilities
            .execute_command_provider
            .as_ref()
            .map(|options| options.commands.as_slice())
            .unwrap_or_default();
        Ok(actions
            .unwrap_or_default()
            .into_iter()
            .filter_map(|entry| {
                let (lsp_action, resolved) = match entry {
                    lsp::CodeActionOrCommand::CodeAction(lsp_action) => {
                        if let Some(command) = lsp_action.command.as_ref()
                            && !available_commands.contains(&command.command)
                        {
                            return None;
                        }
                        (LspAction::Action(Box::new(lsp_action)), false)
                    }
                    lsp::CodeActionOrCommand::Command(command) => {
                        if available_commands.contains(&command.command) {
                            (LspAction::Command(command), true)
                        } else {
                            return None;
                        }
                    }
                };

                if let Some((kinds, kind)) = requested_kinds.zip(lsp_action.action_kind())
                    && !kinds
                        .iter()
                        .any(|requested_kind| code_action_kind_matches(requested_kind, &kind))
                {
                    return None;
                }

                Some(CodeAction {
                    server_id,
                    range: self.range.clone(),
                    lsp_action,
                    resolved,
                })
            })
            .collect())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetCodeActions {
        proto::GetCodeActions {
            project_id,
            buffer_id: buffer.remote_id().into(),
            start: Some(language::proto::serialize_anchor(&self.range.start)),
            end: Some(language::proto::serialize_anchor(&self.range.end)),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        message: proto::GetCodeActions,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Self> {
        let start = message
            .start
            .and_then(language::proto::deserialize_anchor)
            .context("invalid start")?;
        let end = message
            .end
            .and_then(language::proto::deserialize_anchor)
            .context("invalid end")?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;

        Ok(Self {
            range: start..end,
            kinds: None,
        })
    }

    fn response_to_proto(
        code_actions: Vec<CodeAction>,
        _: &mut LspStore,
        _: PeerId,
        buffer_version: &clock::Global,
        _: &mut App,
    ) -> proto::GetCodeActionsResponse {
        proto::GetCodeActionsResponse {
            actions: code_actions
                .iter()
                .map(LspStore::serialize_code_action)
                .collect(),
            version: serialize_version(buffer_version),
        }
    }

    async fn response_from_proto(
        self,
        message: proto::GetCodeActionsResponse,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        mut cx: AsyncApp,
    ) -> Result<Vec<CodeAction>> {
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&message.version))
            })
            .await?;
        message
            .actions
            .into_iter()
            .map(LspStore::deserialize_code_action)
            .collect()
    }

    fn buffer_id_from_proto(message: &proto::GetCodeActions) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}

impl GetCodeActions {
    fn supported_code_action_kinds(
        capabilities: AdapterServerCapabilities,
    ) -> Option<Vec<CodeActionKind>> {
        match capabilities.server_capabilities.code_action_provider {
            Some(lsp::CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(supported_action_kinds),
                ..
            })) => Some(supported_action_kinds),
            _ => capabilities.code_action_kinds,
        }
    }

    pub fn can_resolve_actions(capabilities: &ServerCapabilities) -> bool {
        capabilities
            .code_action_provider
            .as_ref()
            .and_then(|options| match options {
                lsp::CodeActionProviderCapability::Simple(_is_supported) => None,
                lsp::CodeActionProviderCapability::Options(options) => options.resolve_provider,
            })
            .unwrap_or(false)
    }
}
