use super::*;

#[test]
fn into_open_ai_response_builds_complete_payload() {
    let tool_call_id = LanguageModelToolUseId::from("call-42");
    let tool_input = json!({ "city": "Boston" });
    let tool_arguments = serde_json::to_string(&tool_input).unwrap();
    let tool_use = LanguageModelToolUse {
        id: tool_call_id.clone(),
        name: Arc::from("get_weather"),
        raw_input: tool_arguments.clone(),
        input: tool_input,
        is_input_complete: true,
        thought_signature: None,
    };
    let tool_result = LanguageModelToolResult {
        tool_use_id: tool_call_id,
        tool_name: Arc::from("get_weather"),
        is_error: false,
        content: vec![LanguageModelToolResultContent::Text(Arc::from("Sunny"))],
        output: Some(json!({ "forecast": "Sunny" })),
    };
    let user_image = LanguageModelImage {
        source: SharedString::from("aGVsbG8="),
    };
    let expected_image_url = user_image.to_base64_url();

    let request = LanguageModelRequest {
        thread_id: Some("thread-123".into()),
        prompt_id: None,
        intent: None,
        messages: vec![
            LanguageModelRequestMessage {
                role: Role::System,
                content: vec![MessageContent::Text("System context".into())],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![
                    MessageContent::Text("Please check the weather.".into()),
                    MessageContent::Image(user_image),
                ],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![
                    MessageContent::Text("Looking that up.".into()),
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
        tools: vec![LanguageModelRequestTool {
            name: "get_weather".into(),
            description: "Fetches the weather".into(),
            input_schema: json!({ "type": "object" }),
            use_input_streaming: false,
        }],
        tool_choice: Some(LanguageModelToolChoice::Any),
        stop: vec!["<STOP>".into()],
        temperature: None,
        thinking_allowed: true,
        thinking_effort: Some("high".into()),
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(
        request,
        "custom-model",
        true,
        true,
        Some(2048),
        Some(ReasoningEffort::Low),
        false,
    );

    let serialized = serde_json::to_value(&response).unwrap();
    let expected = json!({
        "model": "custom-model",
        "input": [
            {
                "type": "message",
                "role": "system",
                "content": [
                    { "type": "input_text", "text": "System context" }
                ]
            },
            {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "Please check the weather." },
                    { "type": "input_image", "image_url": expected_image_url }
                ]
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    { "type": "output_text", "text": "Looking that up.", "annotations": [] }
                ]
            },
            {
                "type": "function_call",
                "call_id": "call-42",
                "name": "get_weather",
                "arguments": tool_arguments
            },
            {
                "type": "function_call_output",
                "call_id": "call-42",
                "output": "Sunny"
            }
        ],
        "store": false,
        "include": ["reasoning.encrypted_content"],
        "stream": true,
        "max_output_tokens": 2048,
        "parallel_tool_calls": true,
        "tool_choice": "required",
        "tools": [
            {
                "type": "function",
                "name": "get_weather",
                "description": "Fetches the weather",
                "parameters": { "type": "object" }
            }
        ],
        "prompt_cache_key": "thread-123",
        "reasoning": { "effort": "high", "summary": "auto" }
    });

    assert_eq!(serialized, expected);
}

#[test]
fn into_open_ai_response_replays_encrypted_reasoning_details() {
    let tool_call_id = LanguageModelToolUseId::from("call-42");
    let tool_arguments = "{\"city\":\"Boston\"}".to_string();
    let tool_use = LanguageModelToolUse {
        id: tool_call_id,
        name: Arc::from("get_weather"),
        raw_input: tool_arguments.clone(),
        input: json!({ "city": "Boston" }),
        is_input_complete: true,
        thought_signature: None,
    };

    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![LanguageModelRequestMessage {
            role: Role::Assistant,
            content: vec![MessageContent::ToolUse(tool_use)],
            cache: false,
            reasoning_details: Some(Arc::new(json!({
                "reasoning_items": [
                    {
                        "id": "rs_123",
                        "summary": [
                            {
                                "type": "summary_text",
                                "text": "Checked what information is needed."
                            }
                        ],
                        "content": [
                            {
                                "type": "reasoning_text",
                                "text": "Internal reasoning text."
                            }
                        ],
                        "encrypted_content": "ENC",
                        "status": "completed",
                    }
                ]
            }))),
        }],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: false,
        thinking_effort: None,
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(
        request,
        "gpt-5",
        true,
        true,
        None,
        Some(ReasoningEffort::Low),
        false,
    );

    let serialized = serde_json::to_value(&response).unwrap();
    assert_eq!(
        serialized["input"],
        json!([
            {
                "type": "reasoning",
                "id": "rs_123",
                "summary": [
                    {
                        "type": "summary_text",
                        "text": "Checked what information is needed."
                    }
                ],
                "content": [
                    {
                        "type": "reasoning_text",
                        "text": "Internal reasoning text."
                    }
                ],
                "encrypted_content": "ENC",
                "status": "completed"
            },
            {
                "type": "function_call",
                "call_id": "call-42",
                "name": "get_weather",
                "arguments": tool_arguments
            }
        ])
    );
    assert_eq!(
        serialized["include"],
        json!(["reasoning.encrypted_content"])
    );
    assert_eq!(serialized.get("reasoning"), None);
}

#[test]
fn into_open_ai_response_replays_reasoning_without_encrypted_content() {
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![LanguageModelRequestMessage {
            role: Role::Assistant,
            content: vec![MessageContent::Text("Done.".into())],
            cache: false,
            reasoning_details: Some(Arc::new(json!({
                "reasoning_items": [
                    {
                        "id": "rs_123",
                        "summary": [],
                        "status": "completed"
                    },
                    {
                        "id": "rs_456",
                        "summary": [],
                        "encrypted_content": "",
                        "status": "completed"
                    }
                ]
            }))),
        }],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: false,
        thinking_effort: None,
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(request, "custom-model", false, false, None, None, false);
    let serialized = serde_json::to_value(&response).unwrap();

    assert_eq!(
        serialized["input"],
        json!([
            {
                "type": "reasoning",
                "id": "rs_123",
                "summary": [],
                "status": "completed"
            },
            {
                "type": "reasoning",
                "id": "rs_456",
                "summary": [],
                "encrypted_content": "",
                "status": "completed"
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": "Done.",
                        "annotations": []
                    }
                ]
            }
        ])
    );
}

