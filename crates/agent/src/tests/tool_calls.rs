use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
#[cfg_attr(not(feature = "e2e"), ignore)]
async fn test_basic_tool_calls(cx: &mut TestAppContext) {
    let ThreadTest { thread, .. } = setup(cx, TestModel::Sonnet4).await;

    let events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(EchoTool);
            thread.send(
                ClientUserMessageId::new(),
                ["Now test the echo tool with 'Hello'. Does it work? Say 'Yes' or 'No'."],
                cx,
            )
        })
        .unwrap()
        .collect()
        .await;
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);

    let events = thread
        .update(cx, |thread, cx| {
            thread.remove_tool(&EchoTool::NAME);
            thread.add_tool(DelayTool);
            thread.send(
                ClientUserMessageId::new(),
                [
                    "Now call the delay tool with 200ms.",
                    "When the timer goes off, then you echo the output of the tool.",
                ],
                cx,
            )
        })
        .unwrap()
        .collect()
        .await;
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);
    thread.update(cx, |thread, _cx| {
        assert!(
            thread
                .last_received_or_pending_message()
                .unwrap()
                .as_agent_message()
                .unwrap()
                .content
                .iter()
                .any(|content| {
                    if let AgentMessageContent::Text(text) = content {
                        text.contains("Ding")
                    } else {
                        false
                    }
                }),
            "{}",
            thread.to_markdown()
        );
    });
}

#[gpui::test]
#[cfg_attr(not(feature = "e2e"), ignore)]
async fn test_streaming_tool_calls(cx: &mut TestAppContext) {
    let ThreadTest { thread, .. } = setup(cx, TestModel::Sonnet4).await;

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(WordListTool);
            thread.send(ClientUserMessageId::new(), ["Test the word_list tool."], cx)
        })
        .unwrap();

    let mut saw_partial_tool_use = false;
    while let Some(event) = events.next().await {
        if let Ok(ThreadEvent::ToolCall(tool_call)) = event {
            thread.update(cx, |thread, _cx| {
                let message = thread.last_received_or_pending_message().unwrap();
                let agent_message = message.as_agent_message().unwrap();
                let last_content = agent_message.content.last().unwrap();
                if let AgentMessageContent::ToolUse(last_tool_use) = last_content {
                    assert_eq!(last_tool_use.name.as_ref(), "word_list");
                    if tool_call.status == acp::ToolCallStatus::Pending {
                        if !last_tool_use.is_input_complete
                            && last_tool_use.input.get("g").is_none()
                        {
                            saw_partial_tool_use = true;
                        }
                    } else {
                        last_tool_use
                            .input
                            .get("a")
                            .expect("'a' has streamed because input is now complete");
                        last_tool_use
                            .input
                            .get("g")
                            .expect("'g' has streamed because input is now complete");
                    }
                } else {
                    panic!("last content should be a tool use");
                }
            });
        }
    }

    assert!(
        saw_partial_tool_use,
        "should see at least one partially streamed tool use in the history"
    );
}

#[gpui::test]
async fn test_tool_authorization(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(ToolRequiringPermission);
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_1".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_2".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    let tool_call_auth_1 = next_tool_call_authorization(&mut events).await;
    let tool_call_auth_2 = next_tool_call_authorization(&mut events).await;

    tool_call_auth_1
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();
    cx.run_until_parked();

    tool_call_auth_2
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("deny"),
            acp::PermissionOptionKind::RejectOnce,
        ))
        .unwrap();
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    assert_eq!(
        message.content,
        vec![
            language_model::MessageContent::ToolResult(LanguageModelToolResult {
                tool_use_id: tool_call_auth_1.tool_call.tool_call_id.0.to_string().into(),
                tool_name: ToolRequiringPermission::NAME.into(),
                is_error: false,
                content: vec!["Allowed".into()],
                output: Some("Allowed".into())
            }),
            language_model::MessageContent::ToolResult(LanguageModelToolResult {
                tool_use_id: tool_call_auth_2.tool_call.tool_call_id.0.to_string().into(),
                tool_name: ToolRequiringPermission::NAME.into(),
                is_error: true,
                content: vec!["Permission to run tool denied by user".into()],
                output: Some("Permission to run tool denied by user".into())
            })
        ]
    );

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_3".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    let tool_call_auth_3 = next_tool_call_authorization(&mut events).await;
    tool_call_auth_3
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("always_allow:tool_requiring_permission"),
            acp::PermissionOptionKind::AllowAlways,
        ))
        .unwrap();
    cx.run_until_parked();
    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    assert_eq!(
        message.content,
        vec![language_model::MessageContent::ToolResult(
            LanguageModelToolResult {
                tool_use_id: tool_call_auth_3.tool_call.tool_call_id.0.to_string().into(),
                tool_name: ToolRequiringPermission::NAME.into(),
                is_error: false,
                content: vec!["Allowed".into()],
                output: Some("Allowed".into())
            }
        )]
    );

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_4".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();
    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    assert_eq!(
        message.content,
        vec![language_model::MessageContent::ToolResult(
            LanguageModelToolResult {
                tool_use_id: "tool_id_4".into(),
                tool_name: ToolRequiringPermission::NAME.into(),
                is_error: false,
                content: vec!["Allowed".into()],
                output: Some("Allowed".into())
            }
        )]
    );
}

