use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_send_no_retry_on_success(cx: &mut TestAppContext) {
    let ThreadTest { thread, model, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello!"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("Hey!");
    fake_model.end_last_completion_stream();

    let mut retry_events = Vec::new();
    while let Some(Ok(event)) = events.next().await {
        match event {
            ThreadEvent::Retry(retry_status) => {
                retry_events.push(retry_status);
            }
            ThreadEvent::Stop(..) => break,
            _ => {}
        }
    }

    assert_eq!(retry_events.len(), 0);
    thread.read_with(cx, |thread, _cx| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Hello!

                ## Assistant

                Hey!
            "}
        )
    });
}

#[gpui::test]
async fn test_send_retry_on_error(cx: &mut TestAppContext) {
    let ThreadTest { thread, model, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello!"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("Hey,");
    fake_model.send_last_completion_stream_error(LanguageModelCompletionError::ServerOverloaded {
        provider: LanguageModelProviderName::new("Anthropic"),
        retry_after: Some(Duration::from_secs(3)),
    });
    fake_model.end_last_completion_stream();

    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();

    fake_model.send_last_completion_stream_text_chunk("there!");
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let mut retry_events = Vec::new();
    while let Some(Ok(event)) = events.next().await {
        match event {
            ThreadEvent::Retry(retry_status) => {
                retry_events.push(retry_status);
            }
            ThreadEvent::Stop(..) => break,
            _ => {}
        }
    }

    assert_eq!(retry_events.len(), 1);
    assert!(matches!(
        retry_events[0],
        acp_thread::RetryStatus { attempt: 1, .. }
    ));
    thread.read_with(cx, |thread, _cx| {
        assert_eq!(
            thread.to_markdown(),
            indoc! {"
                ## User

                Hello!

                ## Assistant

                Hey,

                [resume]

                ## Assistant

                there!
            "}
        )
    });
}

#[gpui::test]
async fn test_send_retry_finishes_tool_calls_on_error(cx: &mut TestAppContext) {
    let ThreadTest { thread, model, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(EchoTool);
            thread.send(ClientUserMessageId::new(), ["Call the echo tool!"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let tool_use_1 = LanguageModelToolUse {
        id: "tool_1".into(),
        name: EchoTool::NAME.into(),
        raw_input: json!({"text": "test"}).to_string(),
        input: json!({"text": "test"}),
        is_input_complete: true,
        thought_signature: None,
    };
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        tool_use_1.clone(),
    ));
    fake_model.send_last_completion_stream_error(LanguageModelCompletionError::ServerOverloaded {
        provider: LanguageModelProviderName::new("Anthropic"),
        retry_after: Some(Duration::from_secs(3)),
    });
    fake_model.end_last_completion_stream();

    cx.executor().advance_clock(Duration::from_secs(3));
    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Call the echo tool!".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![language_model::MessageContent::ToolUse(tool_use_1.clone())],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![language_model::MessageContent::ToolResult(
                    LanguageModelToolResult {
                        tool_use_id: tool_use_1.id.clone(),
                        tool_name: tool_use_1.name.clone(),
                        is_error: false,
                        content: vec!["test".into()],
                        output: Some("test".into())
                    }
                )],
                cache: true,
                reasoning_details: None,
            },
        ]
    );

    fake_model.send_last_completion_stream_text_chunk("Done");
    fake_model.end_last_completion_stream();
    cx.run_until_parked();
    events.collect::<Vec<_>>().await;
    thread.read_with(cx, |thread, _cx| {
        assert_eq!(
            thread.last_received_or_pending_message().as_deref(),
            Some(&Message::Agent(AgentMessage {
                content: vec![AgentMessageContent::Text("Done".into())],
                tool_results: IndexMap::default(),
                reasoning_details: None,
            }))
        );
    })
}

#[gpui::test]
async fn test_send_max_retries_exceeded(cx: &mut TestAppContext) {
    let ThreadTest { thread, model, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Hello!"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    for _ in 0..crate::thread::MAX_RETRY_ATTEMPTS + 1 {
        fake_model.send_last_completion_stream_error(
            LanguageModelCompletionError::ServerOverloaded {
                provider: LanguageModelProviderName::new("Anthropic"),
                retry_after: Some(Duration::from_secs(3)),
            },
        );
        fake_model.end_last_completion_stream();
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }

    let mut errors = Vec::new();
    let mut retry_events = Vec::new();
    while let Some(event) = events.next().await {
        match event {
            Ok(ThreadEvent::Retry(retry_status)) => {
                retry_events.push(retry_status);
            }
            Ok(ThreadEvent::Stop(..)) => break,
            Err(error) => errors.push(error),
            _ => {}
        }
    }

    assert_eq!(
        retry_events.len(),
        crate::thread::MAX_RETRY_ATTEMPTS as usize
    );
    for i in 0..crate::thread::MAX_RETRY_ATTEMPTS as usize {
        assert_eq!(retry_events[i].attempt, i + 1);
    }
    assert_eq!(errors.len(), 1);
    let error = errors[0]
        .downcast_ref::<LanguageModelCompletionError>()
        .unwrap();
    assert!(matches!(
        error,
        LanguageModelCompletionError::ServerOverloaded { .. }
    ));
}

