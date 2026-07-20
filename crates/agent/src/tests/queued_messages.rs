use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_queued_message_ends_turn_at_boundary(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(EchoTool);
    });

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Use the echo tool"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "echo".into(),
            raw_input: r#"{"text": "hello"}"#.into(),
            input: json!({"text": "hello"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::ToolUse));

    thread.update(cx, |thread, _cx| {
        thread.set_end_turn_at_next_boundary(true);
    });

    fake_model.end_last_completion_stream();

    let all_events = collect_events_until_stop(&mut events, cx).await;
    let tool_call_ids: Vec<_> = all_events
        .iter()
        .filter_map(|e| match e {
            Ok(ThreadEvent::ToolCall(tc)) => Some(tc.tool_call_id.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tool_call_ids,
        vec!["tool_1"],
        "Should have received a tool call event for our echo tool"
    );

    let stop_reasons = stop_events(all_events);
    assert_eq!(
        stop_reasons,
        vec![acp::StopReason::EndTurn],
        "Turn should have ended after tool completion due to queued message"
    );

    thread.update(cx, |thread, _cx| {
        assert!(
            thread.end_turn_at_next_boundary(),
            "Should still have the end-turn-at-boundary flag set"
        );
    });

    thread.update(cx, |thread, _cx| {
        assert!(
            thread.is_turn_complete(),
            "Thread should not be running after turn ends"
        );
    });
}

#[gpui::test]
async fn test_queued_message_does_not_end_turn_without_boundary_flag(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(EchoTool);
    });

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Use the echo tool"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "echo".into(),
            raw_input: r#"{"text": "hello"}"#.into(),
            input: json!({"text": "hello"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::ToolUse));

    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let continuation = fake_model.pending_completions();
    assert_eq!(
        continuation.len(),
        1,
        "Without the boundary flag, the turn should continue with another completion request"
    );

    fake_model.send_last_completion_stream_text_chunk("All done");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    let all_events = collect_events_until_stop(&mut events, cx).await;
    let stop_reasons = stop_events(all_events);
    assert_eq!(
        stop_reasons,
        vec![acp::StopReason::EndTurn],
        "Turn should end only after the agent finishes, not at the tool boundary"
    );
}
