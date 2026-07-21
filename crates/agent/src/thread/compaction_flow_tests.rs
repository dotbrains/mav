use super::*;
use gpui::TestAppContext;
use language_model::fake_provider::FakeLanguageModel;
use serde_json::json;

#[gpui::test]
async fn test_compaction_inserts_before_new_user_and_requests_compacted_window(
    cx: &mut TestAppContext,
) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    let old_user_message_id = ClientUserMessageId::new();
    let new_user_message_id = ClientUserMessageId::new();

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.messages.push(tests::user_text_message(
                old_user_message_id.clone(),
                "old user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("old assistant"));
            thread.request_token_usage.insert(
                old_user_message_id.clone(),
                language_model::TokenUsage {
                    input_tokens: 960_000,
                    ..Default::default()
                },
            );
        });
    });

    let _events = cx
        .update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.send(new_user_message_id, vec!["new prompt"], cx)
            })
        })
        .unwrap();
    cx.run_until_parked();

    let compaction_request = model.pending_completions().pop().unwrap();
    assert_eq!(
        compaction_request.intent,
        Some(CompletionIntent::ThreadContextSummarization)
    );
    let compaction_texts = tests::request_texts_after_system(&compaction_request.messages);
    assert_eq!(compaction_texts.len(), 3);
    assert_eq!(compaction_texts[0], "old user");
    assert_eq!(compaction_texts[1], "old assistant");
    assert_eq!(compaction_texts[2], COMPACTION_PROMPT);

    model.send_completion_stream_text_chunk(&compaction_request, "compacted old context");
    model.end_completion_stream(&compaction_request);
    cx.run_until_parked();

    let final_request = model.pending_completions().pop().unwrap();
    assert_eq!(final_request.intent, Some(CompletionIntent::UserPrompt));
    assert_eq!(
        tests::request_texts_after_system(&final_request.messages),
        vec![
            "old user".to_string(),
            tests::summary_request_text("compacted old context"),
            "new prompt".to_string(),
        ]
    );

    model.send_completion_stream_text_chunk(&final_request, "answer");
    model.end_completion_stream(&final_request);
    cx.run_until_parked();

    cx.update(|cx| {
        thread.read_with(cx, |thread, _cx| {
            assert!(matches!(&*thread.messages[0], Message::User(_)));
            assert!(matches!(&*thread.messages[1], Message::Agent(_)));
            assert!(matches!(
                &*thread.messages[2],
                Message::Compaction(CompactionInfo::Summary(summary)) if summary.as_ref() == "compacted old context"
            ));
            assert!(matches!(&*thread.messages[3], Message::User(_)));
        });
    });
}

#[gpui::test]
async fn test_compaction_usage_counts_toward_cumulative_usage(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    let old_user_message_id = ClientUserMessageId::new();
    let new_user_message_id = ClientUserMessageId::new();
    let prior_usage = TokenUsage {
        input_tokens: 960_000,
        output_tokens: 25,
        ..Default::default()
    };
    let compaction_usage = TokenUsage {
        input_tokens: 40,
        output_tokens: 9,
        cache_creation_input_tokens: 2,
        cache_read_input_tokens: 3,
    };
    let final_usage = TokenUsage {
        input_tokens: 500,
        output_tokens: 50,
        ..Default::default()
    };

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.messages.push(tests::user_text_message(
                old_user_message_id.clone(),
                "old user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("old assistant"));
            thread
                .request_token_usage
                .insert(old_user_message_id.clone(), prior_usage);
            thread.cumulative_token_usage = prior_usage;
            thread.current_request_token_usage = prior_usage;
        });
    });

    let _events = cx
        .update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.send(new_user_message_id.clone(), vec!["new prompt"], cx)
            })
        })
        .unwrap();
    cx.run_until_parked();

    let compaction_request = model.pending_completions().pop().unwrap();
    assert_eq!(
        compaction_request.intent,
        Some(CompletionIntent::ThreadContextSummarization)
    );

    model.send_completion_stream_event(
        &compaction_request,
        LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
            input_tokens: 40,
            output_tokens: 4,
            ..Default::default()
        }),
    );
    model.send_completion_stream_event(
        &compaction_request,
        LanguageModelCompletionEvent::UsageUpdate(compaction_usage),
    );
    model.send_completion_stream_text_chunk(&compaction_request, "compacted old context");
    model.end_completion_stream(&compaction_request);
    cx.run_until_parked();

    let expected_after_compaction = prior_usage + compaction_usage;
    thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.cumulative_token_usage(), expected_after_compaction);
        assert!(
            !thread
                .request_token_usage
                .contains_key(&new_user_message_id)
        );
    });

    let final_request = model.pending_completions().pop().unwrap();
    assert_eq!(final_request.intent, Some(CompletionIntent::UserPrompt));

    model.send_completion_stream_event(
        &final_request,
        LanguageModelCompletionEvent::UsageUpdate(final_usage),
    );
    model.end_completion_stream(&final_request);
    cx.run_until_parked();

    thread.read_with(cx, |thread, _cx| {
        assert_eq!(
            thread.cumulative_token_usage(),
            expected_after_compaction + final_usage
        );
        assert_eq!(
            thread.request_token_usage.get(&new_user_message_id),
            Some(&final_usage)
        );
    });
}

