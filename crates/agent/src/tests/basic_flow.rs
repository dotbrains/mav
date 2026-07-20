use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_echo(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Testing: Reply with 'Hello'"],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hello");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    let events = events.collect().await;
    thread.update(cx, |thread, _cx| {
        assert_eq!(
            thread.last_received_or_pending_message().unwrap().role(),
            Role::Assistant
        );
        assert_eq!(
            thread
                .last_received_or_pending_message()
                .unwrap()
                .to_markdown(),
            "Hello\n"
        )
    });
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);
}

#[gpui::test]
async fn test_thinking(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                [indoc! {"
                    Testing:

                    Generate a thinking step where you just think the word 'Think',
                    and have your final answer be 'Hello'
                "}],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::Thinking {
        text: "Think".to_string(),
        signature: None,
    });
    fake_model.send_last_completion_stream_text_chunk("Hello");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    let events = events.collect().await;
    thread.update(cx, |thread, _cx| {
        assert_eq!(
            thread.last_received_or_pending_message().unwrap().role(),
            Role::Assistant
        );
        assert_eq!(
            thread
                .last_received_or_pending_message()
                .unwrap()
                .to_markdown(),
            indoc! {"
                <think>Think</think>
                Hello
            "}
        )
    });
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);
}

#[gpui::test]
async fn test_thinking_allowed_when_model_cannot_disable_thinking(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();
    fake_model.set_supports_thinking(true);

    thread.update(cx, |thread, cx| {
        thread.set_thinking_enabled(false, cx);
        let request = thread
            .build_completion_request(CompletionIntent::UserPrompt, cx)
            .unwrap();
        assert!(!request.thinking_allowed);
    });

    fake_model.set_supports_disabling_thinking(false);
    thread.update(cx, |thread, cx| {
        let request = thread
            .build_completion_request(CompletionIntent::UserPrompt, cx)
            .unwrap();
        assert!(request.thinking_allowed);
    });
}

#[gpui::test]
async fn test_system_prompt(cx: &mut TestAppContext) {
    let ThreadTest {
        model,
        thread,
        project_context,
        ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    project_context.update(cx, |project_context, _cx| {
        project_context.shell = "test-shell".into()
    });
    thread.update(cx, |thread, _| thread.add_tool(EchoTool));
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    let mut pending_completions = fake_model.pending_completions();
    assert_eq!(
        pending_completions.len(),
        1,
        "unexpected pending completions: {:?}",
        pending_completions
    );

    let pending_completion = pending_completions.pop().unwrap();
    assert_eq!(pending_completion.messages[0].role, Role::System);

    let system_message = &pending_completion.messages[0];
    let MessageContent::Text(system_prompt) = &system_message.content[0] else {
        panic!("Expected text content");
    };
    assert!(
        system_prompt.contains("test-shell"),
        "unexpected system message: {:?}",
        system_message
    );
    assert!(
        system_prompt.contains("## Fixing Diagnostics"),
        "unexpected system message: {:?}",
        system_message
    );
}

#[gpui::test]
async fn test_system_prompt_without_tools(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    let mut pending_completions = fake_model.pending_completions();
    assert_eq!(
        pending_completions.len(),
        1,
        "unexpected pending completions: {:?}",
        pending_completions
    );

    let pending_completion = pending_completions.pop().unwrap();
    assert_eq!(pending_completion.messages[0].role, Role::System);

    let system_message = &pending_completion.messages[0];
    let MessageContent::Text(system_prompt) = &system_message.content[0] else {
        panic!("Expected text content");
    };
    assert!(
        !system_prompt.contains("## Tool Use"),
        "unexpected system message: {:?}",
        system_message
    );
    assert!(
        !system_prompt.contains("## Fixing Diagnostics"),
        "unexpected system message: {:?}",
        system_message
    );
}

#[gpui::test]
async fn test_prompt_caching(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Message 1"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        completion.messages[1..],
        vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec!["Message 1".into()],
            cache: true,
            reasoning_details: None,
        }]
    );
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::Text(
        "Response to Message 1".into(),
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Message 2"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Message 1".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec!["Response to Message 1".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Message 2".into()],
                cache: true,
                reasoning_details: None,
            }
        ]
    );
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::Text(
        "Response to Message 2".into(),
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    thread.update(cx, |thread, _| thread.add_tool(EchoTool));
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Use the echo tool"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let tool_use = LanguageModelToolUse {
        id: "tool_1".into(),
        name: EchoTool::NAME.into(),
        raw_input: json!({"text": "test"}).to_string(),
        input: json!({"text": "test"}),
        is_input_complete: true,
        thought_signature: None,
    };
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(tool_use.clone()));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    let tool_result = LanguageModelToolResult {
        tool_use_id: "tool_1".into(),
        tool_name: EchoTool::NAME.into(),
        is_error: false,
        content: vec!["test".into()],
        output: Some("test".into()),
    };
    assert_eq!(
        completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Message 1".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec!["Response to Message 1".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Message 2".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec!["Response to Message 2".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Use the echo tool".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![MessageContent::ToolUse(tool_use)],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::ToolResult(tool_result)],
                cache: true,
                reasoning_details: None,
            }
        ]
    );
}
