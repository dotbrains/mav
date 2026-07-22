mod chat_completions;
mod chat_completions;

use super::*;
use copilot_chat::responses;
use futures::StreamExt;
use serde_json::json;

fn map_events(events: Vec<responses::StreamEvent>) -> Vec<LanguageModelCompletionEvent> {
    futures::executor::block_on(async {
        CopilotResponsesEventMapper::new()
            .map_stream(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(Result::unwrap)
            .collect()
    })
}

fn test_responses_model() -> CopilotChatModel {
    serde_json::from_value(json!({
        "billing": {
            "is_premium": false,
            "multiplier": 1.0
        },
        "capabilities": {
            "family": "test",
            "limits": {
                "max_context_window_tokens": 128000,
                "max_output_tokens": 4096
            },
            "supports": {
                "streaming": true,
                "tool_calls": true,
                "parallel_tool_calls": false,
                "vision": false
            },
            "type": "chat"
        },
        "id": "test-model",
        "is_chat_default": false,
        "is_chat_fallback": false,
        "model_picker_enabled": true,
        "name": "Test Model",
        "vendor": "OpenAI",
        "supported_endpoints": ["/responses"]
    }))
    .expect("valid test model")
}

#[test]
fn responses_stream_maps_text_and_usage() {
    let events = vec![
        responses::StreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: responses::ResponseOutputItem::Message {
                id: "msg_1".into(),
                role: "assistant".into(),
                content: Some(Vec::new()),
            },
        },
        responses::StreamEvent::OutputTextDelta {
            item_id: "msg_1".into(),
            output_index: 0,
            delta: "Hello".into(),
        },
        responses::StreamEvent::Completed {
            response: responses::Response {
                usage: Some(responses::ResponseUsage {
                    input_tokens: Some(5),
                    output_tokens: Some(3),
                    total_tokens: Some(8),
                }),
                ..Default::default()
            },
        },
    ];

    let mapped = map_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::StartMessage { ref message_id } if message_id == "msg_1"
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::Text(ref text) if text == "Hello"
    ));
    assert!(matches!(
        mapped[2],
        LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
            input_tokens: 5,
            output_tokens: 3,
            ..
        })
    ));
    assert!(matches!(
        mapped[3],
        LanguageModelCompletionEvent::Stop(StopReason::EndTurn)
    ));
}

#[test]
fn responses_stream_maps_tool_calls() {
    let events = vec![responses::StreamEvent::OutputItemDone {
        output_index: 0,
        sequence_number: None,
        item: responses::ResponseOutputItem::FunctionCall {
            id: Some("fn_1".into()),
            call_id: "call_1".into(),
            name: "do_it".into(),
            arguments: "{\"x\":1}".into(),
            status: None,
            thought_signature: None,
        },
    }];

    let mapped = map_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::ToolUse(ref use_) if use_.id.to_string() == "call_1" && use_.name.as_ref() == "do_it"
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::Stop(StopReason::ToolUse)
    ));
}

#[test]
fn responses_stream_handles_json_parse_error() {
    let events = vec![responses::StreamEvent::OutputItemDone {
        output_index: 0,
        sequence_number: None,
        item: responses::ResponseOutputItem::FunctionCall {
            id: Some("fn_1".into()),
            call_id: "call_1".into(),
            name: "do_it".into(),
            arguments: "{not json}".into(),
            status: None,
            thought_signature: None,
        },
    }];

    let mapped = map_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::ToolUseJsonParseError { ref id, ref tool_name, .. }
            if id.to_string() == "call_1" && tool_name.as_ref() == "do_it"
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::Stop(StopReason::ToolUse)
    ));
}

#[test]
fn responses_stream_maps_reasoning_summary_and_encrypted_content() {
    let events = vec![responses::StreamEvent::OutputItemDone {
        output_index: 0,
        sequence_number: None,
        item: responses::ResponseOutputItem::Reasoning {
            id: "r1".into(),
            summary: Some(vec![responses::ResponseReasoningItem {
                kind: "summary_text".into(),
                text: "Chain".into(),
            }]),
            encrypted_content: Some("ENC".into()),
        },
    }];

    let mapped = map_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::Thinking { ref text, signature: None } if text == "Chain"
    ));
    match &mapped[1] {
        LanguageModelCompletionEvent::ReasoningDetails(details) => assert_eq!(
            details,
            &json!({
                "reasoning_items": [
                    {
                        "id": "r1",
                        "summary": [],
                        "encrypted_content": "ENC"
                    }
                ]
            })
        ),
        other => panic!("expected reasoning details, got {other:?}"),
    }
}

