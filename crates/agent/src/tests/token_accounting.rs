use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_tokens_before_message(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    // First message
    let message_1_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(message_1_id.clone(), ["First message"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Before any response, tokens_before_message should return None for first message
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.tokens_before_message(&message_1_id),
            None,
            "First message should have no tokens before it"
        );
    });

    // Complete first message with usage
    fake_model.send_last_completion_stream_text_chunk("Response 1");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // First message still has no tokens before it
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.tokens_before_message(&message_1_id),
            None,
            "First message should still have no tokens before it after response"
        );
    });

    // Second message
    let message_2_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(message_2_id.clone(), ["Second message"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Second message should have first message's input tokens before it
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.tokens_before_message(&message_2_id),
            Some(100),
            "Second message should have 100 tokens before it (from first request)"
        );
    });

    // Complete second message
    fake_model.send_last_completion_stream_text_chunk("Response 2");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 250, // Total for this request (includes previous context)
            output_tokens: 75,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // Third message
    let message_3_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(message_3_id.clone(), ["Third message"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Third message should have second message's input tokens (250) before it
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.tokens_before_message(&message_3_id),
            Some(250),
            "Third message should have 250 tokens before it (from second request)"
        );
        // Second message should still have 100
        assert_eq!(
            thread.tokens_before_message(&message_2_id),
            Some(100),
            "Second message should still have 100 tokens before it"
        );
        // First message still has none
        assert_eq!(
            thread.tokens_before_message(&message_1_id),
            None,
            "First message should still have no tokens before it"
        );
    });
}

#[gpui::test]
async fn test_tokens_before_message_after_truncate(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    // Set up three messages with responses
    let message_1_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(message_1_id.clone(), ["Message 1"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Response 1");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let message_2_id = ClientUserMessageId::new();
    thread
        .update(cx, |thread, cx| {
            thread.send(message_2_id.clone(), ["Message 2"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Response 2");
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::UsageUpdate(
        language_model::TokenUsage {
            input_tokens: 250,
            output_tokens: 75,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // Verify initial state
    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.tokens_before_message(&message_2_id), Some(100));
    });

    // Truncate at message 2 (removes message 2 and everything after)
    thread
        .update(cx, |thread, cx| thread.truncate(message_2_id.clone(), cx))
        .unwrap();
    cx.run_until_parked();

    // After truncation, message_2_id no longer exists, so lookup should return None
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.tokens_before_message(&message_2_id),
            None,
            "After truncation, message 2 no longer exists"
        );
        // Message 1 still exists but has no tokens before it
        assert_eq!(
            thread.tokens_before_message(&message_1_id),
            None,
            "First message still has no tokens before it"
        );
    });
}
