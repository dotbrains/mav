use super::*;

impl ToolCallEventStream {
    /// Returns a future that resolves when the user cancels the tool call.
    /// Tools should select on this alongside their main work to detect user cancellation.
    pub fn cancelled_by_user(&self) -> impl std::future::Future<Output = ()> + '_ {
        let mut rx = self.cancellation_rx.clone();
        async move {
            loop {
                if *rx.borrow() {
                    return;
                }
                if rx.changed().await.is_err() {
                    // Sender dropped, will never be cancelled
                    std::future::pending::<()>().await;
                }
            }
        }
    }

    /// Returns true if the user has cancelled this tool call.
    /// This is useful for checking cancellation state after an operation completes,
    /// to determine if the completion was due to user cancellation.
    pub fn was_cancelled_by_user(&self) -> bool {
        *self.cancellation_rx.clone().borrow()
    }

    pub fn tool_use_id(&self) -> &LanguageModelToolUseId {
        &self.tool_use_id
    }

    pub fn update_fields(&self, fields: acp::ToolCallUpdateFields) {
        self.stream
            .update_tool_call_fields(&self.tool_use_id, fields, None);
    }

    pub fn update_fields_with_meta(
        &self,
        fields: acp::ToolCallUpdateFields,
        meta: Option<acp::Meta>,
    ) {
        self.stream
            .update_tool_call_fields(&self.tool_use_id, fields, meta);
    }

    pub fn resolve_authorization(&self, outcome: acp_thread::SelectedPermissionOutcome) {
        self.stream
            .resolve_tool_call_authorization(&self.tool_use_id, outcome);
    }

    pub fn update_diff(&self, diff: Entity<acp_thread::Diff>) {
        self.stream
            .0
            .unbounded_send(Ok(ThreadEvent::ToolCallUpdate(
                acp_thread::ToolCallUpdateDiff {
                    id: acp::ToolCallId::new(self.tool_use_id.to_string()),
                    diff,
                }
                .into(),
            )))
            .ok();
    }

    pub fn subagent_spawned(&self, id: acp::SessionId) {
        self.stream
            .0
            .unbounded_send(Ok(ThreadEvent::SubagentSpawned(id)))
            .ok();
    }
}
