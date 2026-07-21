use super::*;
use gpui::TestAppContext;
use language_model::fake_provider::FakeLanguageModel;

#[test]
fn test_summary_compaction_renders_for_request_and_markdown() {
    let message = Message::Compaction(CompactionInfo::Summary("Older context".into()));

    assert_eq!(message.role(), Role::User);
    assert_eq!(message.to_markdown(), "--- Context Compacted ---\n");

    let request_messages = message.to_request();
    assert_eq!(request_messages.len(), 1);
    assert_eq!(request_messages[0].role, Role::User);
    assert!(!request_messages[0].cache);
    assert_eq!(request_messages[0].reasoning_details, None);
    assert_eq!(request_messages[0].content.len(), 1);
    let language_model::MessageContent::Text(text) = &request_messages[0].content[0] else {
        panic!("expected text summary context");
    };
    assert_eq!(
        text.as_str(),
        "The previous conversation was compacted. Use this summary as context:\n\nOlder context"
    );
}

#[gpui::test]
async fn test_thread_summary_request_uses_compacted_history(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let summary_model = Arc::new(FakeLanguageModel::default());

    let summary_task = cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_summarization_model(Some(summary_model.clone()), cx);
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "old user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("old assistant"));
            thread
                .messages
                .push(tests::summary_compaction("first summary"));
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "between user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("between assistant"));
            thread
                .messages
                .push(tests::summary_compaction("latest summary"));
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "after user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("after assistant"));

            thread.summary(cx)
        })
    });
    cx.run_until_parked();

    let summary_request = summary_model.pending_completions().pop().unwrap();
    assert_eq!(
        summary_request.intent,
        Some(CompletionIntent::ThreadContextSummarization)
    );
    assert_eq!(
        tests::request_texts(&summary_request.messages),
        vec![
            "old user".to_string(),
            "between user".to_string(),
            tests::summary_request_text("latest summary"),
            "after user".to_string(),
            "after assistant".to_string(),
            SUMMARIZE_THREAD_DETAILED_PROMPT.to_string(),
        ]
    );

    summary_model.send_completion_stream_text_chunk(&summary_request, "thread summary");
    summary_model.end_completion_stream(&summary_request);
    assert_eq!(summary_task.await.as_deref(), Some("thread summary"));
}

#[test]
fn test_thread_title_request_uses_compacted_history() {
    let messages = vec![
        tests::user_text_message(ClientUserMessageId::new(), "old user"),
        tests::agent_text_message("old assistant"),
        tests::summary_compaction("first summary"),
        tests::user_text_message(ClientUserMessageId::new(), "between user"),
        tests::agent_text_message("between assistant"),
        tests::summary_compaction("latest summary"),
        tests::user_text_message(ClientUserMessageId::new(), "after user"),
        tests::agent_text_message("after assistant"),
    ];

    let request = build_thread_title_request(&messages, Some(0.2));

    assert_eq!(request.intent, Some(CompletionIntent::ThreadSummarization));
    assert_eq!(request.temperature, Some(0.2));
    assert_eq!(
        tests::request_texts(&request.messages),
        vec![
            "old user".to_string(),
            "between user".to_string(),
            tests::summary_request_text("latest summary"),
            "after user".to_string(),
            "after assistant".to_string(),
            SUMMARIZE_THREAD_PROMPT.to_string(),
        ]
    );
}
