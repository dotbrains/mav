use super::*;

impl AgentPanel {
    pub(super) fn active_thread_has_messages(&self, cx: &App) -> bool {
        self.active_agent_thread(cx)
            .is_some_and(|thread| !thread.read(cx).entries().is_empty())
    }

    /// Whether the active view is in the **ephemeral** new-draft slot
    pub fn active_view_is_new_draft(&self, cx: &App) -> bool {
        self.draft_thread.as_ref().is_some_and(|draft| {
            draft
                .read(cx)
                .root_thread(cx)
                .is_some_and(|thread| thread.read(cx).is_draft_thread())
                && self
                    .active_conversation_view()
                    .is_some_and(|active| active.entity_id() == draft.entity_id())
        })
    }

    /// Whether the active thread is any kind of draft
    pub fn active_thread_is_draft(&self, cx: &App) -> bool {
        self.active_agent_thread(cx)
            .is_some_and(|thread| thread.read(cx).is_draft_thread())
    }
}
