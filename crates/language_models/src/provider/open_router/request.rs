use super::*;

pub(super) fn into_open_router(
    request: LanguageModelRequest,
    model: &Model,
    max_output_tokens: Option<u64>,
) -> open_router::Request {
    // Anthropic models via OpenRouter don't accept reasoning_details being echoed back
    // in requests - it's an output-only field for them. However, Gemini models require
    // the thought signatures to be echoed back for proper reasoning chain continuity.
    // Note: OpenRouter's model API provides an `architecture.tokenizer` field (e.g. "Claude",
    // "Gemini") which could replace this ID prefix check, but since this is the only place
    // we need this distinction, we're just using this less invasive check instead.
    // If we ever have a more formal distionction between the models in the future,
    // we should revise this to use that instead.
    let is_anthropic_model = model.id().starts_with("anthropic/");
    let session_id = open_router_session_id(request.thread_id);

    let mut messages = Vec::new();
    let mut any_message_wants_cache = false;
    let mut last_cache_message_index: Option<usize> = None;

    for message in request.messages {
        let mut message_added_content = false;
        let reasoning_details_for_message = if is_anthropic_model {
            None
        } else {
            message.reasoning_details.clone()
        };

        let message_wants_cache = message.cache;
        if message_wants_cache {
            any_message_wants_cache = true;
        }

        for content in message.content {
            match content {
                MessageContent::Text(text) => {
                    add_message_content_part(
                        open_router::MessagePart::Text {
                            text,
                            cache_control: None,
                        },
                        message.role,
                        &mut messages,
                        reasoning_details_for_message.clone(),
                    );
                    message_added_content = true;
                }
                MessageContent::Thinking { .. } => {}
                MessageContent::RedactedThinking(_) => {}
                MessageContent::Compaction(_) => {}
                MessageContent::Image(image) => {
                    add_message_content_part(
                        open_router::MessagePart::Image {
                            image_url: image.to_base64_url(),
                        },
                        message.role,
                        &mut messages,
                        reasoning_details_for_message.clone(),
                    );
                    message_added_content = true;
                }
                MessageContent::ToolUse(tool_use) => {
                    let tool_call = open_router::ToolCall {
                        id: tool_use.id.to_string(),
                        content: open_router::ToolCallContent::Function {
                            function: open_router::FunctionContent {
                                name: tool_use.name.to_string(),
                                arguments: serde_json::to_string(&tool_use.input)
                                    .unwrap_or_default(),
                                thought_signature: tool_use.thought_signature.clone(),
                            },
                        },
                    };

                    if let Some(open_router::RequestMessage::Assistant { tool_calls, .. }) =
                        messages.last_mut()
                    {
                        tool_calls.push(tool_call);
                    } else {
                        messages.push(open_router::RequestMessage::Assistant {
                            content: None,
                            tool_calls: vec![tool_call],
                            reasoning_details: reasoning_details_for_message.clone(),
                        });
                    }
                    message_added_content = true;
                }
                MessageContent::ToolResult(tool_result) => {
                    let content: Vec<open_router::MessagePart> = tool_result
                        .content
                        .iter()
                        .map(|part| match part {
                            LanguageModelToolResultContent::Text(text) => {
                                open_router::MessagePart::Text {
                                    text: text.to_string(),
                                    cache_control: None,
                                }
                            }
                            LanguageModelToolResultContent::Image(image) => {
                                open_router::MessagePart::Image {
                                    image_url: image.to_base64_url(),
                                }
                            }
                        })
                        .collect();

                    messages.push(open_router::RequestMessage::Tool {
                        content: content.into(),
                        tool_call_id: tool_result.tool_use_id.to_string(),
                    });
                    message_added_content = true;
                }
            }
        }

        if message_wants_cache && message_added_content {
            last_cache_message_index = messages.len().checked_sub(1);
        }
    }

    if is_anthropic_model && any_message_wants_cache {
        // OpenRouter's top-level automatic cache_control restricts routing to
        // Anthropic direct; explicit block breakpoints also work on Bedrock and Vertex.
        if let Some(content) = last_cache_message_index
            .and_then(|index| messages.get_mut(index))
            .and_then(request_message_content_mut)
        {
            set_last_text_cache_control(content, cache_control(None));
        }

        if let Some(content) = messages.iter_mut().find_map(|message| match message {
            open_router::RequestMessage::System { content } => Some(content),
            _ => None,
        }) {
            set_last_text_cache_control(
                content,
                cache_control(Some(open_router::CacheTtl::OneHour)),
            );
        }
    }

    open_router::Request {
        model: model.id().into(),
        messages,
        stream: true,
        session_id,
        stop: request.stop,
        temperature: request.temperature.unwrap_or(0.4),
        max_tokens: max_output_tokens,
        parallel_tool_calls: if model.supports_parallel_tool_calls() && !request.tools.is_empty() {
            Some(false)
        } else {
            None
        },
        usage: open_router::RequestUsage { include: true },
        reasoning: if request.thinking_allowed
            && let OpenRouterModelMode::Thinking { budget_tokens } = model.mode
        {
            Some(open_router::Reasoning {
                effort: None,
                max_tokens: budget_tokens,
                exclude: Some(false),
                enabled: Some(true),
            })
        } else {
            None
        },
        tools: request
            .tools
            .into_iter()
            .map(|tool| open_router::ToolDefinition::Function {
                function: open_router::FunctionDefinition {
                    name: tool.name,
                    description: Some(tool.description),
                    parameters: Some(tool.input_schema),
                },
            })
            .collect(),
        tool_choice: request.tool_choice.map(|choice| match choice {
            LanguageModelToolChoice::Auto => open_router::ToolChoice::Auto,
            LanguageModelToolChoice::Any => open_router::ToolChoice::Required,
            LanguageModelToolChoice::None => open_router::ToolChoice::None,
        }),
        provider: model.provider.clone(),
    }
}

