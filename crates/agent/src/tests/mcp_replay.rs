use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_mcp_tool_result_displayed_when_server_disconnected(cx: &mut TestAppContext) {
    let ThreadTest {
        model,
        thread,
        context_server_store,
        fs,
        ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    // Setup settings to allow MCP tools
    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "always_allow_tool_actions": true,
                "profiles": {
                    "test": {
                        "name": "Test Profile",
                        "enable_all_context_servers": true,
                        "tools": {}
                    },
                }
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    cx.run_until_parked();
    thread.update(cx, |thread, cx| {
        thread.set_profile(AgentProfileId("test".into()), cx)
    });

    // Setup a context server with a tool
    let mut mcp_tool_calls = setup_context_server(
        "github_server",
        vec![context_server::types::Tool {
            name: "issue_read".into(),
            title: None,
            description: Some("Read a GitHub issue".into()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "issue_url": { "type": "string" }
                }
            }),
            output_schema: None,
            annotations: None,
        }],
        &context_server_store,
        cx,
    );

    // Send a message and have the model call the MCP tool
    let events = thread.update(cx, |thread, cx| {
        thread
            .send(ClientUserMessageId::new(), ["Read issue #47404"], cx)
            .unwrap()
    });
    cx.run_until_parked();

    // Verify the MCP tool is available to the model
    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        tool_names_for_completion(&completion),
        vec!["issue_read"],
        "MCP tool should be available"
    );

    // Simulate the model calling the MCP tool
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "issue_read".into(),
            raw_input: json!({"issue_url": "https://github.com/mav-industries/mav/issues/47404"})
                .to_string(),
            input: json!({"issue_url": "https://github.com/mav-industries/mav/issues/47404"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // The MCP server receives the tool call and responds with content
    let expected_tool_output = "Issue #47404: Tool call results are cleared upon app close";
    let (tool_call_params, tool_call_response) = mcp_tool_calls.next().await.unwrap();
    assert_eq!(tool_call_params.name, "issue_read");
    tool_call_response
        .send(context_server::types::CallToolResponse {
            content: vec![context_server::types::ToolResponseContent::Text {
                text: expected_tool_output.into(),
            }],
            is_error: None,
            meta: None,
            structured_content: None,
        })
        .unwrap();
    cx.run_until_parked();

    // After tool completes, the model continues with a new completion request
    // that includes the tool results. We need to respond to this.
    let _completion = fake_model.pending_completions().pop().unwrap();
    fake_model.send_last_completion_stream_text_chunk("I found the issue!");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();
    events.collect::<Vec<_>>().await;

    // Verify the tool result is stored in the thread by checking the markdown output.
    // The tool result is in the first assistant message (not the last one, which is
    // the model's response after the tool completed).
    thread.update(cx, |thread, _cx| {
        let markdown = thread.to_markdown();
        assert!(
            markdown.contains("**Tool Result**: issue_read"),
            "Thread should contain tool result header"
        );
        assert!(
            markdown.contains(expected_tool_output),
            "Thread should contain tool output: {}",
            expected_tool_output
        );
    });

    // Simulate app restart: disconnect the MCP server.
    // After restart, the MCP server won't be connected yet when the thread is replayed.
    context_server_store.update(cx, |store, cx| {
        let _ = store.stop_server(&ContextServerId("github_server".into()), cx);
    });
    cx.run_until_parked();

    // Replay the thread (this is what happens when loading a saved thread)
    let mut replay_events = thread.update(cx, |thread, cx| thread.replay(cx));

    let mut found_tool_call = None;
    let mut found_tool_call_update_with_output = None;

    while let Some(event) = replay_events.next().await {
        let event = event.unwrap();
        match &event {
            ThreadEvent::ToolCall(tc) if tc.tool_call_id.to_string() == "tool_1" => {
                found_tool_call = Some(tc.clone());
            }
            ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(update))
                if update.tool_call_id.to_string() == "tool_1" =>
            {
                if update.fields.raw_output.is_some() {
                    found_tool_call_update_with_output = Some(update.clone());
                }
            }
            _ => {}
        }
    }

    // The tool call should be found
    assert!(
        found_tool_call.is_some(),
        "Tool call should be emitted during replay"
    );

    assert!(
        found_tool_call_update_with_output.is_some(),
        "ToolCallUpdate with raw_output should be emitted even when MCP server is disconnected."
    );

    let update = found_tool_call_update_with_output.unwrap();
    assert_eq!(
        update.fields.raw_output,
        Some(expected_tool_output.into()),
        "raw_output should contain the saved tool result"
    );

    // Also verify the status is correct (completed, not failed)
    assert_eq!(
        update.fields.status,
        Some(acp::ToolCallStatus::Completed),
        "Tool call status should reflect the original completion status"
    );
}

