use super::*;

impl AcpThread {
    pub fn handle_session_update(
        &mut self,
        update: acp::SessionUpdate,
        cx: &mut Context<Self>,
    ) -> Result<(), acp::Error> {
        match update {
            acp::SessionUpdate::UserMessageChunk(acp::ContentChunk {
                content,
                message_id,
                ..
            }) => {
                // We optimistically add the full user prompt before calling `prompt`.
                // Some ACP servers echo user chunks back over updates. Skip echoed
                // chunks only when they match the local optimistic message.
                let already_in_user_message = self
                    .entries
                    .last_mut()
                    .and_then(|entry| match entry {
                        AgentThreadEntry::UserMessage(message) => Some(message),
                        _ => None,
                    })
                    .is_some_and(|message| {
                        let already_in_user_message = message.is_optimistic
                            && message.chunks.contains(&content)
                            && can_merge_message_chunks(
                                message.protocol_id.as_ref(),
                                message_id.as_ref(),
                            );
                        if already_in_user_message && message.protocol_id.is_none() {
                            message.protocol_id = message_id.clone();
                        }
                        already_in_user_message
                    });
                if !already_in_user_message {
                    self.push_user_content_block_from_agent(message_id, content, cx);
                }
            }
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk {
                content,
                message_id,
                ..
            }) => {
                self.push_assistant_content_block_with_message_id(
                    message_id, content, false, false, cx,
                );
            }
            acp::SessionUpdate::AgentThoughtChunk(acp::ContentChunk {
                content,
                message_id,
                ..
            }) => {
                self.push_assistant_content_block_with_message_id(
                    message_id, content, true, false, cx,
                );
            }
            acp::SessionUpdate::ToolCall(tool_call) => {
                self.upsert_tool_call(tool_call, cx)?;
            }
            acp::SessionUpdate::ToolCallUpdate(tool_call_update) => {
                self.update_tool_call(tool_call_update, cx)?;
            }
            acp::SessionUpdate::Plan(plan) => {
                self.update_plan(plan, cx);
            }
            acp::SessionUpdate::SessionInfoUpdate(info_update) => {
                if let MaybeUndefined::Value(title) = info_update.title {
                    let had_provisional = self.provisional_title.take().is_some();
                    let title: SharedString = title.into();
                    if self.title.as_ref() != Some(&title) {
                        self.title = Some(title);
                        cx.emit(AcpThreadEvent::TitleUpdated);
                    } else if had_provisional {
                        cx.emit(AcpThreadEvent::TitleUpdated);
                    }
                }
            }
            acp::SessionUpdate::AvailableCommandsUpdate(acp::AvailableCommandsUpdate {
                available_commands,
                ..
            }) => {
                self.available_commands = available_commands.clone();
                cx.emit(AcpThreadEvent::AvailableCommandsUpdated(available_commands));
            }
            acp::SessionUpdate::CurrentModeUpdate(acp::CurrentModeUpdate {
                current_mode_id,
                ..
            }) => cx.emit(AcpThreadEvent::ModeUpdated(current_mode_id)),
            acp::SessionUpdate::ConfigOptionUpdate(acp::ConfigOptionUpdate {
                config_options,
                ..
            }) => cx.emit(AcpThreadEvent::ConfigOptionsUpdated(config_options)),
            acp::SessionUpdate::UsageUpdate(update) => {
                let usage = self.token_usage.get_or_insert_with(Default::default);
                usage.max_tokens = update.size;
                usage.used_tokens = update.used;
                if let Some(cost) = update.cost {
                    self.cost = Some(SessionCost {
                        amount: cost.amount,
                        currency: cost.currency.into(),
                    });
                }
                cx.emit(AcpThreadEvent::TokenUsageUpdated);
            }
            _ => {}
        }
        Ok(())
    }
}
