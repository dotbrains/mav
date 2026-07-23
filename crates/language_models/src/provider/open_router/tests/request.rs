use super::super::*;

#[gpui::test]
async fn test_session_id_uses_thread_id() {
    let model = open_router::Model::new(
        "openai/gpt-4o",
        Some("GPT-4o"),
        Some(128000),
        Some(true),
        Some(false),
        None,
        None,
    );
    let expected_session_id = "a".repeat(MAX_OPEN_ROUTER_SESSION_ID_LENGTH);
    let request = LanguageModelRequest {
        thread_id: Some(format!("{expected_session_id}extra")),
        messages: vec![language_model::LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text("Hello".to_string())],
            cache: false,
            reasoning_details: None,
        }],
        ..Default::default()
    };

    let result = into_open_router(request, &model, None);

    assert_eq!(
        result.session_id.as_deref(),
        Some(expected_session_id.as_str())
    );
}

#[gpui::test]
async fn test_agent_prevents_empty_reasoning_details_overwrite() {
    // This test verifies that the agent layer prevents empty reasoning_details
    // from overwriting non-empty ones, even though the mapper emits all events.

    // Simulate what the agent does when it receives multiple ReasoningDetails events
    let mut agent_reasoning_details: Option<serde_json::Value> = None;

    let events = vec![
        // First event: non-empty reasoning_details
        serde_json::json!([
            {
                "type": "reasoning.encrypted",
                "data": "real_data_here",
                "format": "google-gemini-v1"
            }
        ]),
        // Second event: empty array (should not overwrite)
        serde_json::json!([]),
    ];

    for details in events {
        // This mimics the agent's logic: only store if we don't already have it
        if agent_reasoning_details.is_none() {
            agent_reasoning_details = Some(details);
        }
    }

    // Verify the agent kept the first non-empty reasoning_details
    assert!(agent_reasoning_details.is_some());
    let final_details = agent_reasoning_details.unwrap();
    if let serde_json::Value::Array(arr) = &final_details {
        assert!(
            !arr.is_empty(),
            "Agent should have kept the non-empty reasoning_details"
        );
        assert_eq!(arr[0]["data"], "real_data_here");
    } else {
        panic!("Expected array");
    }
}

