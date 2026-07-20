use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_streaming_tool_error_breaks_stream_loop_immediately(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(StreamingFailingEchoTool {
            receive_chunks_until_failure: 1,
        });
    });

    let _events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Use the streaming_failing_echo tool"],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();

    let tool_use = LanguageModelToolUse {
        id: "call_1".into(),
        name: StreamingFailingEchoTool::NAME.into(),
        raw_input: "hello".into(),
        input: json!({}),
        is_input_complete: false,
        thought_signature: None,
    };

    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(tool_use.clone()));

    cx.run_until_parked();

    let completions = fake_model.pending_completions();
    let last_completion = completions.last().unwrap();

    assert_eq!(
        last_completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Use the streaming_failing_echo tool".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![language_model::MessageContent::ToolUse(tool_use.clone())],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![language_model::MessageContent::ToolResult(
                    LanguageModelToolResult {
                        tool_use_id: tool_use.id.clone(),
                        tool_name: tool_use.name,
                        is_error: true,
                        content: vec!["failed".into()],
                        output: Some("failed".into()),
                    }
                )],
                cache: true,
                reasoning_details: None,
            },
        ]
    );
}

#[gpui::test]
async fn test_streaming_tool_error_waits_for_prior_tools_to_complete(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let (complete_streaming_echo_tool_call_tx, complete_streaming_echo_tool_call_rx) =
        oneshot::channel();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(
            StreamingEchoTool::new().with_wait_until_complete(complete_streaming_echo_tool_call_rx),
        );
        thread.add_tool(StreamingFailingEchoTool {
            receive_chunks_until_failure: 1,
        });
    });

    let _events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Use the streaming_echo tool and the streaming_failing_echo tool"],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "call_1".into(),
            name: StreamingEchoTool::NAME.into(),
            raw_input: "hello".into(),
            input: json!({ "text": "hello" }),
            is_input_complete: false,
            thought_signature: None,
        },
    ));
    let first_tool_use = LanguageModelToolUse {
        id: "call_1".into(),
        name: StreamingEchoTool::NAME.into(),
        raw_input: "hello world".into(),
        input: json!({ "text": "hello world" }),
        is_input_complete: true,
        thought_signature: None,
    };
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        first_tool_use.clone(),
    ));
    let second_tool_use = LanguageModelToolUse {
        name: StreamingFailingEchoTool::NAME.into(),
        raw_input: "hello".into(),
        input: json!({ "text": "hello" }),
        is_input_complete: false,
        thought_signature: None,
        id: "call_2".into(),
    };
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        second_tool_use.clone(),
    ));

    cx.run_until_parked();

    complete_streaming_echo_tool_call_tx.send(()).unwrap();

    cx.run_until_parked();

    let completions = fake_model.pending_completions();
    let last_completion = completions.last().unwrap();

    assert_eq!(
        last_completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![
                    "Use the streaming_echo tool and the streaming_failing_echo tool".into()
                ],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![
                    language_model::MessageContent::ToolUse(first_tool_use.clone()),
                    language_model::MessageContent::ToolUse(second_tool_use.clone())
                ],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![
                    language_model::MessageContent::ToolResult(LanguageModelToolResult {
                        tool_use_id: second_tool_use.id.clone(),
                        tool_name: second_tool_use.name,
                        is_error: true,
                        content: vec!["failed".into()],
                        output: Some("failed".into()),
                    }),
                    language_model::MessageContent::ToolResult(LanguageModelToolResult {
                        tool_use_id: first_tool_use.id.clone(),
                        tool_name: first_tool_use.name,
                        is_error: false,
                        content: vec!["hello world".into()],
                        output: Some("hello world".into()),
                    }),
                ],
                cache: true,
                reasoning_details: None,
            },
        ]
    );
}
