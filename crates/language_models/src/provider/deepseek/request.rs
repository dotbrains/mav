use super::*;

pub fn into_deepseek(
    request: LanguageModelRequest,
    model: &deepseek::Model,
    max_output_tokens: Option<u64>,
) -> deepseek::Request {
    let thinking = deepseek_thinking(model, request.thinking_allowed);
    let thinking_enabled = thinking
        .as_ref()
        .is_some_and(|thinking| thinking.kind == deepseek::ThinkingType::Enabled);

    let mut messages = Vec::new();
    let mut current_reasoning: Option<String> = None;

    for message in request.messages {
        for content in message.content {
            match content {
                MessageContent::Text(text) => {
                    let should_add = if message.role == Role::User {
                        !text.trim().is_empty()
                    } else {
                        !text.is_empty()
                    };

                    if should_add {
                        messages.push(match message.role {
                            Role::User => deepseek::RequestMessage::User { content: text },
                            Role::Assistant => deepseek::RequestMessage::Assistant {
                                content: Some(text),
                                tool_calls: Vec::new(),
                                reasoning_content: current_reasoning.take(),
                            },
                            Role::System => deepseek::RequestMessage::System { content: text },
                        });
                    }
                }
                MessageContent::Thinking { text, .. } => {
                    // Accumulate reasoning content for next assistant message
                    current_reasoning.get_or_insert_default().push_str(&text);
                }
                MessageContent::RedactedThinking(_) => {}
                MessageContent::Image(_) => {}
                MessageContent::Compaction(_) => {}
                MessageContent::ToolUse(tool_use) => {
                    let tool_call = deepseek::ToolCall {
                        id: tool_use.id.to_string(),
                        content: deepseek::ToolCallContent::Function {
                            function: deepseek::FunctionContent {
                                name: tool_use.name.to_string(),
                                arguments: serde_json::to_string(&tool_use.input)
                                    .unwrap_or_default(),
                            },
                        },
                    };

                    if let Some(deepseek::RequestMessage::Assistant { tool_calls, .. }) =
                        messages.last_mut()
                    {
                        tool_calls.push(tool_call);
                    } else {
                        messages.push(deepseek::RequestMessage::Assistant {
                            content: None,
                            tool_calls: vec![tool_call],
                            reasoning_content: current_reasoning.take(),
                        });
                    }
                }
                MessageContent::ToolResult(tool_result) => {
                    let mut text_parts: Vec<String> = Vec::new();
                    for part in &tool_result.content {
                        match part {
                            LanguageModelToolResultContent::Text(text) => {
                                text_parts.push(text.to_string());
                            }
                            LanguageModelToolResultContent::Image(_) => {
                                text_parts.push("[Tool responded with an image]".to_string());
                            }
                        }
                    }
                    let content = if text_parts.is_empty() {
                        "<Tool returned an empty string>".to_string()
                    } else {
                        text_parts.join("\n")
                    };
                    messages.push(deepseek::RequestMessage::Tool {
                        content,
                        tool_call_id: tool_result.tool_use_id.to_string(),
                    });
                }
            }
        }
    }

    deepseek::Request {
        model: model.id().to_string(),
        messages,
        stream: true,
        max_tokens: max_output_tokens,
        temperature: if thinking_enabled {
            None
        } else {
            request.temperature
        },
        thinking,
        reasoning_effort: if thinking_enabled {
            into_deepseek_reasoning_effort(request.thinking_effort.as_deref())
        } else {
            None
        },
        response_format: None,
        tool_choice: request.tool_choice.map(|choice| match choice {
            LanguageModelToolChoice::Auto => deepseek::ToolChoice::Auto,
            LanguageModelToolChoice::Any => deepseek::ToolChoice::Required,
            LanguageModelToolChoice::None => deepseek::ToolChoice::None,
        }),
        tools: request
            .tools
            .into_iter()
            .map(|tool| deepseek::ToolDefinition::Function {
                function: deepseek::FunctionDefinition {
                    name: tool.name,
                    description: Some(tool.description),
                    parameters: Some(tool.input_schema),
                },
            })
            .collect(),
    }
}

fn deepseek_thinking(
    model: &deepseek::Model,
    thinking_allowed: bool,
) -> Option<deepseek::Thinking> {
    let kind = match model {
        deepseek::Model::V4Flash | deepseek::Model::V4Pro => {
            if thinking_allowed {
                deepseek::ThinkingType::Enabled
            } else {
                deepseek::ThinkingType::Disabled
            }
        }
        deepseek::Model::Custom { .. } => return None,
    };

    Some(deepseek::Thinking { kind })
}

fn into_deepseek_reasoning_effort(effort: Option<&str>) -> Option<deepseek::ReasoningEffort> {
    match effort {
        Some("high") => Some(deepseek::ReasoningEffort::High),
        Some("max") => Some(deepseek::ReasoningEffort::Max),
        _ => None,
    }
}