#[gpui::test]
async fn test_streaming_tool_completes_when_llm_stream_ends_without_final_input(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(StreamingEchoTool::new());
    });

    let _events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Use the streaming_echo tool"],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();

    // Send a partial tool use (is_input_complete = false), simulating the LLM
    // streaming input for a tool.
    let tool_use = LanguageModelToolUse {
        id: "tool_1".into(),
        name: "streaming_echo".into(),
        raw_input: r#"{"text": "partial"}"#.into(),
        input: json!({"text": "partial"}),
        is_input_complete: false,
        thought_signature: None,
    };
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(tool_use.clone()));
    cx.run_until_parked();

    // Send a stream error WITHOUT ever sending is_input_complete = true.
    // Before the fix, this would deadlock: the tool waits for more partials
    // (or cancellation), run_turn_internal waits for the tool, and the sender
    // keeping the channel open lives inside RunningTurn.
    fake_model.send_last_completion_stream_error(
        LanguageModelCompletionError::UpstreamProviderError {
            message: "Internal server error".to_string(),
            status: http_client::StatusCode::INTERNAL_SERVER_ERROR,
            retry_after: None,
        },
    );
    fake_model.end_last_completion_stream();

    // Advance past the retry delay so run_turn_internal retries.
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();

    // The retry request should contain the streaming tool's error result,
    // proving the tool terminated and its result was forwarded.
    let completion = fake_model
        .pending_completions()
        .pop()
        .expect("No running turn");
    assert_eq!(
        completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Use the streaming_echo tool".into()],
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
                        content: vec!["tool input was not fully received".into(),],
                        output: Some("tool input was not fully received".into()),
                    }
                )],
                cache: true,
                reasoning_details: None,
            },
        ]
    );

    // Finish the retry round so the turn completes cleanly.
    fake_model.send_last_completion_stream_text_chunk("Done");
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    thread.read_with(cx, |thread, _cx| {
        assert!(
            thread.is_turn_complete(),
            "Thread should not be stuck; the turn should have completed",
        );
    });
}

#[gpui::test]
async fn test_streaming_tool_json_parse_error_is_forwarded_to_running_tool(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(StreamingJsonErrorContextTool);
    });

    let _events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Use the streaming_json_error_context tool"],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();

    let tool_use = LanguageModelToolUse {
        id: "tool_1".into(),
        name: StreamingJsonErrorContextTool::NAME.into(),
        raw_input: r#"{"text": "partial"#.into(),
        input: json!({"text": "partial"}),
        is_input_complete: false,
        thought_signature: None,
    };
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(tool_use));
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(
        LanguageModelCompletionEvent::ToolUseJsonParseError {
            id: "tool_1".into(),
            tool_name: StreamingJsonErrorContextTool::NAME.into(),
            raw_input: r#"{"text": "partial"#.into(),
            json_parse_error: "EOF while parsing a string at line 1 column 17".into(),
        },
    );
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::ToolUse));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();

    let completion = fake_model
        .pending_completions()
        .pop()
        .expect("No running turn");

    let tool_results: Vec<_> = completion
        .messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            MessageContent::ToolResult(result)
                if result.tool_use_id == language_model::LanguageModelToolUseId::from("tool_1") =>
            {
                Some(result)
            }
            _ => None,
        })
        .collect();

    assert_eq!(
        tool_results.len(),
        1,
        "Expected exactly 1 tool result for tool_1, got {}: {:#?}",
        tool_results.len(),
        tool_results
    );

    let result = tool_results[0];
    assert!(result.is_error);
    let content_text = result.text_contents();
    assert!(
        content_text.contains("Saw partial text 'partial' before invalid JSON"),
        "Expected tool-enriched partial context, got: {content_text}"
    );
    assert!(
        content_text
            .contains("Error parsing input JSON: EOF while parsing a string at line 1 column 17"),
        "Expected forwarded JSON parse error, got: {content_text}"
    );
    assert!(
        !content_text.contains("tool input was not fully received"),
        "Should not contain orphaned sender error, got: {content_text}"
    );

    fake_model.send_last_completion_stream_text_chunk("Done");
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    thread.read_with(cx, |thread, _cx| {
        assert!(
            thread.is_turn_complete(),
            "Thread should not be stuck; the turn should have completed",
        );
    });
}
