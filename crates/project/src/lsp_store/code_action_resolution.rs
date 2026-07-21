use super::*;

impl LocalLspStore {
    pub(super) async fn try_resolve_code_action(
        lang_server: &LanguageServer,
        action: &mut CodeAction,
        request_timeout: Duration,
    ) -> anyhow::Result<()> {
        match &mut action.lsp_action {
            LspAction::Action(lsp_action) => {
                if !action.resolved
                    && GetCodeActions::can_resolve_actions(&lang_server.capabilities())
                    && lsp_action.data.is_some()
                    && (lsp_action.command.is_none() || lsp_action.edit.is_none())
                {
                    **lsp_action = lang_server
                        .request::<lsp::request::CodeActionResolveRequest>(
                            *lsp_action.clone(),
                            request_timeout,
                        )
                        .await
                        .into_response()?;
                }
            }
            LspAction::CodeLens(lens) => {
                if !action.resolved && GetCodeLens::can_resolve_lens(&lang_server.capabilities()) {
                    *lens = lang_server
                        .request::<lsp::request::CodeLensResolve>(lens.clone(), request_timeout)
                        .await
                        .into_response()?;
                }
            }
            LspAction::Command(_) => {}
        }

        action.resolved = true;
        anyhow::Ok(())
    }
}

