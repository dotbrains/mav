use super::*;

impl Thread {
    pub(crate) fn build_completion_request(
        &self,
        completion_intent: CompletionIntent,
        cx: &App,
    ) -> Result<LanguageModelRequest> {
        let completion_intent =
            if self.is_subagent() && completion_intent == CompletionIntent::UserPrompt {
                CompletionIntent::Subagent
            } else {
                completion_intent
            };

        let model = self
            .model()
            .ok_or_else(|| anyhow!(NoModelConfiguredError))?;
        let tools = if let Some(turn) = self.running_turn.as_ref() {
            turn.tools
                .iter()
                .filter_map(|(tool_name, tool)| {
                    log::trace!("Including tool: {}", tool_name);
                    Some(LanguageModelRequestTool {
                        name: tool_name.to_string(),
                        description: tool.description().to_string(),
                        input_schema: tool.input_schema(model.tool_input_format()).log_err()?,
                        use_input_streaming: tool.supports_input_streaming(),
                    })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        log::debug!("Building completion request");
        log::debug!("Completion intent: {:?}", completion_intent);

        let available_tools: Vec<_> = self
            .running_turn
            .as_ref()
            .map(|turn| turn.tools.keys().cloned().collect())
            .unwrap_or_default();

        log::debug!("Request includes {} tools", available_tools.len());
        let messages = self.build_request_messages(available_tools, cx);
        log::debug!("Request will include {} messages", messages.len());

        let request = LanguageModelRequest {
            thread_id: Some(self.id.to_string()),
            prompt_id: Some(self.prompt_id.to_string()),
            intent: Some(completion_intent),
            messages,
            tools,
            tool_choice: None,
            stop: Vec::new(),
            temperature: AgentSettings::temperature_for_model(model, cx),
            // Models that can't run with thinking disabled ignore the
            // toggle state, which may be stale from a previously selected
            // model that could.
            thinking_allowed: self.thinking_enabled || !model.supports_disabling_thinking(),
            thinking_effort: self.thinking_effort.clone(),
            speed: self.speed(),
            compact_at_tokens: None,
        };

        log::debug!("Completion request built successfully");
        Ok(request)
    }
}