#[test]
fn responses_stream_ignores_reasoning_items_repeated_in_completed_output() {
    let events = vec![
        responses::StreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item: responses::ResponseOutputItem::Reasoning {
                id: "r1".into(),
                summary: Some(Vec::new()),
                encrypted_content: Some("ENC1".into()),
            },
        },
        responses::StreamEvent::Completed {
            response: responses::Response {
                output: vec![
                    responses::ResponseOutputItem::Reasoning {
                        id: "r1".into(),
                        summary: Some(Vec::new()),
                        encrypted_content: Some("ENC1".into()),
                    },
                    responses::ResponseOutputItem::Reasoning {
                        id: "r2".into(),
                        summary: Some(Vec::new()),
                        encrypted_content: Some("ENC2".into()),
                    },
                ],
                ..Default::default()
            },
        },
    ];

    let mapped = map_events(events);
    let reasoning_details = mapped
        .iter()
        .filter_map(|event| match event {
            LanguageModelCompletionEvent::ReasoningDetails(details) => Some(details),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        reasoning_details,
        vec![&json!({
            "reasoning_items": [
                {
                    "id": "r1",
                    "summary": [],
                    "encrypted_content": "ENC1"
                }
            ]
        })]
    );
}

#[test]
fn into_copilot_responses_replays_reasoning_details() {
    let model = test_responses_model();
    let request = LanguageModelRequest {
        messages: vec![LanguageModelRequestMessage {
            role: Role::Assistant,
            content: vec![
                MessageContent::RedactedThinking("legacy-redacted".into()),
                MessageContent::Text("Done".into()),
            ],
            cache: false,
            reasoning_details: Some(Arc::new(json!({
                "reasoning_items": [
                    {
                        "id": "r1",
                        "summary": [
                            {
                                "type": "summary_text",
                                "text": "Chain"
                            }
                        ],
                        "encrypted_content": "ENC"
                    }
                ]
            }))),
        }],
        ..Default::default()
    };

    let serialized =
        serde_json::to_value(into_copilot_responses(&model, request)).expect("serialized request");
    let input = serialized["input"].as_array().expect("input items");

    assert_eq!(
        input.first(),
        Some(&json!({
            "type": "reasoning",
            "id": "r1",
            "summary": [],
            "encrypted_content": "ENC"
        }))
    );
    assert_eq!(
        input.get(1),
        Some(&json!({
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "output_text",
                    "text": "Done"
                }
            ],
            "status": "completed"
        }))
    );
    assert!(!serialized.to_string().contains("legacy-redacted"));
}

#[test]
fn responses_stream_handles_incomplete_max_tokens() {
    let events = vec![responses::StreamEvent::Incomplete {
        response: responses::Response {
            usage: Some(responses::ResponseUsage {
                input_tokens: Some(10),
                output_tokens: Some(0),
                total_tokens: Some(10),
            }),
            incomplete_details: Some(responses::IncompleteDetails {
                reason: Some(responses::IncompleteReason::MaxOutputTokens),
            }),
            ..Default::default()
        },
    }];

    let mapped = map_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
            input_tokens: 10,
            output_tokens: 0,
            ..
        })
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::Stop(StopReason::MaxTokens)
    ));
}

#[test]
fn responses_stream_handles_incomplete_content_filter() {
    let events = vec![responses::StreamEvent::Incomplete {
        response: responses::Response {
            usage: None,
            incomplete_details: Some(responses::IncompleteDetails {
                reason: Some(responses::IncompleteReason::ContentFilter),
            }),
            ..Default::default()
        },
    }];

    let mapped = map_events(events);
    assert!(matches!(
        mapped.last().unwrap(),
        LanguageModelCompletionEvent::Stop(StopReason::Refusal)
    ));
}

#[test]
fn responses_stream_completed_no_duplicate_after_tool_use() {
    let events = vec![
        responses::StreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item: responses::ResponseOutputItem::FunctionCall {
                id: Some("fn_1".into()),
                call_id: "call_1".into(),
                name: "do_it".into(),
                arguments: "{}".into(),
                status: None,
                thought_signature: None,
            },
        },
        responses::StreamEvent::Completed {
            response: responses::Response::default(),
        },
    ];

    let mapped = map_events(events);

    let mut stop_count = 0usize;
    let mut saw_tool_use_stop = false;
    for event in mapped {
        if let LanguageModelCompletionEvent::Stop(reason) = event {
            stop_count += 1;
            if matches!(reason, StopReason::ToolUse) {
                saw_tool_use_stop = true;
            }
        }
    }
    assert_eq!(stop_count, 1, "should emit exactly one Stop event");
    assert!(saw_tool_use_stop, "Stop reason should be ToolUse");
}

#[test]
fn responses_stream_failed_maps_http_response_error() {
    let events = vec![responses::StreamEvent::Failed {
        response: responses::Response {
            error: Some(responses::ResponseError {
                code: "429".into(),
                message: "too many requests".into(),
            }),
            ..Default::default()
        },
    }];

    let mapped_results = futures::executor::block_on(async {
        CopilotResponsesEventMapper::new()
            .map_stream(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
            .collect::<Vec<_>>()
            .await
    });

    assert_eq!(mapped_results.len(), 1);
    match &mapped_results[0] {
        Err(LanguageModelCompletionError::HttpResponseError {
            status_code,
            message,
            ..
        }) => {
            assert_eq!(*status_code, http_client::StatusCode::TOO_MANY_REQUESTS);
            assert_eq!(message, "too many requests");
        }
        other => panic!("expected HttpResponseError, got {:?}", other),
    }
}
