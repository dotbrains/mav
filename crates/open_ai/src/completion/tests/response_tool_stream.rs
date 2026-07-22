use super::*;

#[test]
fn responses_stream_handles_multiple_tool_calls() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: response_item_function_call("item_fn1", Some("{\"city\":\"NYC\"}")),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDone {
            item_id: "item_fn1".into(),
            output_index: 0,
            arguments: "{\"city\":\"NYC\"}".into(),
            sequence_number: None,
        },
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 1,
            sequence_number: None,
            item: response_item_function_call("item_fn2", Some("{\"city\":\"LA\"}")),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDone {
            item_id: "item_fn2".into(),
            output_index: 1,
            arguments: "{\"city\":\"LA\"}".into(),
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
        LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse { ref raw_input, .. })
        if raw_input == "{\"city\":\"NYC\"}"
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse { ref raw_input, .. })
        if raw_input == "{\"city\":\"LA\"}"
    ));
    assert!(matches!(
        mapped[2],
        LanguageModelCompletionEvent::Stop(StopReason::ToolUse)
    ));
}

#[test]
fn responses_stream_handles_mixed_text_and_tool_calls() {
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
            delta: "Let me check that".into(),
        },
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 1,
            sequence_number: None,
            item: response_item_function_call("item_fn", Some("{\"query\":\"test\"}")),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDone {
            item_id: "item_fn".into(),
            output_index: 1,
            arguments: "{\"query\":\"test\"}".into(),
            sequence_number: None,
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::StartMessage { .. }
    ));
    assert!(
        matches!(mapped[1], LanguageModelCompletionEvent::Text(ref text) if text == "Let me check that")
    );
    assert!(
        matches!(mapped[2], LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse { ref raw_input, .. }) if raw_input == "{\"query\":\"test\"}")
    );
    assert!(matches!(
        mapped[3],
        LanguageModelCompletionEvent::Stop(StopReason::ToolUse)
    ));
}

#[test]
fn responses_stream_handles_json_parse_error() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: response_item_function_call("item_fn", Some("{invalid json")),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDone {
            item_id: "item_fn".into(),
            output_index: 0,
            arguments: "{invalid json".into(),
            sequence_number: None,
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    assert!(matches!(
        mapped[0],
        LanguageModelCompletionEvent::ToolUseJsonParseError { ref raw_input, .. }
        if raw_input.as_ref() == "{invalid json"
    ));
}

#[test]
fn responses_stream_handles_incomplete_function_call() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: response_item_function_call("item_fn", Some("{\"city\":")),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDelta {
            item_id: "item_fn".into(),
            output_index: 0,
            delta: "\"Boston\"".into(),
            sequence_number: None,
        },
        ResponsesStreamEvent::Incomplete {
            response: ResponseSummary {
                incomplete_details: Some(ResponseIncompleteDetails {
                    reason: Some("max_tokens".into()),
                }),
                output: vec![response_item_function_call(
                    "item_fn",
                    Some("{\"city\":\"Boston\"}"),
                )],
                ..Default::default()
            },
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
    assert!(
        matches!(mapped[1], LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse { ref raw_input, is_input_complete: true, .. }) if raw_input == "{\"city\":\"Boston\"}")
    );
    assert!(matches!(
        mapped[2],
        LanguageModelCompletionEvent::Stop(StopReason::MaxTokens)
    ));
}

#[test]
fn responses_stream_incomplete_does_not_duplicate_tool_calls() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: response_item_function_call("item_fn", Some("{\"city\":\"Boston\"}")),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDone {
            item_id: "item_fn".into(),
            output_index: 0,
            arguments: "{\"city\":\"Boston\"}".into(),
            sequence_number: None,
        },
        ResponsesStreamEvent::Incomplete {
            response: ResponseSummary {
                incomplete_details: Some(ResponseIncompleteDetails {
                    reason: Some("max_tokens".into()),
                }),
                output: vec![response_item_function_call(
                    "item_fn",
                    Some("{\"city\":\"Boston\"}"),
                )],
                ..Default::default()
            },
        },
    ];

    let mapped = map_response_events(events);
    assert_eq!(mapped.len(), 2);
    assert!(
        matches!(mapped[0], LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse { ref raw_input, .. }) if raw_input == "{\"city\":\"Boston\"}")
    );
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::Stop(StopReason::MaxTokens)
    ));
}

#[test]
fn responses_stream_handles_empty_tool_arguments() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: response_item_function_call("item_fn", Some("")),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDone {
            item_id: "item_fn".into(),
            output_index: 0,
            arguments: "".into(),
            sequence_number: None,
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    assert_eq!(mapped.len(), 2);
    assert!(matches!(
        &mapped[0],
        LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
            id, name, raw_input, input, ..
        }) if id.to_string() == "call_123"
            && name.as_ref() == "get_weather"
            && raw_input == ""
            && input.is_object()
            && input.as_object().unwrap().is_empty()
    ));
    assert!(matches!(
        mapped[1],
        LanguageModelCompletionEvent::Stop(StopReason::ToolUse)
    ));
}

#[test]
fn responses_stream_emits_partial_tool_use_events() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::FunctionCall(crate::responses::ResponseFunctionToolCall {
                id: Some("item_fn".to_string()),
                status: Some("in_progress".to_string()),
                name: Some("get_weather".to_string()),
                call_id: Some("call_abc".to_string()),
                arguments: String::new(),
            }),
        },
        ResponsesStreamEvent::FunctionCallArgumentsDelta {
            item_id: "item_fn".into(),
            output_index: 0,
            delta: "{\"city\":\"Bos".into(),
            sequence_number: None,
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
    assert!(mapped.len() >= 3);

    let complete_tool_use = mapped.iter().find(|e| {
        matches!(
            e,
            LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
                is_input_complete: true,
                ..
            })
        )
    });
    assert!(
        complete_tool_use.is_some(),
        "should have a complete tool use event"
    );

    let tool_uses: Vec<_> = mapped
        .iter()
        .filter(|e| matches!(e, LanguageModelCompletionEvent::ToolUse(_)))
        .collect();
    assert!(
        tool_uses.len() >= 2,
        "should have at least one partial and one complete event"
    );
    assert!(matches!(
        tool_uses.last().unwrap(),
        LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
            is_input_complete: true,
            ..
        })
    ));
}