#[gpui::test]
async fn test_mcp_tool_truncation(cx: &mut TestAppContext) {
    let ThreadTest {
        model,
        thread,
        context_server_store,
        fs,
        ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    // Set up a profile with all tools enabled
    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "profiles": {
                    "test": {
                        "name": "Test Profile",
                        "enable_all_context_servers": true,
                        "tools": {
                            EchoTool::NAME: true,
                            DelayTool::NAME: true,
                            WordListTool::NAME: true,
                            ToolRequiringPermission::NAME: true,
                            InfiniteTool::NAME: true,
                        }
                    },
                }
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    cx.run_until_parked();

    thread.update(cx, |thread, cx| {
        thread.set_profile(AgentProfileId("test".into()), cx);
        thread.add_tool(EchoTool);
        thread.add_tool(DelayTool);
        thread.add_tool(WordListTool);
        thread.add_tool(ToolRequiringPermission);
        thread.add_tool(InfiniteTool);
    });

    // Set up multiple context servers with some overlapping tool names
    let _server1_calls = setup_context_server(
        "xxx",
        vec![
            context_server::types::Tool {
                name: "echo".into(), // Conflicts with native EchoTool
                title: None,
                description: None,
                input_schema: serde_json::to_value(EchoTool::input_schema(
                    LanguageModelToolSchemaFormat::JsonSchema,
                ))
                .unwrap(),
                output_schema: None,
                annotations: None,
            },
            context_server::types::Tool {
                name: "unique_tool_1".into(),
                title: None,
                description: None,
                input_schema: json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: None,
            },
        ],
        &context_server_store,
        cx,
    );

    let _server2_calls = setup_context_server(
        "yyy",
        vec![
            context_server::types::Tool {
                name: "echo".into(), // Also conflicts with native EchoTool
                title: None,
                description: None,
                input_schema: serde_json::to_value(EchoTool::input_schema(
                    LanguageModelToolSchemaFormat::JsonSchema,
                ))
                .unwrap(),
                output_schema: None,
                annotations: None,
            },
            context_server::types::Tool {
                name: "unique_tool_2".into(),
                title: None,
                description: None,
                input_schema: json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: None,
            },
            context_server::types::Tool {
                name: "a".repeat(MAX_TOOL_NAME_LENGTH - 2),
                title: None,
                description: None,
                input_schema: json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: None,
            },
            context_server::types::Tool {
                name: "b".repeat(MAX_TOOL_NAME_LENGTH - 1),
                title: None,
                description: None,
                input_schema: json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: None,
            },
        ],
        &context_server_store,
        cx,
    );
    let _server3_calls = setup_context_server(
        "zzz",
        vec![
            context_server::types::Tool {
                name: "a".repeat(MAX_TOOL_NAME_LENGTH - 2),
                title: None,
                description: None,
                input_schema: json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: None,
            },
            context_server::types::Tool {
                name: "b".repeat(MAX_TOOL_NAME_LENGTH - 1),
                title: None,
                description: None,
                input_schema: json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: None,
            },
            context_server::types::Tool {
                name: "c".repeat(MAX_TOOL_NAME_LENGTH + 1),
                title: None,
                description: None,
                input_schema: json!({"type": "object", "properties": {}}),
                output_schema: None,
                annotations: None,
            },
        ],
        &context_server_store,
        cx,
    );

    // Server with spaces in name - tests snake_case conversion for API compatibility
    let _server4_calls = setup_context_server(
        "Azure DevOps",
        vec![context_server::types::Tool {
            name: "echo".into(), // Also conflicts - will be disambiguated as azure_dev_ops_echo
            title: None,
            description: None,
            input_schema: serde_json::to_value(EchoTool::input_schema(
                LanguageModelToolSchemaFormat::JsonSchema,
            ))
            .unwrap(),
            output_schema: None,
            annotations: None,
        }],
        &context_server_store,
        cx,
    );

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Go"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        tool_names_for_completion(&completion),
        vec![
            "azure_dev_ops_echo",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "delay",
            "echo",
            "infinite",
            "tool_requiring_permission",
            "unique_tool_1",
            "unique_tool_2",
            "word_list",
            "xxx_echo",
            "y_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "yyy_echo",
            "z_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ]
    );
}
