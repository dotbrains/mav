use super::*;

pub(super) struct PromptEditorCompletionProviderDelegate;

pub(super) fn inline_assistant_model_supports_images(cx: &App) -> bool {
    LanguageModelRegistry::read_global(cx)
        .inline_assistant_model()
        .map_or(false, |m| m.model.supports_images())
}

impl PromptCompletionProviderDelegate for PromptEditorCompletionProviderDelegate {
    fn supported_modes(&self, _cx: &App) -> Vec<PromptContextType> {
        vec![
            PromptContextType::File,
            PromptContextType::Symbol,
            PromptContextType::Thread,
            PromptContextType::Fetch,
            PromptContextType::Skill,
        ]
    }

    fn supports_images(&self, cx: &App) -> bool {
        inline_assistant_model_supports_images(cx)
    }

    fn available_commands(&self, _cx: &App) -> Vec<crate::completion_provider::AvailableCommand> {
        Vec::new()
    }

    fn confirm_command(&self, _cx: &mut App) {}
}
