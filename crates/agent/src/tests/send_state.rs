use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_in_progress_send_canceled_by_next_send(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let events_1 = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello 1"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hey 1!");
    cx.run_until_parked();

    let events_2 = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello 2"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hey 2!");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    let events_1 = events_1.collect::<Vec<_>>().await;
    assert_eq!(stop_events(events_1), vec![acp::StopReason::Cancelled]);
    let events_2 = events_2.collect::<Vec<_>>().await;
    assert_eq!(stop_events(events_2), vec![acp::StopReason::EndTurn]);
}

#[gpui::test]
async fn test_retry_cancelled_promptly_on_new_send(cx: &mut TestAppContext) {
    // Regression test: when a completion fails with a retryable error (e.g. upstream 500),
    // the retry loop waits on a timer. If the user switches models and sends a new message
    // during that delay, the old turn should exit immediately instead of retrying with the
    // stale model.
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let model_a = model.as_fake();

    // Start a turn with model_a.
    let events_1 = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert_eq!(model_a.completion_count(), 1);

    // Model returns a retryable upstream 500. The turn enters the retry delay.
    model_a.send_last_completion_stream_error(
        LanguageModelCompletionError::UpstreamProviderError {
            message: "Internal server error".to_string(),
            status: http_client::StatusCode::INTERNAL_SERVER_ERROR,
            retry_after: None,
        },
    );
    model_a.end_last_completion_stream();
    cx.run_until_parked();

    // The old completion was consumed; model_a has no pending requests yet because the
    // retry timer hasn't fired.
    assert_eq!(model_a.completion_count(), 0);

    // Switch to model_b and send a new message. This cancels the old turn.
    let model_b = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "fake", "model-b", "Model B", false,
    ));
    thread.update(cx, |thread, cx| {
        thread.set_model(model_b.clone(), cx);
    });
    let events_2 = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Continue"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // model_b should have received its completion request.
    assert_eq!(model_b.as_fake().completion_count(), 1);

    // Advance the clock well past the retry delay (BASE_RETRY_DELAY = 5s).
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();

    // model_a must NOT have received another completion request — the cancelled turn
    // should have exited during the retry delay rather than retrying with the old model.
    assert_eq!(
        model_a.completion_count(),
        0,
        "old model should not receive a retry request after cancellation"
    );

    // Complete model_b's turn.
    model_b
        .as_fake()
        .send_last_completion_stream_text_chunk("Done!");
    model_b
        .as_fake()
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    model_b.as_fake().end_last_completion_stream();

    let events_1 = events_1.collect::<Vec<_>>().await;
    assert_eq!(stop_events(events_1), vec![acp::StopReason::Cancelled]);

    let events_2 = events_2.collect::<Vec<_>>().await;
    assert_eq!(stop_events(events_2), vec![acp::StopReason::EndTurn]);
}

#[gpui::test]
async fn test_subsequent_successful_sends_dont_cancel(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let events_1 = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello 1"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hey 1!");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();
    let events_1 = events_1.collect::<Vec<_>>().await;

    let events_2 = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello 2"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hey 2!");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();
    let events_2 = events_2.collect::<Vec<_>>().await;

    assert_eq!(stop_events(events_1), vec![acp::StopReason::EndTurn]);
    assert_eq!(stop_events(events_2), vec![acp::StopReason::EndTurn]);
}

#[gpui::test]
async fn test_refusal(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Hello
            "}
        );
    });

    fake_model.send_last_completion_stream_text_chunk("Hey!");
    cx.run_until_parked();
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Hello

                ## Assistant

                Hey!
            "}
        );
    });

    // If the model refuses to continue, the thread should remove all the messages after the last user message.
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::Refusal));
    let events = events.collect::<Vec<_>>().await;
    assert_eq!(stop_events(events), vec![acp::StopReason::Refusal]);
    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.to_markdown(), "");
    });
}