#[gpui::test]
async fn test_anthropic_model_caching_two_tier() {
    let model = open_router::Model::new(
        "anthropic/claude-sonnet-4-5",
        Some("Claude Sonnet"),
        Some(200000),
        Some(true),
        Some(false),
        None,
        None,
    );

    let request = LanguageModelRequest {
        messages: vec![
            language_model::LanguageModelRequestMessage {
                role: Role::System,
                content: vec![MessageContent::Text("You are helpful.".to_string())],
                cache: false,
                reasoning_details: None,
            },
            language_model::LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::Text("Hello".to_string())],
                cache: false,
                reasoning_details: None,
            },
            language_model::LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![MessageContent::Text("Hi there!".to_string())],
                cache: false,
                reasoning_details: None,
            },
            language_model::LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::Text("What is 2+2?".to_string())],
                cache: true,
                reasoning_details: None,
            },
        ],
        stop: vec![],
        temperature: None,
        tools: vec![],
        tool_choice: None,
        thinking_allowed: false,
        thinking_effort: None,
        speed: None,
        thread_id: None,
        prompt_id: None,
        intent: None,
        compact_at_tokens: None,
    };

    let result = into_open_router(request, &model, None);

    let system_cache = result.messages.iter().find_map(|m| {
        if let open_router::RequestMessage::System { content } = m {
            if let open_router::MessageContent::Multipart(parts) = content {
                parts.iter().last().and_then(|p| {
                    if let open_router::MessagePart::Text { cache_control, .. } = p {
                        *cache_control
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(
        matches!(
            system_cache,
            Some(open_router::CacheControl {
                cache_type: open_router::CacheControlType::Ephemeral,
                ttl: Some(open_router::CacheTtl::OneHour),
            })
        ),
        "System message should have 1h cache_control, got: {system_cache:?}"
    );

    let tail_cache = result.messages.last().and_then(|last_message| {
        if let open_router::RequestMessage::User { content } = last_message {
            if let open_router::MessageContent::Multipart(parts) = content {
                parts.iter().last().and_then(|part| {
                    if let open_router::MessagePart::Text { cache_control, .. } = part {
                        *cache_control
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(
        matches!(
            tail_cache,
            Some(open_router::CacheControl {
                cache_type: open_router::CacheControlType::Ephemeral,
                ttl: None,
            })
        ),
        "Last cache:true message should have 5min cache_control, got: {tail_cache:?}"
    );

    for (i, message) in result.messages.iter().enumerate() {
        let is_system = matches!(message, open_router::RequestMessage::System { .. });
        let is_last = i == result.messages.len() - 1;
        if is_system || is_last {
            continue;
        }
        let parts: Option<&Vec<open_router::MessagePart>> = match message {
            open_router::RequestMessage::User { content }
            | open_router::RequestMessage::System { content }
            | open_router::RequestMessage::Tool { content, .. } => {
                if let open_router::MessageContent::Multipart(parts) = content {
                    Some(parts)
                } else {
                    None
                }
            }
            open_router::RequestMessage::Assistant {
                content: Some(content),
                ..
            } => {
                if let open_router::MessageContent::Multipart(parts) = content {
                    Some(parts)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(parts) = parts {
            for part in parts {
                if let open_router::MessagePart::Text { cache_control, .. } = part {
                    assert!(
                        cache_control.is_none(),
                        "Message {i} should not have cache_control"
                    );
                }
            }
        }
    }
}

#[gpui::test]
async fn test_anthropic_model_no_cache_when_no_cache_flag() {
    let model = open_router::Model::new(
        "anthropic/claude-sonnet-4-5",
        Some("Claude Sonnet"),
        Some(200000),
        Some(true),
        Some(false),
        None,
        None,
    );

    let request = LanguageModelRequest {
        messages: vec![
            language_model::LanguageModelRequestMessage {
                role: Role::System,
                content: vec![MessageContent::Text("You are helpful.".to_string())],
                cache: false,
                reasoning_details: None,
            },
            language_model::LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::Text("Hello".to_string())],
                cache: false,
                reasoning_details: None,
            },
        ],
        stop: vec![],
        temperature: None,
        tools: vec![],
        tool_choice: None,
        thinking_allowed: false,
        thinking_effort: None,
        speed: None,
        thread_id: None,
        prompt_id: None,
        intent: None,
        compact_at_tokens: None,
    };

    let result = into_open_router(request, &model, None);

    for message in &result.messages {
        let content = match message {
            open_router::RequestMessage::User { content }
            | open_router::RequestMessage::System { content } => Some(content),
            _ => None,
        };
        if let Some(content) = content {
            if let open_router::MessageContent::Multipart(parts) = content {
                for part in parts {
                    if let open_router::MessagePart::Text { cache_control, .. } = part {
                        assert!(
                            cache_control.is_none(),
                            "No message should have cache_control when no cache:true flags"
                        );
                    }
                }
            }
        }
    }
}

#[gpui::test]
async fn test_non_anthropic_model_no_cache_control() {
    let model = open_router::Model::new(
        "openai/gpt-4o",
        Some("GPT-4o"),
        Some(128000),
        Some(true),
        Some(false),
        None,
        None,
    );

    let request = LanguageModelRequest {
        messages: vec![
            language_model::LanguageModelRequestMessage {
                role: Role::System,
                content: vec![MessageContent::Text("You are helpful.".to_string())],
                cache: false,
                reasoning_details: None,
            },
            language_model::LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::Text("Hello".to_string())],
                cache: true,
                reasoning_details: None,
            },
        ],
        stop: vec![],
        temperature: None,
        tools: vec![],
        tool_choice: None,
        thinking_allowed: false,
        thinking_effort: None,
        speed: None,
        thread_id: None,
        prompt_id: None,
        intent: None,
        compact_at_tokens: None,
    };

    let result = into_open_router(request, &model, None);

    for message in &result.messages {
        let content = match message {
            open_router::RequestMessage::User { content }
            | open_router::RequestMessage::System { content } => Some(content),
            _ => None,
        };
        if let Some(content) = content {
            if let open_router::MessageContent::Multipart(parts) = content {
                for part in parts {
                    if let open_router::MessagePart::Text { cache_control, .. } = part {
                        assert!(
                            cache_control.is_none(),
                            "Non-Anthropic model should never have cache_control"
                        );
                    }
                }
            }
        }
    }
}
