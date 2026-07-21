use super::*;
use gpui::TestAppContext;
use serde_json::json;

struct ReplayImageTool;

impl AgentTool for ReplayImageTool {
    type Input = ();
    type Output = String;

    const NAME: &'static str = "registered_image_tool";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Other
    }

    fn initial_title(
        &self,
        _input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        "Registered Image Tool".into()
    }

    fn run(
        self: Arc<Self>,
        _input: ToolInput<Self::Input>,
        _event_stream: ToolCallEventStream,
        _cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        Task::ready(Ok(String::new()))
    }
}

#[gpui::test]
async fn test_replay_tool_call_replays_image_content(cx: &mut TestAppContext) {
    let (thread, _event_stream) = tests::setup_thread_for_test(cx).await;

    let registered_tool_use_id = LanguageModelToolUseId::from("registered_tool_id");
    let missing_tool_use_id = LanguageModelToolUseId::from("missing_tool_id");
    let image_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
    let image = LanguageModelImage {
        source: image_data.into(),
    };

    let mut replay_events = cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.add_tool(ReplayImageTool);

            let registered_tool_use = LanguageModelToolUse {
                id: registered_tool_use_id.clone(),
                name: ReplayImageTool::NAME.into(),
                raw_input: "null".to_string(),
                input: json!(null),
                is_input_complete: true,
                thought_signature: None,
            };
            let missing_tool_use = LanguageModelToolUse {
                id: missing_tool_use_id.clone(),
                name: "missing_image_tool".into(),
                raw_input: "{}".to_string(),
                input: json!({}),
                is_input_complete: true,
                thought_signature: None,
            };

            let mut tool_results = IndexMap::default();
            tool_results.insert(
                registered_tool_use_id.clone(),
                LanguageModelToolResult {
                    tool_use_id: registered_tool_use_id.clone(),
                    tool_name: ReplayImageTool::NAME.into(),
                    is_error: false,
                    content: vec![
                        LanguageModelToolResultContent::Text("before".into()),
                        LanguageModelToolResultContent::Image(image.clone()),
                        LanguageModelToolResultContent::Text("after".into()),
                    ],
                    output: Some(json!("raw output")),
                },
            );
            tool_results.insert(
                missing_tool_use_id.clone(),
                LanguageModelToolResult {
                    tool_use_id: missing_tool_use_id.clone(),
                    tool_name: "missing_image_tool".into(),
                    is_error: false,
                    content: vec![LanguageModelToolResultContent::Image(image.clone())],
                    output: Some(json!("raw output")),
                },
            );

            thread.messages.push(Arc::new(Message::Agent(AgentMessage {
                content: vec![
                    AgentMessageContent::ToolUse(registered_tool_use),
                    AgentMessageContent::ToolUse(missing_tool_use),
                ],
                tool_results,
                reasoning_details: None,
            })));

            thread.replay(cx)
        })
    });

    let mut tool_use_ids_with_image_content = HashSet::default();
    while let Some(event) = replay_events.next().await {
        let event = event.unwrap();
        if let ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(update)) = event
            && let Some(content) = &update.fields.content
            && content.iter().any(|content| {
                matches!(
                    content,
                    acp::ToolCallContent::Content(acp::Content {
                        content: acp::ContentBlock::Image(_),
                        ..
                    })
                )
            })
        {
            tool_use_ids_with_image_content.insert(update.tool_call_id.to_string());
        }
    }

    assert!(tool_use_ids_with_image_content.contains(&registered_tool_use_id.to_string()));
    assert!(tool_use_ids_with_image_content.contains(&missing_tool_use_id.to_string()));
}

#[gpui::test]
async fn test_handle_tool_use_json_parse_error_adds_tool_use_to_content(cx: &mut TestAppContext) {
    let (thread, event_stream) = tests::setup_thread_for_test(cx).await;

    let tool_use_id = LanguageModelToolUseId::from("test_tool_id");
    let tool_name: Arc<str> = Arc::from("test_tool");
    let raw_input: Arc<str> = Arc::from("{invalid json");
    let json_parse_error = "expected value at line 1 column 1".to_string();

    let (_cancellation_tx, cancellation_rx) = watch::channel(false);

    let result = cx
        .update(|cx| {
            thread.update(cx, |thread, cx| {
                thread
                    .handle_tool_use_json_parse_error_event(
                        tool_use_id.clone(),
                        tool_name.clone(),
                        raw_input.clone(),
                        json_parse_error,
                        &event_stream,
                        cancellation_rx,
                        cx,
                    )
                    .unwrap()
            })
        })
        .await;

    assert!(result.is_error);
    assert_eq!(result.tool_use_id, tool_use_id);
    assert_eq!(result.tool_name, tool_name);
    assert!(matches!(
        result.content.as_slice(),
        [LanguageModelToolResultContent::Text(_)]
    ));

    thread.update(cx, |thread, _cx| {
        {
            let last_message = thread.pending_message();
            assert_eq!(
                last_message.content.len(),
                1,
                "Should have one tool_use in content"
            );

            match &last_message.content[0] {
                AgentMessageContent::ToolUse(tool_use) => {
                    assert_eq!(tool_use.id, tool_use_id);
                    assert_eq!(tool_use.name, tool_name);
                    assert_eq!(tool_use.raw_input, raw_input.to_string());
                    assert!(tool_use.is_input_complete);
                    assert_eq!(tool_use.input, json!({}));
                }
                _ => panic!("Expected ToolUse content"),
            }
        }

        thread
            .pending_message()
            .tool_results
            .insert(result.tool_use_id.clone(), result);

        let last_message = thread.pending_message();
        assert_eq!(
            last_message.tool_results.len(),
            1,
            "Should have one tool_result"
        );
        assert!(last_message.tool_results.contains_key(&tool_use_id));
    })
}