#[gpui::test]
async fn test_tool_hallucination(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_1".into(),
            name: "nonexistent_tool".into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    let tool_call = expect_tool_call(&mut events).await;
    assert_eq!(tool_call.title, "nonexistent_tool");
    assert_eq!(tool_call.status, acp::ToolCallStatus::Pending);
    let update = expect_tool_call_update_fields(&mut events).await;
    assert_eq!(update.fields.status, Some(acp::ToolCallStatus::Failed));
}

#[gpui::test]
#[cfg_attr(not(feature = "e2e"), ignore)]
async fn test_concurrent_tool_calls(cx: &mut TestAppContext) {
    let ThreadTest { thread, .. } = setup(cx, TestModel::Sonnet4).await;

    let events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(DelayTool);
            thread.send(
                ClientUserMessageId::new(),
                [
                    "Call the delay tool twice in the same message.",
                    "Once with 100ms. Once with 300ms.",
                    "When both timers are complete, describe the outputs.",
                ],
                cx,
            )
        })
        .unwrap()
        .collect()
        .await;

    let stop_reasons = stop_events(events);
    assert_eq!(stop_reasons, vec![acp::StopReason::EndTurn]);

    thread.update(cx, |thread, _cx| {
        let last_message = thread.last_received_or_pending_message().unwrap();
        let agent_message = last_message.as_agent_message().unwrap();
        let text = agent_message
            .content
            .iter()
            .filter_map(|content| {
                if let AgentMessageContent::Text(text) = content {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<String>();

        assert!(text.contains("Ding"));
    });
}

#[gpui::test]
async fn test_profiles(cx: &mut TestAppContext) {
    let ThreadTest {
        model, thread, fs, ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(DelayTool);
        thread.add_tool(EchoTool);
        thread.add_tool(InfiniteTool);
    });

    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "profiles": {
                    "test-1": {
                        "name": "Test Profile 1",
                        "tools": {
                            EchoTool::NAME: true,
                            DelayTool::NAME: true,
                        }
                    },
                    "test-2": {
                        "name": "Test Profile 2",
                        "tools": {
                            InfiniteTool::NAME: true,
                        }
                    }
                }
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    cx.run_until_parked();

    thread
        .update(cx, |thread, cx| {
            thread.set_profile(AgentProfileId("test-1".into()), cx);
            thread.send(ClientUserMessageId::new(), ["test"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let mut pending_completions = fake_model.pending_completions();
    assert_eq!(pending_completions.len(), 1);
    let completion = pending_completions.pop().unwrap();
    let tool_names: Vec<String> = completion
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect();
    assert_eq!(tool_names, vec![DelayTool::NAME, EchoTool::NAME]);
    fake_model.end_last_completion_stream();

    thread
        .update(cx, |thread, cx| {
            thread.set_profile(AgentProfileId("test-2".into()), cx);
            thread.send(ClientUserMessageId::new(), ["test2"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    let mut pending_completions = fake_model.pending_completions();
    assert_eq!(pending_completions.len(), 1);
    let completion = pending_completions.pop().unwrap();
    let tool_names: Vec<String> = completion
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect();
    assert_eq!(tool_names, vec![InfiniteTool::NAME]);
}
