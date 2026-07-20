use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_truncate_while_terminal_tool_running(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    always_allow_tools(cx);
    disable_sandboxing(cx);
    let fake_model = model.as_fake();

    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));
    let handle = environment.terminal_handle.clone().unwrap();

    let message_id = ClientUserMessageId::new();
    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(crate::TerminalTool::new(
                thread.project().clone(),
                environment,
            ));
            thread.send(message_id.clone(), ["run a command"], cx)
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

    // Truncate the thread while the terminal is running
    thread
        .update(cx, |thread, cx| thread.truncate(message_id, cx))
        .unwrap();

    // Drive the executor to let cancellation complete
    let _ = collect_events_until_stop(&mut events, cx).await;

    // Verify the terminal was killed
    assert!(
        handle.was_killed(),
        "expected terminal handle to be killed on truncate"
    );

    // Verify the thread is empty after truncation
    thread.update(cx, |thread, _cx| {
        assert_eq!(
            thread.to_markdown(),
            "",
            "expected thread to be empty after truncating the only message"
        );
    });

    // Verify we can send a new message after truncation
    verify_thread_recovery(&thread, &fake_model, cx).await;
}

#[gpui::test]
async fn test_cancel_multiple_concurrent_terminal_tools(cx: &mut TestAppContext) {
    // Tests that cancellation properly kills all running terminal tools when multiple are active.
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    always_allow_tools(cx);
    disable_sandboxing(cx);
    let fake_model = model.as_fake();

    let environment = Rc::new(MultiTerminalEnvironment::new());

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(crate::TerminalTool::new(
                thread.project().clone(),
                environment.clone(),
            ));
            thread.send(ClientUserMessageId::new(), ["run multiple commands"], cx)
        })
        .unwrap();

    cx.run_until_parked();

    // Simulate the model calling two terminal tools
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
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "terminal_tool_2".into(),
            name: TerminalTool::NAME.into(),
            raw_input: r#"{"command": "sleep 2000", "cd": "."}"#.into(),
            input: json!({"command": "sleep 2000", "cd": "."}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    // Wait for both terminal tools to start by counting terminal content updates
    let mut terminals_started = 0;
    let deadline = cx.executor().num_cpus() * 100;
    for _ in 0..deadline {
        cx.run_until_parked();

        while let Some(Some(event)) = events.next().now_or_never() {
            if let Ok(ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(
                update,
            ))) = &event
            {
                if update.fields.content.as_ref().is_some_and(|content| {
                    content
                        .iter()
                        .any(|c| matches!(c, acp::ToolCallContent::Terminal(_)))
                }) {
                    terminals_started += 1;
                    if terminals_started >= 2 {
                        break;
                    }
                }
            }
        }
        if terminals_started >= 2 {
            break;
        }

        cx.background_executor
            .timer(Duration::from_millis(10))
            .await;
    }
    assert!(
        terminals_started >= 2,
        "expected 2 terminal tools to start, got {terminals_started}"
    );

    // Cancel the thread while both terminals are running
    thread.update(cx, |thread, cx| thread.cancel(cx)).detach();

    // Collect remaining events
    let remaining_events = collect_events_until_stop(&mut events, cx).await;

    // Verify both terminal handles were killed
    let handles = environment.handles();
    assert_eq!(
        handles.len(),
        2,
        "expected 2 terminal handles to be created"
    );
    assert!(
        handles[0].was_killed(),
        "expected first terminal handle to be killed on cancellation"
    );
    assert!(
        handles[1].was_killed(),
        "expected second terminal handle to be killed on cancellation"
    );

    // Verify we got a cancellation stop event
    assert_eq!(
        stop_events(remaining_events),
        vec![acp::StopReason::Cancelled],
    );
}

#[gpui::test]
async fn test_terminal_tool_stopped_via_terminal_card_button(cx: &mut TestAppContext) {
    // Tests that clicking the stop button on the terminal card (as opposed to the main
    // cancel button) properly reports user stopped via the was_stopped_by_user path.
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

    // Simulate user clicking stop on the terminal card itself.
    // This sets the flag and signals exit (simulating what the real UI would do).
    handle.set_stopped_by_user(true);
    handle.killed.store(true, Ordering::SeqCst);
    handle.signal_exit();

    // Wait for the tool to complete
    cx.run_until_parked();

    // The thread continues after tool completion - simulate the model ending its turn
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    // Collect remaining events
    let remaining_events = collect_events_until_stop(&mut events, cx).await;

    // Verify we got an EndTurn (not Cancelled, since we didn't cancel the thread)
    assert_eq!(
        stop_events(remaining_events),
        vec![acp::StopReason::EndTurn],
    );

    // Verify the tool result indicates user stopped
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

        assert!(
            result_text.contains("The user stopped this command"),
            "expected tool result to indicate user stopped, got: {result_text}"
        );
    });
}

#[gpui::test]
async fn test_terminal_tool_timeout_expires(cx: &mut TestAppContext) {
    // Tests that when a timeout is configured and expires, the tool result indicates timeout.
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
            thread.send(
                ClientUserMessageId::new(),
                ["run a command with timeout"],
                cx,
            )
        })
        .unwrap();

    cx.run_until_parked();

    // Simulate the model calling the terminal tool with a short timeout
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "terminal_tool_1".into(),
            name: TerminalTool::NAME.into(),
            raw_input: r#"{"command": "sleep 1000", "cd": ".", "timeout_ms": 100}"#.into(),
            input: json!({"command": "sleep 1000", "cd": ".", "timeout_ms": 100}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    // Wait for the terminal tool to start running
    wait_for_terminal_tool_started(&mut events, cx).await;

    // Advance clock past the timeout
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    // The thread continues after tool completion - simulate the model ending its turn
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    // Collect remaining events
    let remaining_events = collect_events_until_stop(&mut events, cx).await;

    // Verify the terminal was killed due to timeout
    assert!(
        handle.was_killed(),
        "expected terminal handle to be killed on timeout"
    );

    // Verify we got an EndTurn (the tool completed, just with timeout)
    assert_eq!(
        stop_events(remaining_events),
        vec![acp::StopReason::EndTurn],
    );

    // Verify the tool result indicates timeout, not user stopped
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

        assert!(
            result_text.contains("timed out"),
            "expected tool result to indicate timeout, got: {result_text}"
        );
        assert!(
            !result_text.contains("The user stopped"),
            "tool result should not mention user stopped when it timed out, got: {result_text}"
        );
    });
}
