use super::*;
use gpui::TestAppContext;
use language_model::LanguageModelToolUseId;
use language_model::fake_provider::FakeLanguageModel;
use serde_json::json;
use std::sync::Arc;

pub(super) async fn setup_thread_for_test(
    cx: &mut TestAppContext,
) -> (Entity<Thread>, ThreadEventStream) {
    cx.update(|cx| {
        let settings_store = settings::SettingsStore::test(cx);
        cx.set_global(settings_store);
    });

    let fs = fs::FakeFs::new(cx.background_executor.clone());
    let templates = Templates::new();
    let project = Project::test(fs.clone(), [], cx).await;

    cx.update(|cx| {
        let project_context = cx.new(|_cx| prompt_store::ProjectContext::default());
        let context_server_store = project.read(cx).context_server_store();
        let context_server_registry =
            cx.new(|cx| ContextServerRegistry::new(context_server_store, cx));

        let thread = cx.new(|cx| {
            Thread::new(
                project,
                project_context,
                context_server_registry,
                templates,
                None,
                cx,
            )
        });

        let (event_tx, _event_rx) = mpsc::unbounded();
        let event_stream = ThreadEventStream(event_tx);

        (thread, event_stream)
    })
}

pub(super) fn set_auto_compact_settings(
    cx: &mut App,
    auto_compact: agent_settings::AutoCompactSettings,
) {
    let mut settings = AgentSettings::get_global(cx).clone();
    settings.auto_compact = auto_compact;
    AgentSettings::override_global(settings, cx);
}

pub(super) fn user_text_message(id: ClientUserMessageId, text: &str) -> Arc<Message> {
    Arc::new(Message::User(UserMessage {
        id,
        content: vec![UserMessageContent::Text(text.to_string())].into(),
    }))
}

pub(super) fn agent_text_message(text: &str) -> Arc<Message> {
    Arc::new(Message::Agent(AgentMessage {
        content: vec![AgentMessageContent::Text(text.to_string())],
        ..Default::default()
    }))
}

pub(super) fn summary_compaction(summary: &str) -> Arc<Message> {
    Arc::new(Message::Compaction(CompactionInfo::Summary(summary.into())))
}

pub(super) fn summary_request_text(summary: &str) -> String {
    format!("The previous conversation was compacted. Use this summary as context:\n\n{summary}")
}

pub(super) fn request_texts_after_system(messages: &[LanguageModelRequestMessage]) -> Vec<String> {
    messages
        .iter()
        .skip(1)
        .map(LanguageModelRequestMessage::string_contents)
        .collect()
}

pub(super) fn request_texts(messages: &[LanguageModelRequestMessage]) -> Vec<String> {
    messages
        .iter()
        .map(LanguageModelRequestMessage::string_contents)
        .collect()
}
