use super::super::response_helpers::token_usage_from_response_usage;
use super::*;

#[test]
fn responses_stream_maps_text_and_usage() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: response_item_message("msg_123"),
        },
        ResponsesStreamEvent::OutputTextDelta {
            item_id: "msg_123".into(),
            output_index: 0,
            content_index: Some(0),
            delta: "Hello".into(),
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary {
                usage: Some(ResponseUsage {
                    input_tokens: Some(5),
                    input_tokens_details: ResponseInputTokensDetails { cached_tokens: 2 },
                    output_tokens: Some(3),
                    total_tokens: Some(8),
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
    ];

    let mapped = map_response_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::StartMessage { ref message_id } if message_id == "msg_123"
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::Text(ref text) if text == "Hello"
    ));
    assert!(matches!(
        mapped[2],
        LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
            input_tokens: 3,
            output_tokens: 3,
            cache_read_input_tokens: 2,
            ..
        })
    ));
    assert!(matches!(
        mapped[3],
        LanguageModelCompletionEvent::Stop(StopReason::EndTurn)
    ));
}

#[test]
fn response_usage_deserializes_cached_tokens() -> Result<()> {
    let usage: ResponseUsage = serde_json::from_value(json!({
        "input_tokens": 5,
        "input_tokens_details": {
            "cached_tokens": 2,
        },
        "output_tokens": 3,
        "output_tokens_details": {
            "reasoning_tokens": 1,
        },
        "total_tokens": 8,
    }))?;

    assert_eq!(usage.output_tokens_details.reasoning_tokens, 1);
    assert_eq!(
        token_usage_from_response_usage(&usage),
        TokenUsage {
            input_tokens: 3,
            output_tokens: 3,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 2,
        }
    );

    Ok(())
}

#[test]
fn responses_stream_maps_tool_calls() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: response_item_function_call("item_fn", Some("{\"city\":\"Bos")),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDelta {
            item_id: "item_fn".into(),
            output_index: 0,
            delta: "ton\"}".into(),
            sequence_number: None,
        },
        ResponsesStreamEvent::FunctionCallArgumentsDone {
            item_id: "item_fn".into(),
            output_index: 0,
            arguments: "{\"city\":\"Boston\"}".into(),
            sequence_number: None,
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    assert_eq!(mapped.len(), 3);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
            is_input_complete: false,
            ..
        })
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
            ref id,
            ref name,
            ref raw_input,
            is_input_complete: true,
            ..
        }) if id.to_string() == "call_123"
            && name.as_ref() == "get_weather"
            && raw_input == "{\"city\":\"Boston\"}"
    ));
    assert!(matches!(
        mapped[2],
        LanguageModelCompletionEvent::Stop(StopReason::ToolUse)
    ));
}

#[test]
fn responses_stream_uses_max_tokens_stop_reason() {
    let events = vec![ResponsesStreamEvent::Incomplete {
        response: ResponseSummary {
            incomplete_details: Some(ResponseIncompleteDetails {
                reason: Some("max_tokens".into()),
            }),
            usage: Some(ResponseUsage {
                input_tokens: Some(10),
                output_tokens: Some(20),
                total_tokens: Some(30),
                ..Default::default()
            }),
            ..Default::default()
        },
    }];

    let mapped = map_response_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
            input_tokens: 10,
            output_tokens: 20,
            ..
        })
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::Stop(StopReason::MaxTokens)
    ));
}

#[test]
fn responses_stream_failed_uses_response_error_message() {
    let mut mapper = OpenAiResponseEventMapper::new();
    let mapped = mapper.map_event(ResponsesStreamEvent::Failed {
        response: ResponseSummary {
            status: Some("failed".into()),
            error: Some(ResponseError {
                code: Some("server_error".into()),
                message: "The model failed to generate a response.".into(),
                param: None,
            }),
            ..Default::default()
        },
    });

    assert_eq!(mapped.len(), 1);
    let error = mapped.into_iter().next().unwrap().unwrap_err();
    assert_eq!(
        error.to_string(),
        "server_error: The model failed to generate a response."
    );
}

