use super::*;

impl Thread {
    pub(super) fn build_request_messages(
        &self,
        available_tools: Vec<SharedString>,
        cx: &App,
    ) -> Vec<LanguageModelRequestMessage> {
        let mut messages =
            self.build_request_messages_until(available_tools, self.messages.len(), cx);

        if let Some(message) = self.pending_message.as_ref() {
            messages.extend(message.to_request());
        }

        messages
    }

    pub(super) fn build_request_messages_until(
        &self,
        available_tools: Vec<SharedString>,
        end_ix: usize,
        cx: &App,
    ) -> Vec<LanguageModelRequestMessage> {
        let end_ix = end_ix.min(self.messages.len());
        log::trace!("Building request messages from {} thread messages", end_ix);

        let user_agents_md = UserAgentsMd::global(cx).and_then(|s| s.content().cloned());
        let system_prompt = SystemPromptTemplate {
            project: self.project_context.read(cx),
            available_tools,
            model_name: self.model().map(|m| m.name().0.to_string()),
            date: Local::now().format("%Y-%m-%d").to_string(),
            user_agents_md,
            sandboxing: crate::sandboxing::sandboxing_enabled_for_project(
                self.project.read(cx),
                cx,
            ),
            is_linux: cfg!(target_os = "linux"),
            is_windows: cfg!(target_os = "windows"),
        }
        .render(&self.templates)
        .context("failed to build system prompt")
        .expect("Invalid template");
        let mut messages = vec![LanguageModelRequestMessage {
            role: Role::System,
            content: vec![system_prompt.into()],
            cache: false,
            reasoning_details: None,
        }];
        self.extend_request_history_until(&mut messages, end_ix);

        if let Some(last_message) = messages.last_mut() {
            last_message.cache = true;
        }

        messages
    }

    pub(super) fn extend_request_history_until(
        &self,
        request_messages: &mut Vec<LanguageModelRequestMessage>,
        end_ix: usize,
    ) {
        extend_request_history_until(&self.messages, request_messages, end_ix);
    }

    /// Captures the data for an `"Agent Compaction Completed"` telemetry event
    /// at the moment a compaction starts. Returns `None` if there's no model.
    pub(super) fn build_compaction_telemetry(
        &self,
        trigger: &'static str,
        cx: &App,
    ) -> Option<CompactionTelemetry> {
        let model = self.model()?;
        let auto_compact = AgentSettings::get_global(cx).auto_compact;
        let max_tokens = model.max_token_count();
        let max_input_tokens = max_tokens.saturating_sub(model.max_output_tokens().unwrap_or(0));
        let tokens_before = self
            .latest_request_token_usage()
            .map(|usage| total_input_tokens(usage).saturating_add(usage.output_tokens));
        Some(CompactionTelemetry {
            trigger,
            thread_id: self.id.to_string(),
            parent_thread_id: self.parent_thread_id().map(|id| id.to_string()),
            prompt_id: self.prompt_id.to_string(),
            model: model.telemetry_id(),
            model_provider: model.provider_id().to_string(),
            thinking_effort: self.thinking_effort.clone(),
            max_tokens,
            tokens_before,
            auto_compact_enabled: auto_compact.enabled,
            auto_compact_threshold: auto_compact.threshold.to_string(),
            auto_compact_threshold_tokens: auto_compact_threshold_token_count(
                auto_compact.threshold,
                max_input_tokens,
            ),
            retries: 0,
        })
    }

    /// Emits a pending compaction telemetry event for a non-success outcome
    /// (`"failed"` or `"canceled"`), with no post-compaction token count. A
    /// no-op if no compaction telemetry is pending.
    pub(super) fn emit_compaction_telemetry_outcome(
        &mut self,
        status: &'static str,
        error: Option<String>,
    ) {
        if let Some(telemetry) = self.pending_compaction_telemetry.take() {
            telemetry.emit(status, error, None);
        }
    }

