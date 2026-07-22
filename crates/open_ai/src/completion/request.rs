use super::*;

/// Translates the request's `Speed` into the corresponding OpenAI service tier.
/// Only `Fast` produces a value; `Standard` leaves the field unset so that the
/// project's default tier applies.
pub(super) fn service_tier_for(speed: Option<language_model_core::Speed>) -> Option<ServiceTier> {
    match speed? {
        language_model_core::Speed::Fast => Some(ServiceTier::Priority),
        language_model_core::Speed::Standard => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatCompletionMaxTokensParameter {
    MaxCompletionTokens,
    MaxTokens,
}

pub fn into_open_ai(
    request: LanguageModelRequest,
    model_id: &str,
    supports_parallel_tool_calls: bool,
    supports_prompt_cache_key: bool,
    max_output_tokens: Option<u64>,
    max_tokens_parameter: ChatCompletionMaxTokensParameter,
    reasoning_effort: Option<ReasoningEffort>,
    interleaved_reasoning: bool,
) -> crate::Request {
    let stream = !model_id.starts_with("o1-");
    let service_tier = service_tier_for(request.speed);

    let mut messages = Vec::new();
    let mut current_reasoning: Option<String> = None;
    for message in request.messages {
        for content in message.content {
            match content {
                MessageContent::Thinking { text, .. } if interleaved_reasoning => {
                    current_reasoning.get_or_insert_default().push_str(&text);
                }
                MessageContent::Text(text) | MessageContent::Thinking { text, .. } => {
                    let should_add = if message.role == Role::User {
                        // Including whitespace-only user messages can cause error with OpenAI compatible APIs
                        // See https://github.com/mav-industries/mav/issues/40097
                        !text.trim().is_empty()
                    } else {
                        !text.is_empty()
                    };
                    if should_add {
                        add_message_content_part(
                            MessagePart::Text { text },
                            message.role,
                            &mut messages,
                        );
                        if let Some(reasoning) = current_reasoning.take() {
                            if let Some(crate::RequestMessage::Assistant {
                                reasoning_content,
                                ..
                            }) = messages.last_mut()
                            {
                                *reasoning_content = Some(reasoning);
                            }
                        }
                    }
                }
                MessageContent::RedactedThinking(_) | MessageContent::Compaction(_) => {}
                MessageContent::Image(image) => {
                    add_message_content_part(
                        MessagePart::Image {
                            image_url: ImageUrl {
                                url: image.to_base64_url(),
                                detail: None,
                            },
                        },
                        message.role,
                        &mut messages,
                    );
                }
                MessageContent::ToolUse(tool_use) => {
                    let tool_call = ToolCall {
                        id: tool_use.id.to_string(),
                        content: ToolCallContent::Function {
                            function: FunctionContent {
                                name: tool_use.name.to_string(),
                                arguments: serde_json::to_string(&tool_use.input)
                                    .unwrap_or_default(),
                            },
                        },
                    };

                    if let Some(crate::RequestMessage::Assistant { tool_calls, .. }) =
                        messages.last_mut()
                    {
                        tool_calls.push(tool_call);
                    } else {
                        messages.push(crate::RequestMessage::Assistant {
                            content: None,
                            tool_calls: vec![tool_call],
                            reasoning_content: current_reasoning.take(),
                        });
                    }
                }
                MessageContent::ToolResult(tool_result) => {
                    let content: Vec<MessagePart> = tool_result
                        .content
                        .iter()
                        .map(|part| match part {
                            LanguageModelToolResultContent::Text(text) => MessagePart::Text {
                                text: text.to_string(),
                            },
                            LanguageModelToolResultContent::Image(image) => MessagePart::Image {
                                image_url: ImageUrl {
                                    url: image.to_base64_url(),
                                    detail: None,
                                },
                            },
                        })
                        .collect();

                    messages.push(crate::RequestMessage::Tool {
                        content: content.into(),
                        tool_call_id: tool_result.tool_use_id.to_string(),
                    });
                }
            }
        }
    }

    crate::Request {
        model: model_id.into(),
        messages,
        stream,
        stream_options: if stream {
            Some(crate::StreamOptions::default())
        } else {
            None
        },
        stop: request.stop,
        temperature: request.temperature.or(Some(1.0)),
        max_completion_tokens: match max_tokens_parameter {
            ChatCompletionMaxTokensParameter::MaxCompletionTokens => max_output_tokens,
            ChatCompletionMaxTokensParameter::MaxTokens => None,
        },
        max_tokens: match max_tokens_parameter {
            ChatCompletionMaxTokensParameter::MaxCompletionTokens => None,
            ChatCompletionMaxTokensParameter::MaxTokens => max_output_tokens,
        },
        parallel_tool_calls: if supports_parallel_tool_calls && !request.tools.is_empty() {
            Some(supports_parallel_tool_calls)
        } else {
            None
        },
        prompt_cache_key: if supports_prompt_cache_key {
            request.thread_id
        } else {
            None
        },
        tools: request
            .tools
            .into_iter()
            .map(|tool| crate::ToolDefinition::Function {
                function: FunctionDefinition {
                    name: tool.name,
                    description: Some(tool.description),
                    parameters: Some(tool.input_schema),
                },
            })
            .collect(),
        tool_choice: request.tool_choice.map(|choice| match choice {
            LanguageModelToolChoice::Auto => crate::ToolChoice::Auto,
            LanguageModelToolChoice::Any => crate::ToolChoice::Required,
            LanguageModelToolChoice::None => crate::ToolChoice::None,
        }),
        reasoning_effort,
        service_tier,
    }
}

fn add_message_content_part(
    new_part: MessagePart,
    role: Role,
    messages: &mut Vec<crate::RequestMessage>,
) {
    match (role, messages.last_mut()) {
        (Role::User, Some(crate::RequestMessage::User { content }))
        | (
            Role::Assistant,
            Some(crate::RequestMessage::Assistant {
                content: Some(content),
                ..
            }),
        )
        | (Role::System, Some(crate::RequestMessage::System { content, .. })) => {
            content.push_part(new_part);
        }
        _ => {
            messages.push(match role {
                Role::User => crate::RequestMessage::User {
                    content: crate::MessageContent::from(vec![new_part]),
                },
                Role::Assistant => crate::RequestMessage::Assistant {
                    content: Some(crate::MessageContent::from(vec![new_part])),
                    tool_calls: Vec::new(),
                    reasoning_content: None,
                },
                Role::System => crate::RequestMessage::System {
                    content: crate::MessageContent::from(vec![new_part]),
                },
            });
        }
    }
}
