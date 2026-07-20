use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_mcp_tools(cx: &mut TestAppContext) {
    let ThreadTest {
        model,
        thread,
        context_server_store,
        fs,
        ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    // Override profiles and wait for settings to be loaded.
    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "tool_permissions": { "default": "allow" },
                "profiles": {
                    "test": {
                        "name": "Test Profile",
                        "enable_all_context_servers": true,
                        "tools": {
                            EchoTool::NAME: true,
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
        thread.set_profile(AgentProfileId("test".into()), cx)
    });

    let mut mcp_tool_calls = setup_context_server(
        "test_server",
        vec![context_server::types::Tool {
            name: "echo".into(),
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

    let events = thread.update(cx, |thread, cx| {
        thread
            .send(ClientUserMessageId::new(), ["Hey"], cx)
            .unwrap()
    });
    cx.run_until_parked();

    // Simulate the model calling the MCP tool.
    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(tool_names_for_completion(&completion), vec!["echo"]);
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "echo".into(),
            raw_input: json!({"text": "test"}).to_string(),
            input: json!({"text": "test"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let (tool_call_params, tool_call_response) = mcp_tool_calls.next().await.unwrap();
    assert_eq!(tool_call_params.name, "echo");
    assert_eq!(tool_call_params.arguments, Some(json!({"text": "test"})));
    tool_call_response
        .send(context_server::types::CallToolResponse {
            content: vec![context_server::types::ToolResponseContent::Text {
                text: "test".into(),
            }],
            is_error: None,
            meta: None,
            structured_content: None,
        })
        .unwrap();
    cx.run_until_parked();

    assert_eq!(tool_names_for_completion(&completion), vec!["echo"]);
    fake_model.send_last_completion_stream_text_chunk("Done!");
    fake_model.end_last_completion_stream();
    events.collect::<Vec<_>>().await;

    // Send again after adding the echo tool, ensuring the name collision is resolved.
    let events = thread.update(cx, |thread, cx| {
        thread.add_tool(EchoTool);
        thread.send(ClientUserMessageId::new(), ["Go"], cx).unwrap()
    });
    cx.run_until_parked();
    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        tool_names_for_completion(&completion),
        vec!["echo", "test_server_echo"]
    );
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_2".into(),
            name: "test_server_echo".into(),
            raw_input: json!({"text": "mcp"}).to_string(),
            input: json!({"text": "mcp"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_3".into(),
            name: "echo".into(),
            raw_input: json!({"text": "native"}).to_string(),
            input: json!({"text": "native"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let (tool_call_params, tool_call_response) = mcp_tool_calls.next().await.unwrap();
    assert_eq!(tool_call_params.name, "echo");
    assert_eq!(tool_call_params.arguments, Some(json!({"text": "mcp"})));
    tool_call_response
        .send(context_server::types::CallToolResponse {
            content: vec![context_server::types::ToolResponseContent::Text { text: "mcp".into() }],
            is_error: None,
            meta: None,
            structured_content: None,
        })
        .unwrap();
    cx.run_until_parked();

    // Ensure the tool results were inserted with the correct names.
    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        completion.messages.last().unwrap().content,
        vec![
            MessageContent::ToolResult(LanguageModelToolResult {
                tool_use_id: "tool_3".into(),
                tool_name: "echo".into(),
                is_error: false,
                content: vec!["native".into()],
                output: Some("native".into()),
            },),
            MessageContent::ToolResult(LanguageModelToolResult {
                tool_use_id: "tool_2".into(),
                tool_name: "test_server_echo".into(),
                is_error: false,
                content: vec!["mcp".into()],
                output: Some("mcp".into()),
            },),
        ]
    );
    fake_model.end_last_completion_stream();
    events.collect::<Vec<_>>().await;
}

#[gpui::test]
async fn test_mcp_tool_multi_content_response(cx: &mut TestAppContext) {
    let ThreadTest {
        model,
        thread,
        context_server_store,
        fs,
        ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();
    fake_model.set_supports_images(true);

    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "tool_permissions": { "default": "allow" },
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

    let mut mcp_tool_calls = setup_context_server(
        "screenshot_server",
        vec![context_server::types::Tool {
            name: "screenshot".into(),
            title: None,
            description: None,
            input_schema: json!({"type": "object", "properties": {}}),
            output_schema: None,
            annotations: None,
        }],
        &context_server_store,
        cx,
    );

    let events = thread.update(cx, |thread, cx| {
        thread
            .send(ClientUserMessageId::new(), ["Take a screenshot"], cx)
            .unwrap()
    });
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "screenshot".into(),
            raw_input: json!({}).to_string(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();
    let _ = completion;

    let (tool_call_params, tool_call_response) = mcp_tool_calls.next().await.unwrap();
    assert_eq!(tool_call_params.name, "screenshot");
    let image_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
    tool_call_response
        .send(context_server::types::CallToolResponse {
            content: vec![
                context_server::types::ToolResponseContent::Text {
                    text: "Some text".into(),
                },
                context_server::types::ToolResponseContent::Image {
                    data: image_data.into(),
                    mime_type: "image/png".into(),
                },
                context_server::types::ToolResponseContent::Text {
                    text: "Some more text".into(),
                },
            ],
            is_error: None,
            meta: None,
            structured_content: None,
        })
        .unwrap();
    cx.run_until_parked();

    // Verify the tool result round-trips back to the model as a multi-part Vec.
    let completion = fake_model.pending_completions().pop().unwrap();
    let tool_result = completion
        .messages
        .last()
        .unwrap()
        .content
        .iter()
        .find_map(|c| match c {
            MessageContent::ToolResult(r) => Some(r.clone()),
            _ => None,
        })
        .expect("expected a tool result");
    assert_eq!(tool_result.tool_use_id, "tool_1".into());
    assert_eq!(tool_result.content.len(), 3);
    assert_eq!(
        tool_result.content[0],
        language_model::LanguageModelToolResultContent::Text(Arc::from("Some text"))
    );
    let expected_image =
        language_model::LanguageModelImage::from_base64_image(image_data, "image/png")
            .expect("image conversion should not error")
            .expect("image conversion should succeed");
    assert_eq!(
        tool_result.content[0],
        language_model::LanguageModelToolResultContent::Text(Arc::from("Some text"))
    );
    assert_eq!(
        tool_result.content[1],
        language_model::LanguageModelToolResultContent::Image(expected_image)
    );
    assert_eq!(
        tool_result.content[2],
        language_model::LanguageModelToolResultContent::Text(Arc::from("Some more text"))
    );
    fake_model.end_last_completion_stream();
    events.collect::<Vec<_>>().await;
}