impl LspStore {
    pub fn apply_code_action(
        &self,
        buffer_handle: Entity<Buffer>,
        mut action: CodeAction,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectTransaction>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = proto::ApplyCodeAction {
                project_id,
                buffer_id: buffer_handle.read(cx).remote_id().into(),
                action: Some(Self::serialize_code_action(&action)),
            };
            let buffer_store = self.buffer_store();
            cx.spawn(async move |_, cx| {
                let response = upstream_client
                    .request(request)
                    .await?
                    .transaction
                    .context("missing transaction")?;

                buffer_store
                    .update(cx, |buffer_store, cx| {
                        buffer_store.deserialize_project_transaction(response, push_to_history, cx)
                    })
                    .await
            })
        } else if self.mode.is_local() {
            let Some((_, lang_server, request_timeout)) = buffer_handle.update(cx, |buffer, cx| {
                let request_timeout = ProjectSettings::get_global(cx)
                    .global_lsp_settings
                    .get_request_timeout();
                self.language_server_for_local_buffer(buffer, action.server_id, cx)
                    .map(|(adapter, server)| (adapter.clone(), server.clone(), request_timeout))
            }) else {
                return Task::ready(Ok(ProjectTransaction::default()));
            };

            cx.spawn(async move |this, cx| {
                LocalLspStore::try_resolve_code_action(&lang_server, &mut action, request_timeout)
                    .await
                    .context("resolving a code action")?;
                if let Some(edit) = action.lsp_action.edit()
                    && (edit.changes.is_some() || edit.document_changes.is_some())
                {
                    return LocalLspStore::deserialize_workspace_edit(
                        this.upgrade().context("no app present")?,
                        edit.clone(),
                        push_to_history,
                        lang_server.clone(),
                        cx,
                    )
                    .await;
                }

                let Some(command) = action.lsp_action.command() else {
                    return Ok(ProjectTransaction::default());
                };

                let server_capabilities = lang_server.capabilities();
                let available_commands = server_capabilities
                    .execute_command_provider
                    .as_ref()
                    .map(|options| options.commands.as_slice())
                    .unwrap_or_default();

                if !available_commands.contains(&command.command) {
                    log::debug!(
                        "Skipping executeCommand for {}, not listed in language server capabilities",
                        command.command
                    );
                    return Ok(ProjectTransaction::default());
                }

                let request_timeout = cx.update(|app| {
                    ProjectSettings::get_global(app)
                        .global_lsp_settings
                        .get_request_timeout()
                });

                this.update(cx, |this, _| {
                    this.as_local_mut()
                        .unwrap()
                        .last_workspace_edits_by_language_server
                        .remove(&lang_server.server_id());
                })?;

                let _result = lang_server
                    .request::<lsp::request::ExecuteCommand>(
                        lsp::ExecuteCommandParams {
                            command: command.command.clone(),
                            arguments: command.arguments.clone().unwrap_or_default(),
                            ..lsp::ExecuteCommandParams::default()
                        },
                        request_timeout,
                    )
                    .await
                    .into_response()
                    .context("execute command")?;

                return this.update(cx, |this, _| {
                    this.as_local_mut()
                        .unwrap()
                        .last_workspace_edits_by_language_server
                        .remove(&lang_server.server_id())
                        .unwrap_or_default()
                });
            })
        } else {
            Task::ready(Err(anyhow!("no upstream client and not local")))
        }
    }

    pub fn resolve_code_action(
        &self,
        buffer: &Entity<Buffer>,
        mut action: CodeAction,
        cx: &mut Context<Self>,
    ) -> Task<Result<CodeAction>> {
        if action.resolved {
            return Task::ready(Ok(action));
        }
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = proto::ResolveCodeAction {
                project_id,
                buffer_id: buffer.read(cx).remote_id().into(),
                action: Some(Self::serialize_code_action(&action)),
            };
            cx.background_spawn(async move {
                let response = upstream_client
                    .request(request)
                    .await
                    .context("resolve code action proto request")?;
                let action = response.action.context("missing resolved action")?;
                Self::deserialize_code_action(action)
            })
        } else if self.mode.is_local() {
            let server_id = action.server_id;
            let Some(lang_server) = buffer.update(cx, |buffer, cx| {
                self.language_server_for_local_buffer(buffer, server_id, cx)
                    .map(|(_, server)| server.clone())
            }) else {
                return Task::ready(Ok(action));
            };
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            cx.background_spawn(async move {
                LocalLspStore::try_resolve_code_action(&lang_server, &mut action, request_timeout)
                    .await
                    .context("resolving a code action")?;
                Ok(action)
            })
        } else {
            Task::ready(Err(anyhow!("no upstream client and not local")))
        }
    }

    pub(super) async fn handle_resolve_code_action(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::ResolveCodeAction>,
        mut cx: AsyncApp,
    ) -> Result<proto::ResolveCodeActionResponse> {
        let action =
            Self::deserialize_code_action(envelope.payload.action.context("invalid action")?)?;
        let buffer = lsp_store.update(&mut cx, |lsp_store, cx| {
            let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
            lsp_store.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        let resolved = lsp_store
            .update(&mut cx, |lsp_store, cx| {
                lsp_store.resolve_code_action(&buffer, action, cx)
            })
            .await
            .context("resolving code action")?;
        Ok(proto::ResolveCodeActionResponse {
            action: Some(Self::serialize_code_action(&resolved)),
        })
    }

    pub fn apply_code_action_kind(
        &mut self,
        buffers: HashSet<Entity<Buffer>>,
        kind: CodeActionKind,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<ProjectTransaction>> {
        if self.as_local().is_some() {
            cx.spawn(async move |lsp_store, cx| {
                let buffers = buffers.into_iter().collect::<Vec<_>>();
                let result = LocalLspStore::execute_code_action_kind_locally(
                    lsp_store.clone(),
                    buffers,
                    kind,
                    push_to_history,
                    cx,
                )
                .await;
                lsp_store.update(cx, |lsp_store, _| {
                    lsp_store.update_last_formatting_failure(&result);
                })?;
                result
            })
        } else if let Some((client, project_id)) = self.upstream_client() {
            let buffer_store = self.buffer_store();
            cx.spawn(async move |lsp_store, cx| {
                let result = client
                    .request(proto::ApplyCodeActionKind {
                        project_id,
                        kind: kind.as_str().to_owned(),
                        buffer_ids: buffers
                            .iter()
                            .map(|buffer| {
                                buffer.read_with(cx, |buffer, _| buffer.remote_id().into())
                            })
                            .collect(),
                    })
                    .await
                    .and_then(|result| result.transaction.context("missing transaction"));
                lsp_store.update(cx, |lsp_store, _| {
                    lsp_store.update_last_formatting_failure(&result);
                })?;

                let transaction_response = result?;
                buffer_store
                    .update(cx, |buffer_store, cx| {
                        buffer_store.deserialize_project_transaction(
                            transaction_response,
                            push_to_history,
                            cx,
                        )
                    })
                    .await
            })
        } else {
            Task::ready(Ok(ProjectTransaction::default()))
        }
    }
}
