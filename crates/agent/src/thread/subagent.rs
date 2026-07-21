use super::*;

impl Thread {
    pub(crate) fn register_running_subagent(&mut self, subagent: WeakEntity<Thread>) {
        self.running_subagents.push(subagent);
    }

    pub(crate) fn unregister_running_subagent(
        &mut self,
        subagent_session_id: &acp::SessionId,
        cx: &App,
    ) {
        self.running_subagents.retain(|s| {
            s.upgrade()
                .map_or(false, |s| s.read(cx).id() != subagent_session_id)
        });
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn running_subagent_ids(&self, cx: &App) -> Vec<acp::SessionId> {
        self.running_subagents
            .iter()
            .filter_map(|s| s.upgrade().map(|s| s.read(cx).id().clone()))
            .collect()
    }

    pub fn is_subagent(&self) -> bool {
        self.subagent_context.is_some()
    }

    pub fn parent_thread_id(&self) -> Option<acp::SessionId> {
        self.subagent_context
            .as_ref()
            .map(|c| c.parent_thread_id.clone())
    }

    pub fn depth(&self) -> u8 {
        self.subagent_context.as_ref().map(|c| c.depth).unwrap_or(0)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_subagent_context(&mut self, context: SubagentContext) {
        self.subagent_context = Some(context);
    }

    pub fn is_turn_complete(&self) -> bool {
        self.running_turn.is_none()
    }

    pub fn to_markdown(&self) -> String {
        let mut markdown = messages_to_markdown(&self.messages);

        if let Some(message) = self.pending_message.as_ref() {
            markdown.push_str("\n## Assistant\n\n");
            markdown.push_str(&message.to_markdown());
        }

        markdown
    }
}
