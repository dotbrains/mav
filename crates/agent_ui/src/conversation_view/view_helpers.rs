use super::*;

impl ConversationView {
    pub(crate) fn as_native_connection(
        &self,
        cx: &App,
    ) -> Option<Rc<agent::NativeAgentConnection>> {
        self.root_thread(cx)?
            .read(cx)
            .connection()
            .clone()
            .downcast()
    }

    pub fn as_native_thread(&self, cx: &App) -> Option<Entity<agent::Thread>> {
        self.as_native_connection(cx)?
            .thread(self.root_session_id.as_ref()?, cx)
    }

    pub(super) fn render_markdown(
        &self,
        markdown: Entity<Markdown>,
        style: MarkdownStyle,
        cx: &App,
    ) -> MarkdownElement {
        render_agent_markdown(
            markdown,
            style,
            &self.workspace,
            &self.code_span_resolver,
            cx,
        )
    }

    pub(super) fn current_model_name(&self, cx: &App) -> SharedString {
        if self.as_native_connection(cx).is_some() {
            self.root_thread_view()
                .and_then(|active| active.read(cx).model_selector.clone())
                .and_then(|selector| selector.read(cx).active_model(cx))
                .map(|model| model.name.clone())
                .unwrap_or_else(|| SharedString::from("The model"))
        } else {
            self.agent.agent_id().0
        }
    }

    pub(super) fn create_copy_button(&self, message: impl Into<String>) -> impl IntoElement {
        let message = message.into();

        CopyButton::new("copy-error-message", message).tooltip_label("Copy Error Message")
    }
}

pub(in crate::conversation_view) fn loading_contents_spinner(size: IconSize) -> AnyElement {
    Icon::new(IconName::LoadCircle)
        .size(size)
        .color(Color::Accent)
        .with_rotate_animation(3)
        .into_any_element()
}

pub(in crate::conversation_view) fn native_available_skills(
    native_connection: &agent::NativeAgentConnection,
    session_id: &acp::SessionId,
    cx: &App,
) -> Vec<AvailableSkill> {
    native_connection
        .available_skills(session_id, cx)
        .into_iter()
        .map(|skill| AvailableSkill {
            name: skill.name.into(),
            description: skill.description.into(),
            source: skill.source,
            skill_file_path: skill.skill_file_path,
            warning: skill.warning,
        })
        .collect()
}

pub(in crate::conversation_view) fn placeholder_text(
    _agent_name: &str,
    _has_commands: bool,
) -> String {
    "Ask anything".to_string()
}

pub(in crate::conversation_view) fn plan_label_markdown_style(
    status: &acp::PlanEntryStatus,
    window: &Window,
    cx: &App,
) -> MarkdownStyle {
    let default_md_style = MarkdownStyle::themed(MarkdownFont::Agent, window, cx);

    MarkdownStyle {
        base_text_style: TextStyle {
            color: cx.theme().colors().text_muted,
            strikethrough: if matches!(status, acp::PlanEntryStatus::Completed) {
                Some(gpui::StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(cx.theme().colors().text_muted.opacity(0.8)),
                })
            } else {
                None
            },
            ..default_md_style.base_text_style
        },
        ..default_md_style
    }
}
