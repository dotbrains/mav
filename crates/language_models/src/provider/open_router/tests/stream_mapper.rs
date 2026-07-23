use super::super::*;
use open_router::{ChoiceDelta, FunctionChunk, ResponseMessageDelta, ToolCallChunk};

#[gpui::test]
async fn test_reasoning_details_preservation_with_tool_calls() {
    // This test verifies that reasoning_details are properly captured and preserved
    // when a model uses tool calling with reasoning/thinking tokens.
    //
    // The key regression this prevents:
    // - OpenRouter sends multiple reasoning_details updates during streaming
    // - First with actual content (encrypted reasoning data)
    // - Then with empty array on completion
    // - We must NOT overwrite the real data with the empty array

    let mut mapper = OpenRouterEventMapper::new();

    // Simulate the streaming events as they come from OpenRouter/Gemini
    let events = vec![
        // Event 1: Initial reasoning details with text
        ResponseStreamEvent {
            id: Some("response_123".into()),
            created: 1234567890,
            model: "google/gemini-3.1-pro-preview".into(),
            choices: vec![ChoiceDelta {
                index: 0,
                delta: ResponseMessageDelta {
                    role: None,
                    content: None,
                    reasoning: None,
                    tool_calls: None,
                    reasoning_details: Some(serde_json::json!([
                        {
                            "type": "reasoning.text",
                            "text": "Let me analyze this request...",
                            "format": "google-gemini-v1",
                            "index": 0
                        }
                    ])),
                },
                finish_reason: None,
            }],
            usage: None,
        },
        // Event 2: More reasoning details
        ResponseStreamEvent {
            id: Some("response_123".into()),
            created: 1234567890,
            model: "google/gemini-3.1-pro-preview".into(),
            choices: vec![ChoiceDelta {
                index: 0,
                delta: ResponseMessageDelta {
                    role: None,
                    content: None,
                    reasoning: None,
                    tool_calls: None,
                    reasoning_details: Some(serde_json::json!([
                        {
                            "type": "reasoning.encrypted",
                            "data": "EtgDCtUDAdHtim9OF5jm4aeZSBAtl/randomized123",
                            "format": "google-gemini-v1",
                            "index": 0,
                            "id": "tool_call_abc123"
                        }
                    ])),
                },
                finish_reason: None,
            }],
            usage: None,
        },
        // Event 3: Tool call starts
        ResponseStreamEvent {
            id: Some("response_123".into()),
            created: 1234567890,
            model: "google/gemini-3.1-pro-preview".into(),
            choices: vec![ChoiceDelta {
                index: 0,
                delta: ResponseMessageDelta {
                    role: None,
                    content: None,
                    reasoning: None,
                    tool_calls: Some(vec![ToolCallChunk {
                        index: 0,
                        id: Some("tool_call_abc123".into()),
                        function: Some(FunctionChunk {
                            name: Some("list_directory".into()),
                            arguments: Some("{\"path\":\"test\"}".into()),
                            thought_signature: Some("sha256:test_signature_xyz789".into()),
                        }),
                    }]),
                    reasoning_details: None,
                },
                finish_reason: None,
            }],
            usage: None,
        },
        // Event 4: Empty reasoning_details on tool_calls finish
        // This is the critical event - we must not overwrite with this empty array!
        ResponseStreamEvent {
            id: Some("response_123".into()),
            created: 1234567890,
            model: "google/gemini-3.1-pro-preview".into(),
            choices: vec![ChoiceDelta {
                index: 0,
                delta: ResponseMessageDelta {
                    role: None,
                    content: None,
                    reasoning: None,
                    tool_calls: None,
                    reasoning_details: Some(serde_json::json!([])),
                },
                finish_reason: Some("tool_calls".into()),
            }],
            usage: None,
        },
    ];

    // Process all events
    let mut collected_events = Vec::new();
    for event in events {
        let mapped = mapper.map_event(event);
        collected_events.extend(mapped);
    }

    // Verify we got the expected events
    let mut has_tool_use = false;
    let mut reasoning_details_events = Vec::new();
    let mut thought_signature_value = None;

    for event_result in collected_events {
        match event_result {
            Ok(LanguageModelCompletionEvent::ToolUse(tool_use)) => {
                has_tool_use = true;
                assert_eq!(tool_use.id.to_string(), "tool_call_abc123");
                assert_eq!(tool_use.name.as_ref(), "list_directory");
                thought_signature_value = tool_use.thought_signature.clone();
            }
            Ok(LanguageModelCompletionEvent::ReasoningDetails(details)) => {
                reasoning_details_events.push(details);
            }
            _ => {}
        }
    }

    // Assertions
    assert!(has_tool_use, "Should have emitted ToolUse event");
    assert!(
        !reasoning_details_events.is_empty(),
        "Should have emitted ReasoningDetails events"
    );

    // We should have received multiple reasoning_details events (text, encrypted, empty)
    // The agent layer is responsible for keeping only the first non-empty one
    assert!(
        reasoning_details_events.len() >= 2,
        "Should have multiple reasoning_details events from streaming"
    );

    // Verify at least one contains the encrypted data
    let has_encrypted = reasoning_details_events.iter().any(|details| {
        if let serde_json::Value::Array(arr) = details {
            arr.iter().any(|item| {
                item["type"] == "reasoning.encrypted"
                    && item["data"]
                        .as_str()
                        .map_or(false, |s| s.contains("EtgDCtUDAdHtim9OF5jm4aeZSBAtl"))
            })
        } else {
            false
        }
    });
    assert!(
        has_encrypted,
        "Should have at least one reasoning_details with encrypted data"
    );

    // Verify thought_signature was captured
    assert!(
        thought_signature_value.is_some(),
        "Tool use should have thought_signature"
    );
    assert_eq!(
        thought_signature_value.unwrap(),
        "sha256:test_signature_xyz789"
    );
}

#[gpui::test]
async fn test_usage_only_chunk_with_empty_choices_does_not_error() {
    let mut mapper = OpenRouterEventMapper::new();

    let events = mapper.map_event(ResponseStreamEvent {
        id: Some("response_123".into()),
        created: 1234567890,
        model: "google/gemini-3-flash-preview".into(),
        choices: Vec::new(),
        usage: Some(open_router::Usage {
            prompt_tokens: 12,
            completion_tokens: 7,
            total_tokens: 19,
            prompt_tokens_details: Some(open_router::PromptTokensDetails {
                cached_tokens: 5,
                cache_write_tokens: 3,
            }),
        }),
    });

    assert_eq!(events.len(), 1);
    match events.into_iter().next() {
        Some(Ok(LanguageModelCompletionEvent::UsageUpdate(usage))) => {
            assert_eq!(usage.input_tokens, 4);
            assert_eq!(usage.output_tokens, 7);
            assert_eq!(usage.cache_creation_input_tokens, 3);
            assert_eq!(usage.cache_read_input_tokens, 5);
            assert_eq!(usage.total_tokens(), 19);
        }
        other => panic!("Expected usage update event, got: {other:?}"),
    }
}
