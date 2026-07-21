use super::*;

impl Thread {
    pub(super) fn accumulate_token_usage(&mut self, update: language_model::TokenUsage) {
        let previous_accounted_usage = self.current_request_token_usage;
        let current_accounted_usage = TokenUsage {
            input_tokens: previous_accounted_usage
                .input_tokens
                .max(update.input_tokens),
            output_tokens: previous_accounted_usage
                .output_tokens
                .max(update.output_tokens),
            cache_creation_input_tokens: previous_accounted_usage
                .cache_creation_input_tokens
                .max(update.cache_creation_input_tokens),
            cache_read_input_tokens: previous_accounted_usage
                .cache_read_input_tokens
                .max(update.cache_read_input_tokens),
        };
        self.current_request_token_usage = current_accounted_usage;
        self.cumulative_token_usage = self.cumulative_token_usage
            + TokenUsage {
                input_tokens: current_accounted_usage
                    .input_tokens
                    .saturating_sub(previous_accounted_usage.input_tokens),
                output_tokens: current_accounted_usage
                    .output_tokens
                    .saturating_sub(previous_accounted_usage.output_tokens),
                cache_creation_input_tokens: current_accounted_usage
                    .cache_creation_input_tokens
                    .saturating_sub(previous_accounted_usage.cache_creation_input_tokens),
                cache_read_input_tokens: current_accounted_usage
                    .cache_read_input_tokens
                    .saturating_sub(previous_accounted_usage.cache_read_input_tokens),
            };
    }

    pub(super) fn update_token_usage(
        &mut self,
        update: language_model::TokenUsage,
        cx: &mut Context<Self>,
    ) {
        self.accumulate_token_usage(update);

        let Some(last_user_message) = self.last_user_message() else {
            return;
        };

        self.request_token_usage
            .insert(last_user_message.id.clone(), update);
        cx.emit(TokenUsageUpdated(self.latest_token_usage()));
        cx.notify();
    }

    pub fn truncate(
        &mut self,
        client_user_message_id: ClientUserMessageId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.cancel(cx).detach();
        // Clear pending message since cancel will try to flush it asynchronously,
        // and we don't want that content to be added after we truncate
        self.pending_message.take();
        let Some(position) = self.messages.iter().position(|msg| {
            matches!(&**msg, Message::User(UserMessage { id, .. }) if id == &client_user_message_id)
        }) else {
            return Err(anyhow!("Message not found"));
        };

        for message in self.messages.drain(position..) {
            match &*message {
                Message::User(message) => {
                    self.request_token_usage.remove(&message.id);
                }
                Message::Agent(_) | Message::Resume | Message::Compaction(_) => {}
            }
        }
        self.clear_summary();
        cx.notify();
        Ok(())
    }

    pub fn latest_request_token_usage(&self) -> Option<language_model::TokenUsage> {
        let last_user_message = self.last_user_message()?;
        let tokens = self.request_token_usage.get(&last_user_message.id)?;
        Some(*tokens)
    }

    pub fn cumulative_token_usage(&self) -> language_model::TokenUsage {
        self.cumulative_token_usage
    }

    pub fn latest_token_usage(&self) -> Option<acp_thread::TokenUsage> {
        let usage = self.latest_request_token_usage()?;
        let model = self.model()?;
        let input_tokens = total_input_tokens(usage);

        Some(acp_thread::TokenUsage {
            max_tokens: model.max_token_count(),
            max_output_tokens: model.max_output_tokens(),
            used_tokens: usage.total_tokens(),
            input_tokens,
            output_tokens: usage.output_tokens,
        })
    }

    /// Get the total input token count as of the message before the given message.
    ///
    /// Returns `None` if:
    /// - `target_id` is the first message (no previous message)
    /// - The previous message hasn't received a response yet (no usage data)
    /// - `target_id` is not found in the messages
    pub fn tokens_before_message(&self, target_id: &ClientUserMessageId) -> Option<u64> {
        let mut previous_user_message_id: Option<&ClientUserMessageId> = None;

        for message in &self.messages {
            if let Message::User(user_msg) = &**message {
                if &user_msg.id == target_id {
                    let prev_id = previous_user_message_id?;
                    let usage = self.request_token_usage.get(prev_id)?;
                    return Some(total_input_tokens(*usage));
                }
                previous_user_message_id = Some(&user_msg.id);
            }
        }
        None
    }
}
