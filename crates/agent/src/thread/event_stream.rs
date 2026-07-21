use super::*;

#[derive(Clone)]
pub(super) struct ThreadEventStream(pub(super) mpsc::UnboundedSender<Result<ThreadEvent>>);

impl ThreadEventStream {
    pub(super) fn send_user_message(&self, message: &UserMessage) {
        self.0
            .unbounded_send(Ok(ThreadEvent::UserMessage(message.clone())))
            .ok();
    }

    pub(super) fn send_text(&self, text: &str) {
        self.0
            .unbounded_send(Ok(ThreadEvent::AgentText(text.to_string())))
            .ok();
    }

    pub(super) fn send_thinking(&self, text: &str) {
        self.0
            .unbounded_send(Ok(ThreadEvent::AgentThinking(text.to_string())))
            .ok();
    }

    pub(super) fn send_tool_call(
        &self,
        id: &LanguageModelToolUseId,
        tool_name: &str,
        title: SharedString,
        kind: acp::ToolKind,
        input: serde_json::Value,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ToolCall(Self::initial_tool_call(
                id,
                tool_name,
                title.to_string(),
                kind,
                input,
            ))))
            .ok();
    }

    pub(super) fn initial_tool_call(
        id: &LanguageModelToolUseId,
        tool_name: &str,
        title: String,
        kind: acp::ToolKind,
        input: serde_json::Value,
    ) -> acp::ToolCall {
        acp::ToolCall::new(id.to_string(), title)
            .kind(kind)
            .raw_input(input)
            .meta(acp_thread::meta_with_tool_name(tool_name))
    }

    pub(super) fn update_tool_call_fields(
        &self,
        tool_use_id: &LanguageModelToolUseId,
        fields: acp::ToolCallUpdateFields,
        meta: Option<acp::Meta>,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ToolCallUpdate(
                acp::ToolCallUpdate::new(tool_use_id.to_string(), fields)
                    .meta(meta)
                    .into(),
            )))
            .ok();
    }

    pub(super) fn resolve_tool_call_authorization(
        &self,
        tool_use_id: &LanguageModelToolUseId,
        outcome: acp_thread::SelectedPermissionOutcome,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ToolCallAuthorizationResolved {
                tool_call_id: acp::ToolCallId::new(tool_use_id.to_string()),
                outcome,
            }))
            .ok();
    }

    pub(super) fn send_retry(&self, status: acp_thread::RetryStatus) {
        self.0.unbounded_send(Ok(ThreadEvent::Retry(status))).ok();
    }

    pub(super) fn send_context_compaction(
        &self,
        id: acp_thread::ContextCompactionId,
        status: acp_thread::ContextCompactionStatus,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ContextCompaction(
                acp_thread::ContextCompaction {
                    id,
                    status,
                    summary: None,
                },
            )))
            .ok();
    }

    pub(super) fn send_context_compaction_update(
        &self,
        id: acp_thread::ContextCompactionId,
        summary_delta: &str,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ContextCompactionUpdate(
                acp_thread::ContextCompactionUpdate {
                    id,
                    summary_delta: summary_delta.to_string(),
                    status: None,
                },
            )))
            .ok();
    }

    pub(super) fn update_context_compaction_status(
        &self,
        id: acp_thread::ContextCompactionId,
        status: acp_thread::ContextCompactionStatus,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ContextCompactionUpdate(
                acp_thread::ContextCompactionUpdate {
                    id,
                    summary_delta: String::new(),
                    status: Some(status),
                },
            )))
            .ok();
    }

    pub(super) fn send_stop(&self, reason: acp::StopReason) {
        self.0.unbounded_send(Ok(ThreadEvent::Stop(reason))).ok();
    }

    pub(super) fn send_canceled(&self) {
        self.0
            .unbounded_send(Ok(ThreadEvent::Stop(acp::StopReason::Cancelled)))
            .ok();
    }

    pub(super) fn send_error(&self, error: impl Into<anyhow::Error>) {
        self.0.unbounded_send(Err(error.into())).ok();
    }
}
