use super::*;

impl Thread {
    pub fn last_message(&self) -> Option<&Message> {
        self.messages.last().map(std::ops::Deref::deref)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn last_received_or_pending_message(&self) -> Option<Arc<Message>> {
        if let Some(message) = self.pending_message.clone() {
            Some(Arc::new(Message::Agent(message)))
        } else {
            self.messages.last().cloned()
        }
    }

    pub(super) fn last_user_message(&self) -> Option<&UserMessage> {
        self.messages
            .iter()
            .rev()
            .find_map(|message| match &**message {
                Message::User(user_message) => Some(user_message),
                Message::Agent(_) | Message::Resume | Message::Compaction(_) => None,
            })
    }

    pub(super) fn pending_message(&mut self) -> &mut AgentMessage {
        self.pending_message.get_or_insert_default()
    }

    pub(super) fn flush_pending_message(&mut self, cx: &mut Context<Self>) {
        let Some(mut message) = self.pending_message.take() else {
            return;
        };

        if message.content.is_empty() {
            return;
        }

        for content in &message.content {
            let AgentMessageContent::ToolUse(tool_use) = content else {
                continue;
            };

            if !message.tool_results.contains_key(&tool_use.id) {
                message.tool_results.insert(
                    tool_use.id.clone(),
                    LanguageModelToolResult {
                        tool_use_id: tool_use.id.clone(),
                        tool_name: tool_use.name.clone(),
                        is_error: true,
                        content: vec![LanguageModelToolResultContent::Text(
                            TOOL_CANCELED_MESSAGE.into(),
                        )],
                        output: None,
                    },
                );
            }
        }

        self.messages.push(Arc::new(Message::Agent(message)));
        self.updated_at = Utc::now();
        self.clear_summary();
        cx.notify()
    }
}