#[test]
fn responses_stream_deserializes_documented_error_event() {
    let event = serde_json::from_value::<ResponsesStreamEvent>(json!({
        "type": "error",
        "code": "ERR_SOMETHING",
        "message": "Something went wrong",
        "param": null,
        "sequence_number": 1
    }))
    .expect("documented error event");

    let mut mapper = OpenAiResponseEventMapper::new();
    let mapped = mapper.map_event(event);

    assert_eq!(mapped.len(), 1);
    let error = mapped.into_iter().next().unwrap().unwrap_err();
    assert_eq!(error.to_string(), "ERR_SOMETHING: Something went wrong");
}

#[test]
fn responses_stream_deserializes_nested_error_event() {
    // In practice the Responses API often nests error fields under an
    // `error` object even though the public spec documents them at the top
    // level. Make sure we don't lose the message and code in that case.
    let event = serde_json::from_value::<ResponsesStreamEvent>(json!({
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "code": "context_length_exceeded",
                "message": "Your input exceeds the context window of this model. Please adjust your input and try again.",
                "param": "input"
            },
            "sequence_number": 2
        }))
        .expect("nested error event");

    let mut mapper = OpenAiResponseEventMapper::new();
    let mapped = mapper.map_event(event);

    assert_eq!(mapped.len(), 1);
    let error = mapped.into_iter().next().unwrap().unwrap_err();
    assert_eq!(
        error.to_string(),
        "context_length_exceeded: Your input exceeds the context window of this model. Please adjust your input and try again."
    );
}

#[test]
fn responses_stream_deserializes_response_error_event() {
    let event = serde_json::from_value::<ResponsesStreamEvent>(json!({
        "type": "response.error",
        "error": {
            "code": "invalid_request_error",
            "message": "Invalid request."
        }
    }))
    .expect("response error event");

    let mut mapper = OpenAiResponseEventMapper::new();
    let mapped = mapper.map_event(event);

    assert_eq!(mapped.len(), 1);
    let error = mapped.into_iter().next().unwrap().unwrap_err();
    assert_eq!(error.to_string(), "invalid_request_error: Invalid request.");
}

#[test]
fn responses_stream_maps_refusal_events_to_refusal_stop() {
    let delta = serde_json::from_value::<ResponsesStreamEvent>(json!({
        "type": "response.refusal.delta",
        "item_id": "msg_123",
        "output_index": 0,
        "content_index": 0,
        "delta": "I can't help",
        "sequence_number": 1
    }))
    .expect("documented refusal delta event");
    let done = serde_json::from_value::<ResponsesStreamEvent>(json!({
        "type": "response.refusal.done",
        "item_id": "msg_123",
        "output_index": 0,
        "content_index": 0,
        "refusal": "I can't help with that.",
        "sequence_number": 2
    }))
    .expect("documented refusal done event");

    let mapped = map_response_events(vec![
        delta,
        done,
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ]);

    assert_eq!(mapped.len(), 1);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::Stop(StopReason::Refusal)
    ));
}

#[test]
fn responses_stream_maps_refusal_output_to_refusal_stop() {
    let mapped = map_response_events(vec![ResponsesStreamEvent::Completed {
        response: ResponseSummary {
            output: vec![ResponseOutputItem::Message(ResponseOutputMessage {
                id: Some("msg_123".into()),
                role: Some("assistant".into()),
                status: Some("completed".into()),
                content: vec![json!({
                    "type": "refusal",
                    "refusal": "I can't help with that."
                })],
                phase: None,
            })],
            ..Default::default()
        },
    }]);

    assert_eq!(mapped.len(), 1);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::Stop(StopReason::Refusal)
    ));
}