#[test]
fn into_open_ai_response_omits_reasoning_when_thinking_is_disabled_and_none_is_unsupported() {
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text("Hello".into())],
            cache: false,
            reasoning_details: None,
        }],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: false,
        thinking_effort: Some("high".into()),
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(
        request,
        "gpt-5",
        true,
        true,
        None,
        Some(ReasoningEffort::Medium),
        false,
    );

    let serialized = serde_json::to_value(&response).unwrap();
    assert_eq!(serialized.get("reasoning"), None);
}

/// `Speed::Fast` should translate to `service_tier: "priority"` on the
/// outgoing Responses request, while `Standard` / `None` should leave the
/// field unset so the project's default tier wins.
#[test]
fn into_open_ai_response_sets_service_tier_for_fast_speed() -> Result<()> {
    for (speed, expected) in [
        (None, None),
        (Some(Speed::Standard), None),
        (Some(Speed::Fast), Some("priority")),
    ] {
        let request = LanguageModelRequest {
            thread_id: None,
            prompt_id: None,
            intent: None,
            messages: vec![LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::Text("Hello".into())],
                cache: false,
                reasoning_details: None,
            }],
            tools: Vec::new(),
            tool_choice: None,
            stop: Vec::new(),
            temperature: None,
            thinking_allowed: false,
            thinking_effort: None,
            speed,
            compact_at_tokens: None,
        };

        let response = into_open_ai_response(request, "gpt-5.4", true, true, None, None, true);

        let serialized = serde_json::to_value(&response)?;
        assert_eq!(
            serialized
                .get("service_tier")
                .and_then(|value| value.as_str()),
            expected,
            "speed = {speed:?} should produce service_tier = {expected:?}",
        );
    }
    Ok(())
}

/// Same as above but for the Chat Completions code path.
#[test]
fn into_open_ai_sets_service_tier_for_fast_speed() -> Result<()> {
    for (speed, expected) in [
        (None, None),
        (Some(Speed::Standard), None),
        (Some(Speed::Fast), Some("priority")),
    ] {
        let request = LanguageModelRequest {
            thread_id: None,
            prompt_id: None,
            intent: None,
            messages: vec![LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::Text("Hello".into())],
                cache: false,
                reasoning_details: None,
            }],
            tools: Vec::new(),
            tool_choice: None,
            stop: Vec::new(),
            temperature: None,
            thinking_allowed: false,
            thinking_effort: None,
            speed,
            compact_at_tokens: None,
        };

        let chat = into_open_ai(
            request,
            "gpt-5.4",
            true,
            true,
            None,
            ChatCompletionMaxTokensParameter::MaxCompletionTokens,
            None,
            false,
        );

        let serialized = serde_json::to_value(&chat)?;
        assert_eq!(
            serialized
                .get("service_tier")
                .and_then(|value| value.as_str()),
            expected,
            "speed = {speed:?} should produce service_tier = {expected:?}",
        );
    }
    Ok(())
}

#[test]
fn into_open_ai_can_send_max_tokens_parameter() -> Result<()> {
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text("Hello".into())],
            cache: false,
            reasoning_details: None,
        }],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: false,
        thinking_effort: None,
        speed: None,
        compact_at_tokens: None,
    };

    let chat = into_open_ai(
        request,
        "compatible-model",
        false,
        false,
        Some(4096),
        ChatCompletionMaxTokensParameter::MaxTokens,
        None,
        false,
    );

    let serialized = serde_json::to_value(&chat)?;
    assert_eq!(serialized.get("max_completion_tokens"), None);
    assert_eq!(serialized["max_tokens"], json!(4096));
    Ok(())
}
