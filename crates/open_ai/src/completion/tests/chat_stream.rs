use super::*;

#[test]
fn into_open_ai_interleaved_reasoning() {
    let tool_use_id = LanguageModelToolUseId::from("call-1");
    let tool_input = json!({"query": "foo"});
    let tool_arguments = serde_json::to_string(&tool_input).unwrap();
    let tool_use = LanguageModelToolUse {
        id: tool_use_id.clone(),
        name: Arc::from("search"),
        raw_input: tool_arguments.clone(),
        input: tool_input,
        is_input_complete: true,
        thought_signature: None,
    };
    let tool_result = LanguageModelToolResult {
        tool_use_id: tool_use_id,
        tool_name: Arc::from("search"),
        is_error: false,
        content: vec![LanguageModelToolResultContent::Text(Arc::from("result"))],
        output: None,
    };
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::Text("search for something".into())],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![
                    MessageContent::Thinking {
                        text: "I should search".into(),
                        signature: None,
                    },
                    MessageContent::Text("Searching now.".into()),
                    MessageContent::ToolUse(tool_use),
                ],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![MessageContent::ToolResult(tool_result)],
                cache: false,
                reasoning_details: None,
            },
        ],
        tools: vec![],
        tool_choice: None,
        stop: vec![],
        temperature: None,
        thinking_allowed: true,
        thinking_effort: None,
        speed: None,
        compact_at_tokens: None,
    };

    let result = into_open_ai(
        request.clone(),
        "model",
        false,
        false,
        None,
        ChatCompletionMaxTokensParameter::MaxCompletionTokens,
        None,
        true,
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap()["messages"],
        json!([
            {"role": "user", "content": "search for something"},
            {
                "role": "assistant",
                "content": "Searching now.",
                "tool_calls": [{"id": "call-1", "type": "function", "function": {"name": "search", "arguments": tool_arguments}}],
                "reasoning_content": "I should search"
            },
            {"role": "tool", "content": "result", "tool_call_id": "call-1"}
        ])
    );

    let result = into_open_ai(
        request,
        "model",
        false,
        false,
        None,
        ChatCompletionMaxTokensParameter::MaxCompletionTokens,
        None,
        false,
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap()["messages"],
        json!([
            {"role": "user", "content": "search for something"},
            {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "I should search"},
                    {"type": "text", "text": "Searching now."}
                ],
                "tool_calls": [{"id": "call-1", "type": "function", "function": {"name": "search", "arguments": tool_arguments}}]
            },
            {"role": "tool", "content": "result", "tool_call_id": "call-1"}
        ])
    );
}

#[test]
fn stream_maps_reasoning() {
    let events = map_completion_events(vec![ResponseStreamEvent {
        choices: vec![ChoiceDelta {
            index: 0,
            delta: Some(ResponseMessageDelta {
                role: None,
                content: None,
                reasoning: Some("thinking".into()),
                tool_calls: None,
                reasoning_content: None,
            }),
            finish_reason: None,
        }],
        usage: None,
    }]);

    assert_eq!(
        events,
        vec![LanguageModelCompletionEvent::Thinking {
            text: "thinking".into(),
            signature: None,
        }]
    );
}

#[test]
fn stream_maps_preserves_tool_id_and_name_across_empty_deltas() {
    // DashScope sends id="" and name="" in subsequent tool_calls delta
    // chunks after the first chunk. OpenAiEventMapper must not overwrite
    // the accumulated id and name with these empty strings.

    let events = vec![
        // First chunk: id and name are present
        ResponseStreamEvent {
            choices: vec![ChoiceDelta {
                index: 0,
                delta: Some(ResponseMessageDelta {
                    role: None,
                    content: None,
                    reasoning: None,
                    tool_calls: Some(vec![ToolCallChunk {
                        index: 0,
                        id: Some("call_dashscope_test".into()),
                        function: Some(FunctionChunk {
                            name: Some("list_directory".into()),
                            arguments: Some("".into()),
                        }),
                    }]),
                    reasoning_content: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        },
        // Subsequent chunks: DashScope sends id="" and name=""
        ResponseStreamEvent {
            choices: vec![ChoiceDelta {
                index: 0,
                delta: Some(ResponseMessageDelta {
                    role: None,
                    content: None,
                    reasoning: None,
                    tool_calls: Some(vec![ToolCallChunk {
                        index: 0,
                        id: Some("".into()),
                        function: Some(FunctionChunk {
                            name: Some("".into()),
                            arguments: Some("{\"path\": \"".into()),
                        }),
                    }]),
                    reasoning_content: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        },
        ResponseStreamEvent {
            choices: vec![ChoiceDelta {
                index: 0,
                delta: Some(ResponseMessageDelta {
                    role: None,
                    content: None,
                    reasoning: None,
                    tool_calls: Some(vec![ToolCallChunk {
                        index: 0,
                        id: Some("".into()),
                        function: Some(FunctionChunk {
                            name: Some("".into()),
                            arguments: Some("blog-scraper\"}".into()),
                        }),
                    }]),
                    reasoning_content: None,
                }),
                finish_reason: None,
            }],
            usage: None,
        },
        // Final chunk: finish_reason = "tool_calls"
        ResponseStreamEvent {
            choices: vec![ChoiceDelta {
                index: 0,
                delta: None,
                finish_reason: Some("tool_calls".into()),
            }],
            usage: None,
        },
    ];

    let mapped = map_completion_events(events);

    // Events emitted:
    //   1. Partial ToolUse from chunk 1 (fix_json("") → "{}", parseable)
    //   2. Partial ToolUse from chunk 3 (arguments fully assembled)
    //   3. Complete ToolUse from finish_reason="tool_calls" drain
    //   4. Stop(ToolUse)
    assert_eq!(mapped.len(), 4);

    // Verify the complete ToolUse event (from finish_reason drain)
    // has the correct id, name, and accumulated arguments.
    let complete_tool_use = mapped.iter().find_map(|event| {
        if let LanguageModelCompletionEvent::ToolUse(tool_use) = event {
            if tool_use.is_input_complete {
                return Some(tool_use);
            }
        }
        None
    });
    assert!(
        complete_tool_use.is_some(),
        "expected a completed ToolUse event"
    );
    let tool_use = complete_tool_use.unwrap();
    assert_eq!(
        tool_use.id.to_string(),
        "call_dashscope_test",
        "id must survive empty-string overwrites"
    );
    assert_eq!(
        tool_use.name.as_ref(),
        "list_directory",
        "name must survive empty-string overwrites"
    );
    assert_eq!(
        tool_use.raw_input, "{\"path\": \"blog-scraper\"}",
        "arguments should accumulate across chunks"
    );

    // Verify the Stop event
    assert!(mapped.iter().any(|event| {
        matches!(
            event,
            LanguageModelCompletionEvent::Stop(StopReason::ToolUse)
        )
    }));
}
