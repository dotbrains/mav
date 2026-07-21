use super::*;

#[derive(Default)]
pub struct SessionCapabilities {
    prompt_capabilities: acp::PromptCapabilities,
    available_commands: Vec<acp::AvailableCommand>,
    available_skills: Vec<AvailableSkill>,
}

impl SessionCapabilities {
    pub fn new(
        prompt_capabilities: acp::PromptCapabilities,
        available_commands: Vec<acp::AvailableCommand>,
        available_skills: Vec<AvailableSkill>,
    ) -> Self {
        Self {
            prompt_capabilities,
            available_commands,
            available_skills,
        }
    }

    pub fn from_acp_commands(
        prompt_capabilities: acp::PromptCapabilities,
        available_commands: Vec<acp::AvailableCommand>,
    ) -> Self {
        Self::new(prompt_capabilities, available_commands, Vec::new())
    }

    pub fn supports_images(&self) -> bool {
        self.prompt_capabilities.image
    }

    pub fn supports_embedded_context(&self) -> bool {
        self.prompt_capabilities.embedded_context
    }

    pub fn available_commands(&self) -> &[acp::AvailableCommand] {
        &self.available_commands
    }

    pub fn available_skills(&self) -> &[AvailableSkill] {
        &self.available_skills
    }

    pub fn has_slash_completions(&self) -> bool {
        !self.available_commands.is_empty() || !self.available_skills.is_empty()
    }

    fn supported_modes(&self, has_thread_store: bool) -> Vec<PromptContextType> {
        let mut supported = vec![PromptContextType::File, PromptContextType::Symbol];
        if self.prompt_capabilities.embedded_context {
            if has_thread_store {
                supported.push(PromptContextType::Thread);
            }
            supported.extend(&[
                PromptContextType::Diagnostics,
                PromptContextType::Fetch,
                PromptContextType::Skill,
                PromptContextType::BranchDiff,
            ]);
        }
        supported
    }

    pub fn completion_commands(&self) -> Vec<AvailableCommand> {
        self.available_commands
            .iter()
            .map(|command| AvailableCommand {
                name: command.name.clone().into(),
                description: command.description.clone().into(),
                requires_argument: command.input.is_some(),
                source: None,
                category: acp_thread::command_category_from_meta(&command.meta),
            })
            .collect()
    }

    pub fn completion_skills(&self) -> Vec<AvailableSkill> {
        self.available_skills.clone()
    }

    pub fn set_prompt_capabilities(&mut self, prompt_capabilities: acp::PromptCapabilities) {
        self.prompt_capabilities = prompt_capabilities;
    }

    pub fn set_available_commands(&mut self, available_commands: Vec<acp::AvailableCommand>) {
        self.available_commands = available_commands;
    }

    pub fn set_available_skills(&mut self, available_skills: Vec<AvailableSkill>) {
        self.available_skills = available_skills;
    }
}

pub type SharedSessionCapabilities = Arc<RwLock<SessionCapabilities>>;

pub(super) struct MessageEditorCompletionDelegate {
    pub(super) session_capabilities: SharedSessionCapabilities,
    pub(super) has_thread_store: bool,
    pub(super) message_editor: WeakEntity<MessageEditor>,
}

impl PromptCompletionProviderDelegate for MessageEditorCompletionDelegate {
    fn supports_images(&self, _cx: &App) -> bool {
        self.session_capabilities.read().supports_images()
    }

    fn supported_modes(&self, _cx: &App) -> Vec<PromptContextType> {
        self.session_capabilities
            .read()
            .supported_modes(self.has_thread_store)
    }

    fn available_commands(&self, _cx: &App) -> Vec<AvailableCommand> {
        self.session_capabilities.read().completion_commands()
    }

    fn available_skills(&self, _cx: &App) -> Vec<AvailableSkill> {
        self.session_capabilities.read().completion_skills()
    }

    fn slash_autocomplete_invoked(&self, cx: &mut App) {
        // This may be called synchronously from inside a `MessageEditor`
        // update (e.g. when pasting a slash command triggers completions),
        // so we defer the emit to avoid a reentrant update panic.
        let Some(editor) = self.message_editor.upgrade() else {
            return;
        };
        cx.defer(move |cx| {
            editor.update(cx, |_editor, cx| {
                cx.emit(MessageEditorEvent::SlashAutocompleteOpened);
            });
        });
    }

    fn confirm_command(&self, cx: &mut App) {
        let _ = self.message_editor.update(cx, |this, cx| this.send(cx));
    }
}
