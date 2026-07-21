use super::*;

impl Thread {
    pub fn replay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> mpsc::UnboundedReceiver<Result<ThreadEvent>> {
        let (tx, rx) = mpsc::unbounded();
        let stream = ThreadEventStream(tx);
        for (message_ix, message) in self.messages.iter().enumerate() {
            match &**message {
                Message::User(user_message) => stream.send_user_message(user_message),
                Message::Agent(assistant_message) => {
                    for content in &assistant_message.content {
                        match content {
                            AgentMessageContent::Text(text) => stream.send_text(text),
                            AgentMessageContent::Thinking { text, .. } => {
                                stream.send_thinking(text)
                            }
                            AgentMessageContent::RedactedThinking(_) => {}
                            AgentMessageContent::ToolUse(tool_use) => {
                                self.replay_tool_call(
                                    tool_use,
                                    assistant_message.tool_results.get(&tool_use.id),
                                    &stream,
                                    cx,
                                );
                            }
                        }
                    }
                }
                Message::Resume => {}
                Message::Compaction(info) => {
                    let compaction_id = acp_thread::ContextCompactionId(
                        format!("replay-compaction-{message_ix}").into(),
                    );
                    match info {
                        CompactionInfo::Summary(summary) => {
                            stream.send_context_compaction(
                                compaction_id.clone(),
                                acp_thread::ContextCompactionStatus::Completed,
                            );
                            stream.send_context_compaction_update(compaction_id.clone(), summary);
                        }
                        CompactionInfo::ProviderNative { .. } => {
                            stream.send_context_compaction(
                                compaction_id,
                                acp_thread::ContextCompactionStatus::Completed,
                            );
                        }
                    }
                }
            }
        }
        rx
    }

    fn replay_tool_call(
        &self,
        tool_use: &LanguageModelToolUse,
        tool_result: Option<&LanguageModelToolResult>,
        stream: &ThreadEventStream,
        cx: &mut Context<Self>,
    ) {
        // A tool call left only with the canceled sentinel produced nothing useful
        // (the sentinel is model-facing only, and is inserted exactly when a tool
        // had no real result). Don't replay it into the UI at all.
        if tool_result.is_some_and(Self::is_canceled_tool_result) {
            return;
        }

        let output = tool_result
            .as_ref()
            .and_then(|result| result.output.clone());
        let replay_content = tool_result.and_then(Self::tool_result_content_for_replay);
        let status = tool_result
            .as_ref()
            .map_or(acp::ToolCallStatus::Failed, |result| {
                if result.is_error {
                    acp::ToolCallStatus::Failed
                } else {
                    acp::ToolCallStatus::Completed
                }
            });

        // Recorded tool calls use the model-facing name, so a terminal call is
        // always keyed as `terminal` and resolves to the non-sandboxed
        // `TerminalTool` here, even if it originally ran under
        // `SandboxedTerminalTool`. That's safe because both variants share the
        // same `replay` behavior; replay only reconstructs UI state and never
        // re-runs the command or re-applies sandbox policy.
        let tool = self.tools.get(tool_use.name.as_ref()).cloned().or_else(|| {
            self.context_server_registry
                .read(cx)
                .servers()
                .find_map(|(_, tools)| {
                    if let Some(tool) = tools.get(tool_use.name.as_ref()) {
                        Some(tool.clone())
                    } else {
                        None
                    }
                })
        });

        let Some(tool) = tool else {
            // Tool not found (e.g., MCP server not connected after restart),
            // but still display the saved result if available.
            // We need to send both ToolCall and ToolCallUpdate events because the UI
            // only converts raw_output to displayable content in update_fields, not from_acp.
            stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCall(
                    acp::ToolCall::new(tool_use.id.to_string(), tool_use.name.to_string())
                        .status(status)
                        .raw_input(tool_use.input.clone()),
                )))
                .ok();
            let mut fields = acp::ToolCallUpdateFields::new()
                .status(status)
                .raw_output(output);
            if let Some(content) = replay_content {
                fields = fields.content(content);
            }
            stream.update_tool_call_fields(&tool_use.id, fields, None);
            return;
        };

        let title = tool.initial_title(tool_use.input.clone(), cx);
        let kind = tool.kind();
        stream.send_tool_call(
            &tool_use.id,
            &tool_use.name,
            title,
            kind,
            tool_use.input.clone(),
        );

        if let Some(content) = replay_content {
            stream.update_tool_call_fields(
                &tool_use.id,
                acp::ToolCallUpdateFields::new().content(content),
                None,
            );
        }

        if let Some(output) = output.clone() {
            // For replay, we use a dummy cancellation receiver since the tool already completed
            let (_cancellation_tx, cancellation_rx) = watch::channel(false);
            let tool_event_stream = ToolCallEventStream::new(
                tool_use.id.clone(),
                stream.clone(),
                Some(self.project.read(cx).fs().clone()),
                cancellation_rx,
                self.sandbox_grants.clone(),
                Some(cx.weak_entity()),
            );
            tool.replay(tool_use.input.clone(), output, tool_event_stream, cx)
                .log_err();
        }

        stream.update_tool_call_fields(
            &tool_use.id,
            acp::ToolCallUpdateFields::new()
                .status(status)
                .raw_output(output),
            None,
        );
    }

    /// A canceled tool result carries only the model-facing `TOOL_CANCELED_MESSAGE`
    /// sentinel (inserted exactly when a tool had no real result). It's never
    /// meaningful to the user, so we detect it to skip replaying the tool call.
    fn is_canceled_tool_result(tool_result: &LanguageModelToolResult) -> bool {
        tool_result.is_error
            && matches!(
                tool_result.content.as_slice(),
                [LanguageModelToolResultContent::Text(text)]
                    if text.as_ref() == TOOL_CANCELED_MESSAGE
            )
    }

    fn tool_result_content_for_replay(
        tool_result: &LanguageModelToolResult,
    ) -> Option<Vec<acp::ToolCallContent>> {
        let has_image = tool_result
            .content
            .iter()
            .any(|part| matches!(part, LanguageModelToolResultContent::Image(_)));
        if !has_image && tool_result.output.is_some() {
            return None;
        }

        let content = tool_result
            .content
            .iter()
            .filter_map(|part| match part {
                LanguageModelToolResultContent::Text(text) => {
                    if text.is_empty() {
                        None
                    } else {
                        Some(acp::ToolCallContent::Content(acp::Content::new(
                            acp::ContentBlock::Text(acp::TextContent::new(text.to_string())),
                        )))
                    }
                }
                LanguageModelToolResultContent::Image(image) => Some(
                    acp::ToolCallContent::Content(acp::Content::new(acp::ContentBlock::Image(
                        acp::ImageContent::new(image.source.clone(), "image/png"),
                    ))),
                ),
            })
            .collect::<Vec<_>>();

        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    }
}