    pub(super) fn compaction_message_target_ix(&self, cx: &App) -> Option<usize> {
        let auto_compact = AgentSettings::get_global(cx).auto_compact;
        if !auto_compact.enabled {
            return None;
        }

        let model = self.model()?;
        let max_token_count = model.max_token_count();
        let max_input_tokens =
            max_token_count.saturating_sub(model.max_output_tokens().unwrap_or(0));
        // Models with a small context window don't leave enough headroom for a
        // compaction pass; the UI warns the user about the token limit instead.
        if max_input_tokens < MIN_COMPACTION_CONTEXT_WINDOW {
            return None;
        }
        let (usage_ix, usage) = {
            let this = &self;
            this.messages
                .iter()
                .enumerate()
                .rev()
                .find_map(|(ix, message)| {
                    let Message::User(user_message) = &**message else {
                        return None;
                    };
                    this.request_token_usage
                        .get(&user_message.id)
                        .copied()
                        .map(|usage| (ix, usage))
                })
        }?;
        if latest_compaction_message_ix_before(&self.messages, self.messages.len())
            .is_some_and(|compaction_ix| compaction_ix > usage_ix)
        {
            return None;
        }

        let active_tokens = total_input_tokens(usage).saturating_add(usage.output_tokens);
        let compaction_threshold =
            auto_compact_threshold_token_count(auto_compact.threshold, max_input_tokens);
        if active_tokens < compaction_threshold {
            return None;
        }

        let insertion_ix = match self.messages.last() {
            Some(message)
                if matches!(
                    &**message,
                    Message::User(UserMessage { id, .. }) if !self.request_token_usage.contains_key(id)
                ) =>
            {
                self.messages.len().saturating_sub(1)
            }
            _ => self.messages.len(),
        };
        Some(insertion_ix)
    }

    /// Insertion point for a manually-triggered compaction.
    /// Returns `None` only when there is nothing to summarize (no messages, or the thread already ends in a compaction).
    pub(super) fn forced_compaction_target_ix(&self) -> Option<usize> {
        if matches!(
            self.messages.last().map(|message| &**message),
            None | Some(Message::Compaction(_))
        ) {
            return None;
        }
        Some(self.messages.len())
    }

    pub(super) fn build_compaction_request(
        &self,
        insertion_ix: usize,
        model: &Arc<dyn LanguageModel>,
        cx: &App,
    ) -> LanguageModelRequest {
        let mut request = LanguageModelRequest {
            thread_id: Some(self.id.to_string()),
            prompt_id: Some(self.prompt_id.to_string()),
            intent: Some(CompletionIntent::ThreadContextSummarization),
            temperature: AgentSettings::temperature_for_model(model, cx),
            messages: self.build_request_messages_until(Vec::new(), insertion_ix, cx),
            ..Default::default()
        };

        request.messages.push(LanguageModelRequestMessage {
            role: Role::User,
            content: vec![COMPACTION_PROMPT.into()],
            cache: false,
            reasoning_details: None,
        });

        request
    }
}

pub(super) fn total_input_tokens(usage: language_model::TokenUsage) -> u64 {
    usage
        .input_tokens
        .saturating_add(usage.cache_creation_input_tokens)
        .saturating_add(usage.cache_read_input_tokens)
}

fn auto_compact_threshold_token_count(
    threshold: AutoCompactThreshold,
    max_token_count: u64,
) -> u64 {
    match threshold {
        AutoCompactThreshold::Percentage(percent) => {
            ((max_token_count as f64) * percent).ceil() as u64
        }
        AutoCompactThreshold::TokensUsed(tokens) => tokens,
        AutoCompactThreshold::TokensRemaining(tokens) => {
            max_token_count.saturating_sub(tokens).saturating_add(1)
        }
    }
}

/// Snapshot of the data needed to report an `"Agent Compaction Completed"`
/// telemetry event, captured when a compaction starts.
pub(super) struct CompactionTelemetry {
    /// `"auto"` for threshold-triggered compaction, `"manual"` for `/compact`.
    trigger: &'static str,
    thread_id: String,
    parent_thread_id: Option<String>,
    prompt_id: String,
    model: String,
    model_provider: String,
    thinking_effort: Option<String>,
    max_tokens: u64,
    /// Tokens in the context window immediately before compaction.
    tokens_before: Option<u64>,
    auto_compact_enabled: bool,
    auto_compact_threshold: String,
    auto_compact_threshold_tokens: u64,
    /// Number of times the compaction request was retried before the final
    /// outcome.
    pub(super) retries: u32,
}

impl CompactionTelemetry {
    pub(super) fn emit(
        self,
        status: &'static str,
        error: Option<String>,
        tokens_after: Option<u64>,
    ) {
        telemetry::event!(
            "Agent Compaction Completed",
            trigger = self.trigger,
            status = status,
            error = error,
            thread_id = self.thread_id,
            parent_thread_id = self.parent_thread_id,
            prompt_id = self.prompt_id,
            model = self.model,
            model_provider = self.model_provider,
            thinking_effort = self.thinking_effort,
            max_tokens = self.max_tokens,
            tokens_before = self.tokens_before,
            tokens_after = tokens_after,
            auto_compact_enabled = self.auto_compact_enabled,
            auto_compact_threshold = self.auto_compact_threshold,
            auto_compact_threshold_tokens = self.auto_compact_threshold_tokens,
            retries = self.retries,
        );
    }
}
