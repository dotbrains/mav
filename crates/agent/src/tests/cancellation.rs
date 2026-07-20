use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
#[cfg_attr(not(feature = "e2e"), ignore)]
async fn test_cancellation(cx: &mut TestAppContext) {
    let ThreadTest { thread, .. } = setup(cx, TestModel::Sonnet4).await;

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(InfiniteTool);
            thread.add_tool(EchoTool);
            thread.send(
                ClientUserMessageId::new(),
                ["Call the echo tool, then call the infinite tool, then explain their output"],
                cx,
            )
        })
        .unwrap();

    // Wait until both tools are called.
    let mut expected_tools = vec!["Echo", "Infinite Tool"];
    let mut echo_id = None;
    let mut echo_completed = false;
    while let Some(event) = events.next().await {
        match event.unwrap() {
            ThreadEvent::ToolCall(tool_call) => {
                assert_eq!(tool_call.title, expected_tools.remove(0));
                if tool_call.title == "Echo" {
                    echo_id = Some(tool_call.tool_call_id);
                }
            }
            ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(
                acp::ToolCallUpdate {
                    tool_call_id,
                    fields:
                        acp::ToolCallUpdateFields {
                            status: Some(acp::ToolCallStatus::Completed),
                            ..
                        },
                    ..
                },
            )) if Some(&tool_call_id) == echo_id.as_ref() => {
                echo_completed = true;
            }
            _ => {}
        }

        if expected_tools.is_empty() && echo_completed {
            break;
        }
    }

    // Cancel the current send and ensure that the event stream is closed, even
    // if one of the tools is still running.
    thread.update(cx, |thread, cx| thread.cancel(cx)).await;
    let events = events.collect::<Vec<_>>().await;
    let last_event = events.last();
    assert!(
        matches!(
            last_event,
            Some(Ok(ThreadEvent::Stop(acp::StopReason::Cancelled)))
        ),
        "unexpected event {last_event:?}"
    );

    // Ensure we can still send a new message after cancellation.
    let events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Testing: reply with 'Hello' then stop."],
                cx,
            )
        })
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    thread.update(cx, |thread, _cx| {
        let message = thread.last_received_or_pending_message().unwrap();
        let agent_message = message.as_agent_message().unwrap();
        assert_eq!(
            agent_message.content,
            vec![AgentMessageContent::Text("Hello".to_string())]
        );
    });
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);
}

#[gpui::test]
async fn test_terminal_tool_cancellation_captures_output(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    always_allow_tools(cx);
    disable_sandboxing(cx);
    let fake_model = model.as_fake();

    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));
    let handle = environment.terminal_handle.clone().unwrap();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(crate::TerminalTool::new(
                thread.project().clone(),
                environment,
            ));
            thread.send(ClientUserMessageId::new(), ["run a command"], cx)
        })
        .unwrap();

    cx.run_until_parked();

    // Simulate the model calling the terminal tool
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "terminal_tool_1".into(),
            name: TerminalTool::NAME.into(),
            raw_input: r#"{"command": "sleep 1000", "cd": "."}"#.into(),
            input: json!({"command": "sleep 1000", "cd": "."}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    // Wait for the terminal tool to start running
    wait_for_terminal_tool_started(&mut events, cx).await;

    // Cancel the thread while the terminal is running
    thread.update(cx, |thread, cx| thread.cancel(cx)).detach();

    // Collect remaining events, driving the executor to let cancellation complete
    let remaining_events = collect_events_until_stop(&mut events, cx).await;

    // Verify the terminal was killed
    assert!(
        handle.was_killed(),
        "expected terminal handle to be killed on cancellation"
    );

    // Verify we got a cancellation stop event
    assert_eq!(
        stop_events(remaining_events),
        vec![acp::StopReason::Cancelled],
    );

    // Verify the tool result contains the terminal output, not just "Tool canceled by user"
    thread.update(cx, |thread, _cx| {
        let message = thread.last_received_or_pending_message().unwrap();
        let agent_message = message.as_agent_message().unwrap();

        let tool_use = agent_message
            .content
            .iter()
            .find_map(|content| match content {
                AgentMessageContent::ToolUse(tool_use) => Some(tool_use),
                _ => None,
            })
            .expect("expected tool use in agent message");

        let tool_result = agent_message
            .tool_results
            .get(&tool_use.id)
            .expect("expected tool result");

        let result_text = tool_result.text_contents();

        // "partial output" comes from FakeTerminalHandle's output field
        assert!(
            result_text.contains("partial output"),
            "expected tool result to contain terminal output, got: {result_text}"
        );
        // Match the actual format from process_content in terminal_tool.rs
        assert!(
            result_text.contains("The user stopped this command"),
            "expected tool result to indicate user stopped, got: {result_text}"
        );
    });

    // Verify we can send a new message after cancellation
    verify_thread_recovery(&thread, &fake_model, cx).await;
}

#[gpui::test]
async fn test_cancellation_aware_tool_responds_to_cancellation(cx: &mut TestAppContext) {
    // This test verifies that tools which properly handle cancellation via
    // `event_stream.cancelled_by_user()` (like edit_file_tool) respond promptly
    // to cancellation and report that they were cancelled.
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    always_allow_tools(cx);
    let fake_model = model.as_fake();

    let (tool, was_cancelled) = CancellationAwareTool::new();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(tool);
            thread.send(
                ClientUserMessageId::new(),
                ["call the cancellation aware tool"],
                cx,
            )
        })
        .unwrap();

    cx.run_until_parked();

    // Simulate the model calling the cancellation-aware tool
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "cancellation_aware_1".into(),
            name: "cancellation_aware".into(),
            raw_input: r#"{}"#.into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    cx.run_until_parked();

    // Wait for the tool call to be reported
    let mut tool_started = false;
    let deadline = cx.executor().num_cpus() * 100;
    for _ in 0..deadline {
        cx.run_until_parked();

        while let Some(Some(event)) = events.next().now_or_never() {
            if let Ok(ThreadEvent::ToolCall(tool_call)) = &event {
                if tool_call.title == "Cancellation Aware Tool" {
                    tool_started = true;
                    break;
                }
            }
        }

        if tool_started {
            break;
        }

        cx.background_executor
            .timer(Duration::from_millis(10))
            .await;
    }
    assert!(tool_started, "expected cancellation aware tool to start");

    // Cancel the thread and wait for it to complete
    let cancel_task = thread.update(cx, |thread, cx| thread.cancel(cx));

    // The cancel task should complete promptly because the tool handles cancellation
    let timeout = cx.background_executor.timer(Duration::from_secs(5));
    futures::select! {
        _ = cancel_task.fuse() => {}
        _ = timeout.fuse() => {
            panic!("cancel task timed out - tool did not respond to cancellation");
        }
    }

    // Verify the tool detected cancellation via its flag
    assert!(
        was_cancelled.load(std::sync::atomic::Ordering::SeqCst),
        "tool should have detected cancellation via event_stream.cancelled_by_user()"
    );

    // Collect remaining events
    let remaining_events = collect_events_until_stop(&mut events, cx).await;

    // Verify we got a cancellation stop event
    assert_eq!(
        stop_events(remaining_events),
        vec![acp::StopReason::Cancelled],
    );

    // Verify we can send a new message after cancellation
    verify_thread_recovery(&thread, &fake_model, cx).await;
}
