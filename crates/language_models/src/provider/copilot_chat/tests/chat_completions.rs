use super::*;

#[test]
fn chat_completions_stream_maps_reasoning_data() {
    use copilot_chat::{
        FunctionChunk, ResponseChoice, ResponseDelta, ResponseEvent, Role, ToolCallChunk,
    };

    let events = vec![
        ResponseEvent {
            choices: vec![ResponseChoice {
                index: Some(0),
                finish_reason: None,
                delta: Some(ResponseDelta {
                    content: None,
                    role: Some(Role::Assistant),
                    tool_calls: vec![ToolCallChunk {
                        index: Some(0),
                        id: Some("call_abc123".to_string()),
                        function: Some(FunctionChunk {
                            name: Some("list_directory".to_string()),
                            arguments: Some("{\"path\":\"test\"}".to_string()),
                            thought_signature: None,
                        }),
                    }],
                    reasoning_opaque: Some("encrypted_reasoning_token_xyz".to_string()),
                    reasoning_text: Some("Let me check the directory".to_string()),
                }),
                message: None,
            }],
            id: "chatcmpl-123".to_string(),
            usage: None,
        },
        ResponseEvent {
            choices: vec![ResponseChoice {
                index: Some(0),
                finish_reason: Some("tool_calls".to_string()),
                delta: Some(ResponseDelta {
                    content: None,
                    role: None,
                    tool_calls: vec![],
                    reasoning_opaque: None,
                    reasoning_text: None,
                }),
                message: None,
            }],
            id: "chatcmpl-123".to_string(),
            usage: None,
        },
    ];

    let mapped = futures::executor::block_on(async {
        map_to_language_model_completion_events(
            Box::pin(futures::stream::iter(events.into_iter().map(Ok))),
            true,
        )
        .collect::<Vec<_>>()
        .await
    });

    let mut has_reasoning_details = false;
    let mut has_tool_use = false;
    let mut reasoning_opaque_value: Option<String> = None;
    let mut reasoning_text_value: Option<String> = None;

    for event_result in mapped {
        match event_result {
            Ok(LanguageModelCompletionEvent::ReasoningDetails(details)) => {
                has_reasoning_details = true;
                reasoning_opaque_value = details
                    .get("reasoning_opaque")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                reasoning_text_value = details
                    .get("reasoning_text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            Ok(LanguageModelCompletionEvent::ToolUse(tool_use)) => {
                has_tool_use = true;
                assert_eq!(tool_use.id.to_string(), "call_abc123");
                assert_eq!(tool_use.name.as_ref(), "list_directory");
            }
            _ => {}
        }
    }

    assert!(
        has_reasoning_details,
        "Should emit ReasoningDetails event for Gemini 3 reasoning"
    );
    assert!(has_tool_use, "Should emit ToolUse event");
    assert_eq!(
        reasoning_opaque_value,
        Some("encrypted_reasoning_token_xyz".to_string()),
        "Should capture reasoning_opaque"
    );
    assert_eq!(
        reasoning_text_value,
        Some("Let me check the directory".to_string()),
        "Should capture reasoning_text"
    );
}
