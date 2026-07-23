use super::*;

pub fn into_mistral(
    request: LanguageModelRequest,
    model: mistral::Model,
    max_output_tokens: Option<u64>,
) -> (mistral::Request, Option<String>) {
    let stream = true;

    let mut messages = Vec::new();
    for message in &request.messages {
        match message.role {
            Role::User => {
                let mut message_content = mistral::MessageContent::empty();
                for content in &message.content {
                    match content {
                        MessageContent::Text(text) => {
                            message_content
                                .push_part(mistral::MessagePart::Text { text: text.clone() });
                        }
                        MessageContent::Image(image_content) => {
                            if model.supports_images() {
                                message_content.push_part(mistral::MessagePart::ImageUrl {
                                    image_url: image_content.to_base64_url(),
                                });
                            }
                        }
                        MessageContent::Thinking { text, .. } => {
                            if model.supports_thinking() {
                                message_content.push_part(mistral::MessagePart::Thinking {
                                    thinking: vec![mistral::ThinkingPart::Text {
                                        text: text.clone(),
                                    }],
                                });
                            }
                        }
                        MessageContent::RedactedThinking(_) => {}
                        MessageContent::Compaction(_) => {}
                        MessageContent::ToolUse(_) => {
                            // Tool use is not supported in User messages for Mistral
                        }
                        MessageContent::ToolResult(tool_result) => {
                            let mut text_parts: Vec<String> = Vec::new();
                            for part in &tool_result.content {
                                match part {
                                    LanguageModelToolResultContent::Text(text) => {
                                        text_parts.push(text.to_string());
                                    }
                                    LanguageModelToolResultContent::Image(_) => {
                                        text_parts.push("[Tool responded with an image, but Mav doesn't support these in Mistral models yet]".to_string());
                                    }
                                }
                            }
                            messages.push(mistral::RequestMessage::Tool {
                                content: text_parts.join("\n"),
                                tool_call_id: tool_result.tool_use_id.to_string(),
                            });
                        }
                    }
                }
                if !matches!(message_content, mistral::MessageContent::Plain { ref content } if content.is_empty())
                {
                    messages.push(mistral::RequestMessage::User {
                        content: message_content,
                    });
                }
            }
            Role::Assistant => {
                for content in &message.content {
                    match content {
                        MessageContent::Text(text) if text.is_empty() => {
                            // Mistral API returns a 400 if there's neither content nor tool_calls
                        }
                        MessageContent::Text(text) => {
                            messages.push(mistral::RequestMessage::Assistant {
                                content: Some(mistral::MessageContent::Plain {
                                    content: text.clone(),
                                }),
                                tool_calls: Vec::new(),
                            });
                        }
                        MessageContent::Thinking { text, .. } => {
                            if model.supports_thinking() {
                                messages.push(mistral::RequestMessage::Assistant {
                                    content: Some(mistral::MessageContent::Multipart {
                                        content: vec![mistral::MessagePart::Thinking {
                                            thinking: vec![mistral::ThinkingPart::Text {
                                                text: text.clone(),
                                            }],
                                        }],
                                    }),
                                    tool_calls: Vec::new(),
                                });
                            }
                        }
                        MessageContent::RedactedThinking(_) => {}
                        MessageContent::Image(_) => {}
                        MessageContent::Compaction(_) => {}
                        MessageContent::ToolUse(tool_use) => {
                            let tool_call = mistral::ToolCall {
                                id: tool_use.id.to_string(),
                                content: mistral::ToolCallContent::Function {
                                    function: mistral::FunctionContent {
                                        name: tool_use.name.to_string(),
                                        arguments: serde_json::to_string(&tool_use.input)
                                            .unwrap_or_default(),
                                    },
                                },
                            };

                            if let Some(mistral::RequestMessage::Assistant { tool_calls, .. }) =
                                messages.last_mut()
                            {
                                tool_calls.push(tool_call);
                            } else {
                                messages.push(mistral::RequestMessage::Assistant {
                                    content: None,
                                    tool_calls: vec![tool_call],
                                });
                            }
                        }
                        MessageContent::ToolResult(_) => {
                            // Tool results are not supported in Assistant messages
                        }
                    }
                }
            }
            Role::System => {
                for content in &message.content {
                    match content {
                        MessageContent::Text(text) => {
                            messages.push(mistral::RequestMessage::System {
                                content: mistral::MessageContent::Plain {
                                    content: text.clone(),
                                },
                            });
                        }
                        MessageContent::Thinking { text, .. } => {
                            if model.supports_thinking() {
                                messages.push(mistral::RequestMessage::System {
                                    content: mistral::MessageContent::Multipart {
                                        content: vec![mistral::MessagePart::Thinking {
                                            thinking: vec![mistral::ThinkingPart::Text {
                                                text: text.clone(),
                                            }],
                                        }],
                                    },
                                });
                            }
                        }
                        MessageContent::RedactedThinking(_) => {}
                        MessageContent::Compaction(_) => {}
                        MessageContent::Image(_)
                        | MessageContent::ToolUse(_)
                        | MessageContent::ToolResult(_) => {
                            // Images and tools are not supported in System messages
                        }
                    }
                }
            }
        }
    }

    (
        mistral::Request {
            model: model.id().to_string(),
            messages,
            stream,
            stream_options: if stream {
                Some(mistral::StreamOptions {
                    stream_tool_calls: Some(true),
                })
            } else {
                None
            },
            max_tokens: max_output_tokens,
            temperature: request.temperature,
            response_format: None,
            tool_choice: match request.tool_choice {
                Some(LanguageModelToolChoice::Auto) if !request.tools.is_empty() => {
                    Some(mistral::ToolChoice::Auto)
                }
                Some(LanguageModelToolChoice::Any) if !request.tools.is_empty() => {
                    Some(mistral::ToolChoice::Any)
                }
                Some(LanguageModelToolChoice::None) => Some(mistral::ToolChoice::None),
                _ if !request.tools.is_empty() => Some(mistral::ToolChoice::Auto),
                _ => None,
            },
            parallel_tool_calls: if !request.tools.is_empty() {
                Some(false)
            } else {
                None
            },
            tools: request
                .tools
                .into_iter()
                .map(|tool| mistral::ToolDefinition::Function {
                    function: mistral::FunctionDefinition {
                        name: tool.name,
                        description: Some(tool.description),
                        parameters: Some(tool.input_schema),
                    },
                })
                .collect(),
        },
        request.thread_id,
    )
}
