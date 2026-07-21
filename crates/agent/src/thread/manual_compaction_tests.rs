use super::*;
use gpui::TestAppContext;
use language_model::fake_provider::FakeLanguageModel;

#[gpui::test]
async fn test_manual_compact_forces_summary(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    model.set_max_token_count(MIN_COMPACTION_CONTEXT_WINDOW - 1);
    let user_message_id = ClientUserMessageId::new();
    let compact_message_id = ClientUserMessageId::new();

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.messages.push(tests::user_text_message(
                user_message_id.clone(),
                "old user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("old assistant"));
            assert_eq!(thread.compaction_message_target_ix(cx), None);
        });
    });

    let _events = cx
        .update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.compact(compact_message_id.clone(), cx)
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

    model.send_completion_stream_text_chunk(&compaction_request, "summary of old context");
    model.end_completion_stream(&compaction_request);
    cx.run_until_parked();

    assert!(model.pending_completions().is_empty());
    cx.update(|cx| {
        thread.read_with(cx, |thread, _cx| {
            assert!(matches!(&*thread.messages[0], Message::User(_)));
            assert!(matches!(&*thread.messages[1], Message::Agent(_)));
            assert!(matches!(
                &*thread.messages[2],
                Message::User(UserMessage { id, content }) if id == &compact_message_id && content.is_empty()
            ));
            assert!(matches!(
                &*thread.messages[3],
                Message::Compaction(CompactionInfo::Summary(summary)) if summary.as_ref() == "summary of old context"
            ));
            assert_eq!(thread.forced_compaction_target_ix(), None);
        });

        thread
            .update(cx, |thread, cx| thread.truncate(compact_message_id.clone(), cx))
            .unwrap();

        thread.read_with(cx, |thread, _cx| {
            assert_eq!(thread.messages.len(), 2);
            assert!(matches!(&*thread.messages[0], Message::User(_)));
            assert!(matches!(&*thread.messages[1], Message::Agent(_)));
        });
    });
}

#[gpui::test]
async fn test_manual_compact_cancelled_leaves_no_marker(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "old user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("old assistant"));
        });
    });

    let _events = cx
        .update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.compact(ClientUserMessageId::new(), cx)
            })
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(model.pending_completions().len(), 1);

    cx.update(|cx| thread.update(cx, |thread, cx| thread.cancel(cx)))
        .await;
    cx.run_until_parked();

    thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.messages.len(), 2);
        assert!(matches!(&*thread.messages[0], Message::User(_)));
        assert!(matches!(&*thread.messages[1], Message::Agent(_)));
    });
}

#[gpui::test]
async fn test_manual_compact_empty_summary_leaves_no_marker(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "old user",
            ));
            thread
                .messages
                .push(tests::agent_text_message("old assistant"));
        });
    });

    let mut events = cx
        .update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.compact(ClientUserMessageId::new(), cx)
            })
        })
        .unwrap();
    cx.run_until_parked();

    let request = model.pending_completions().pop().unwrap();
    model.end_completion_stream(&request);
    cx.run_until_parked();

    let mut saw_error = false;
    while let Some(event) = events.next().await {
        if event.is_err() {
            saw_error = true;
        }
    }
    assert!(saw_error, "expected an error event for the empty summary");
    thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.messages.len(), 2);
        assert!(matches!(&*thread.messages[0], Message::User(_)));
        assert!(matches!(&*thread.messages[1], Message::Agent(_)));
    });
}

#[gpui::test]
async fn test_manual_compact_noop_on_empty_thread(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let model = Arc::new(FakeLanguageModel::default());
    cx.update(|cx| thread.update(cx, |thread, cx| thread.set_model(model.clone(), cx)));

    let _events = cx
        .update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.compact(ClientUserMessageId::new(), cx)
            })
        })
        .unwrap();
    cx.run_until_parked();

    assert!(model.pending_completions().is_empty());
    thread.read_with(cx, |thread, _cx| {
        assert!(thread.messages.is_empty());
    });
}

#[gpui::test]
async fn test_manual_compact_marker_replays_as_empty_user_message(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;
    let marker_id = ClientUserMessageId::new();

    let mut replay_events = cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.messages.push(tests::user_text_message(
                ClientUserMessageId::new(),
                "before",
            ));
            thread.messages.push(tests::agent_text_message("answer"));
            thread.messages.push(Arc::new(Message::User(UserMessage {
                id: marker_id.clone(),
                content: Arc::from([]),
            })));
            thread.messages.push(tests::summary_compaction("summary"));
            thread.replay(cx)
        })
    });

    let _ = replay_events.next().await;
    let _ = replay_events.next().await;

    let event = replay_events.next().await;
    match event {
        Some(Ok(ThreadEvent::UserMessage(message))) => {
            assert_eq!(message.id, marker_id);
            assert!(
                message.content.is_empty(),
                "marker should replay with no content so the UI renders nothing"
            );
        }
        _ => panic!("expected the marker to replay as a user message, got {event:?}"),
    }

    let event = replay_events.next().await;
    assert!(
        matches!(&event, Some(Ok(ThreadEvent::ContextCompaction(_)))),
        "expected the compaction to replay after the marker, got {event:?}"
    );
}