#[gpui::test]
async fn test_replay_emits_context_compaction(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let user_message_id = ClientUserMessageId::new();

    let mut replay_events = cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread
                .messages
                .push(tests::user_text_message(user_message_id.clone(), "before"));
            thread.messages.push(tests::summary_compaction("summary"));
            thread.messages.push(tests::agent_text_message("after"));

            thread.replay(cx)
        })
    });

    let event = replay_events.next().await;
    assert!(
        matches!(
            &event,
            Some(Ok(ThreadEvent::UserMessage(UserMessage { id, .. }))) if id == &user_message_id
        ),
        "expected replayed user message, got {event:?}"
    );

    let event = replay_events.next().await;
    let compaction_id = match &event {
        Some(Ok(ThreadEvent::ContextCompaction(compaction))) => compaction.id.clone(),
        _ => panic!("expected context compaction event, got {event:?}"),
    };

    let event = replay_events.next().await;
    assert!(
        matches!(
            &event,
            Some(Ok(ThreadEvent::ContextCompactionUpdate(update)))
                if update.id == compaction_id && update.summary_delta == "summary"
        ),
        "expected context compaction summary event, got {event:?}"
    );

    let event = replay_events.next().await;
    assert!(
        matches!(&event, Some(Ok(ThreadEvent::AgentText(text))) if text == "after"),
        "expected replayed agent text, got {event:?}"
    );
}

#[gpui::test]
async fn test_native_compaction_boundary(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;

    let request_messages = cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "before native",
            ));
            thread.messages.push(Arc::new(Message::Compaction(
                CompactionInfo::ProviderNative {
                    provider: LanguageModelProviderId::from("openai".to_string()),
                    items: vec![json!({"type": "compaction"})],
                },
            )));
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "after native",
            ));

            thread.build_request_messages(Vec::new(), cx)
        })
    });

    assert_eq!(
        tests::request_texts_after_system(&request_messages),
        vec!["after native".to_string()]
    );
}

#[gpui::test]
async fn test_retained_users_truncate_oldest(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let mut long_text = "START".to_string();
    long_text.push_str(&"x".repeat(COMPACTION_RETAINED_USER_MESSAGES_BYTE_BUDGET));
    long_text.push_str("END");

    let request_messages = cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "dropped older user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("dropped assistant"));
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                &long_text,
            ));
            thread
                .messages
                .push(tests::user_text_message(ClientUserMessageId::new(), "new"));
            thread
                .messages
                .push(tests::summary_compaction("summary context"));
            thread
                .messages
                .push(tests::agent_text_message("after assistant"));
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "after user",
            ));

            thread.build_request_messages(Vec::new(), cx)
        })
    });

    let request_texts = tests::request_texts_after_system(&request_messages);
    assert_eq!(request_texts.len(), 5);
    assert_eq!(
        request_texts[0],
        format!(
            "START{}",
            "x".repeat(COMPACTION_RETAINED_USER_MESSAGES_BYTE_BUDGET - "START".len() - "new".len())
        )
    );
    assert_eq!(request_texts[1], "new");
    assert_eq!(
        request_texts[2],
        tests::summary_request_text("summary context")
    );
    assert_eq!(request_texts[3], "after assistant");
    assert_eq!(request_texts[4], "after user");
    assert!(request_texts.iter().all(
        |text| !text.contains("dropped older user") && !text.contains("dropped assistant")
    ));
}
