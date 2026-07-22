use super::*;

enum PostReviewState {
    AllReviewed,
    Pending,
}

pub struct EditorAgentDiffAddon;

impl editor::Addon for EditorAgentDiffAddon {
    fn to_any(&self) -> &dyn std::any::Any {
        self
    }

    fn extend_key_context(&self, key_context: &mut gpui::KeyContext, _: &App) {
        key_context.add("agent_diff");
        key_context.add("editor_agent_diff");
    }
}
