use super::*;

impl NativeAgent {
    pub(super) fn send_mcp_prompt(
        &self,
        client_user_message_id: ClientUserMessageId,
        session_id: acp::SessionId,
        prompt_name: String,
        server_id: ContextServerId,
        arguments: HashMap<String, String>,
        original_content: Vec<acp::ContentBlock>,
        cx: &mut Context<Self>,
    ) -> Task<Result<acp::PromptResponse>> {
        let Some(state) = self.session_project_state(&session_id) else {
            return Task::ready(Err(anyhow!("Project state not found for session")));
        };
        let server_store = state
            .context_server_registry
            .read(cx)
            .server_store()
            .clone();
        let path_style = state.project.read(cx).path_style(cx);

        cx.spawn(async move |this, cx| {
            let prompt =
                crate::get_prompt(&server_store, &server_id, &prompt_name, arguments, cx).await?;

            let (acp_thread, thread) = this.update(cx, |this, _cx| {
                let session = this
                    .sessions
                    .get(&session_id)
                    .context("Failed to get session")?;
                anyhow::Ok((session.acp_thread.clone(), session.thread.clone()))
            })??;

            let mut last_is_user = true;

            thread.update(cx, |thread, cx| {
                thread.push_acp_user_block(
                    client_user_message_id,
                    original_content.into_iter().skip(1),
                    path_style,
                    cx,
                );
            });

            for message in prompt.messages {
                let context_server::types::PromptMessage { role, content } = message;
                let block = mcp_message_content_to_acp_content_block(content);

                match role {
                    context_server::types::Role::User => {
                        let id = acp_thread::ClientUserMessageId::new();

                        acp_thread.update(cx, |acp_thread, cx| {
                            acp_thread.push_user_content_block_with_indent(
                                Some(id.clone()),
                                block.clone(),
                                true,
                                cx,
                            );
                        });

                        thread.update(cx, |thread, cx| {
                            thread.push_acp_user_block(id, [block], path_style, cx);
                        });
                    }
                    context_server::types::Role::Assistant => {
                        acp_thread.update(cx, |acp_thread, cx| {
                            acp_thread.push_assistant_content_block_with_indent(
                                block.clone(),
                                false,
                                true,
                                cx,
                            );
                        });

                        thread.update(cx, |thread, cx| {
                            thread.push_acp_agent_block(block, cx);
                        });
                    }
                }

                last_is_user = role == context_server::types::Role::User;
            }

            let response_stream = thread.update(cx, |thread, cx| {
                if last_is_user {
                    thread.send_existing(cx)
                } else {
                    // Resume if MCP prompt did not end with a user message
                    thread.resume(cx)
                }
            })?;

            let connection = this.upgrade().map(NativeAgentConnection);
            cx.update(|cx| {
                NativeAgentConnection::handle_thread_events(
                    response_stream,
                    acp_thread.downgrade(),
                    connection,
                    cx,
                )
            })
            .await
        })
    }

    /// Run a summary-based context compaction in response to the built-in
    /// `/compact` slash command.
    pub(super) fn send_compact_command(
        &self,
        client_user_message_id: ClientUserMessageId,
        session_id: acp::SessionId,
        cx: &mut Context<Self>,
    ) -> Task<Result<acp::PromptResponse>> {
        cx.spawn(async move |this, cx| {
            let (acp_thread, thread) = this.update(cx, |this, _cx| {
                let session = this
                    .sessions
                    .get(&session_id)
                    .context("Failed to get session")?;
                anyhow::Ok((session.acp_thread.clone(), session.thread.clone()))
            })??;

            let response_stream =
                thread.update(cx, |thread, cx| thread.compact(client_user_message_id, cx))?;
            acp_thread.update(cx, |acp_thread, cx| {
                acp_thread.update_token_usage(None, cx);
            });

            let connection = this.upgrade().map(NativeAgentConnection);
            cx.update(|cx| {
                NativeAgentConnection::handle_thread_events(
                    response_stream,
                    acp_thread.downgrade(),
                    connection,
                    cx,
                )
            })
            .await
        })
    }