fn open_router_session_id(thread_id: Option<String>) -> Option<String> {
    thread_id.map(|thread_id| {
        thread_id
            .chars()
            .take(MAX_OPEN_ROUTER_SESSION_ID_LENGTH)
            .collect()
    })
}

fn cache_control(ttl: Option<open_router::CacheTtl>) -> open_router::CacheControl {
    open_router::CacheControl {
        cache_type: open_router::CacheControlType::Ephemeral,
        ttl,
    }
}

fn request_message_content_mut(
    message: &mut open_router::RequestMessage,
) -> Option<&mut open_router::MessageContent> {
    match message {
        open_router::RequestMessage::User { content }
        | open_router::RequestMessage::System { content }
        | open_router::RequestMessage::Tool { content, .. } => Some(content),
        open_router::RequestMessage::Assistant {
            content: Some(content),
            ..
        } => Some(content),
        open_router::RequestMessage::Assistant { content: None, .. } => None,
    }
}

fn set_last_text_cache_control(
    content: &mut open_router::MessageContent,
    cache_control: open_router::CacheControl,
) {
    match content {
        open_router::MessageContent::Plain(text) => {
            let text = std::mem::take(text);
            *content =
                open_router::MessageContent::Multipart(vec![open_router::MessagePart::Text {
                    text,
                    cache_control: Some(cache_control),
                }]);
        }
        open_router::MessageContent::Multipart(parts) => {
            for part in parts.iter_mut().rev() {
                if let open_router::MessagePart::Text {
                    cache_control: target,
                    ..
                } = part
                {
                    *target = Some(cache_control);
                    break;
                }
            }
        }
    }
}

fn add_message_content_part(
    new_part: open_router::MessagePart,
    role: Role,
    messages: &mut Vec<open_router::RequestMessage>,
    reasoning_details: Option<Arc<serde_json::Value>>,
) {
    match (role, messages.last_mut()) {
        (Role::User, Some(open_router::RequestMessage::User { content }))
        | (Role::System, Some(open_router::RequestMessage::System { content })) => {
            content.push_part(new_part);
        }
        (
            Role::Assistant,
            Some(open_router::RequestMessage::Assistant {
                content: Some(content),
                ..
            }),
        ) => {
            content.push_part(new_part);
        }
        _ => {
            messages.push(match role {
                Role::User => open_router::RequestMessage::User {
                    content: open_router::MessageContent::from(vec![new_part]),
                },
                Role::Assistant => open_router::RequestMessage::Assistant {
                    content: Some(open_router::MessageContent::from(vec![new_part])),
                    tool_calls: Vec::new(),
                    reasoning_details,
                },
                Role::System => open_router::RequestMessage::System {
                    content: open_router::MessageContent::from(vec![new_part]),
                },
            });
        }
    }
}