    /// Activate a skill in response to a `/skill-name` slash command. The
    /// skill body is wrapped in the same `<skill_content>` envelope the
    /// model-driven `skill` tool uses, so the conversation looks the same
    /// regardless of who initiated the load. Any text the user typed after
    /// the command on the same line — plus any additional content blocks
    /// they attached (file mentions, etc.) — is appended to the same user
    /// message after the skill envelope, so the model sees the skill
    /// instructions followed by the user's request.
    pub(super) fn send_skill_invocation(
        &self,
        client_user_message_id: ClientUserMessageId,
        session_id: acp::SessionId,
        skill: Skill,
        original_content: Vec<acp::ContentBlock>,
        cx: &mut Context<Self>,
    ) -> Task<Result<acp::PromptResponse>> {
        let Some(state) = self.session_project_state(&session_id) else {
            return Task::ready(Err(anyhow!("Project state not found for session")));
        };
        let path_style = state.project.read(cx).path_style(cx);
        let read_skill_body =
            skill_body_resolver_for_project(state.project.clone(), self.fs.clone());

        cx.spawn(async move |this, cx| {
            let (acp_thread, thread) = this.update(cx, |this, _cx| {
                let session = this
                    .sessions
                    .get(&session_id)
                    .context("Failed to get session")?;
                anyhow::Ok((session.acp_thread.clone(), session.thread.clone()))
            })??;

            // Build the model-context message: skill envelope first, then
            // anything the user wrote after the slash command. The first
            // text block has its leading `/cmd` stripped so the literal
            // command name isn't echoed into the model's context, but any
            // text the user typed after it on the same line is preserved
            // verbatim and appended after the envelope.
            //
            // Read the body on demand here — bodies live on disk between
            // materializations to keep memory cost O(total frontmatter)
            // rather than O(total file size).
            let body = if let Some(embedded) = skill.embedded_body {
                embedded.to_string()
            } else {
                read_skill_body(skill.clone(), cx).await.with_context(|| {
                    format!(
                        "Failed to read skill body from {}",
                        skill.skill_file_path.display()
                    )
                })?
            };
            let envelope = crate::tools::render_skill_envelope(&skill, &body);
            let envelope_block = acp::ContentBlock::Text(acp::TextContent::new(envelope));

            let mut user_blocks = original_content;
            if let Some(acp::ContentBlock::Text(text_content)) = user_blocks.first_mut() {
                let stripped = strip_slash_command_prefix(&text_content.text);
                if stripped.trim().is_empty() {
                    user_blocks.remove(0);
                } else {
                    text_content.text = stripped;
                }
            }

            // UI: show the rendered envelope as a sibling user message so
            // the user can see what context was loaded for the skill. The
            // user's own typed message is already rendered by the normal
            // prompt flow, so we don't push it to the UI again here.
            let injected_id = acp_thread::ClientUserMessageId::new();
            acp_thread.update(cx, |acp_thread, cx| {
                acp_thread.push_user_content_block_with_indent(
                    Some(injected_id),
                    envelope_block.clone(),
                    true,
                    cx,
                );
            });

            // Model context: a single user message containing the skill
            // envelope followed by the user's appended content.
            let mut combined = Vec::with_capacity(user_blocks.len() + 1);
            combined.push(envelope_block);
            combined.extend(user_blocks);

            thread.update(cx, |thread, cx| {
                thread.push_acp_user_block(client_user_message_id, combined, path_style, cx);
            });

            let response_stream = thread.update(cx, |thread, cx| thread.send_existing(cx))?;

            let connection = this.upgrade().map(NativeAgentConnection);
            cx.update(|cx| {
                NativeAgentConnection::handle_thread_events(
                    response_stream,
                    acp_thread.downgrade(),
                    connection,
                    cx,
                )
            })
            .await
        })
    }
}
