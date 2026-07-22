use super::*;

pub(super) fn build_llama_cpp_request(
    model_name: &str,
    supports_images: bool,
    capabilities: LiveCapabilities,
    request: LanguageModelRequest,
) -> llama_cpp::ChatCompletionRequest {
    let supports_tools = capabilities.supports_tools;
    let supports_thinking = capabilities.supports_thinking;
    let mut messages = Vec::new();

    for message in request.messages {
        let mut reasoning_content: Option<String> = None;
        for content in message.content {
            match content {
                MessageContent::Text(text) => add_message_content_part(
                    llama_cpp::MessagePart::Text { text },
                    message.role,
                    &mut messages,
                    if supports_thinking && message.role == Role::Assistant {
                        reasoning_content.take()
                    } else {
                        None
                    },
                ),
                MessageContent::Thinking { text, .. } => {
                    if supports_thinking && message.role == Role::Assistant && !text.is_empty() {
                        reasoning_content.get_or_insert_default().push_str(&text);
                    }
                }
                MessageContent::RedactedThinking(_) => {}
                MessageContent::Compaction(_) => {}
                MessageContent::Image(image) => {
                    if supports_images {
                        add_message_content_part(
                            llama_cpp::MessagePart::Image {
                                image_url: llama_cpp::ImageUrl {
                                    url: image.to_base64_url(),
                                    detail: None,
                                },
                            },
                            message.role,
                            &mut messages,
                            if supports_thinking && message.role == Role::Assistant {
                                reasoning_content.take()
                            } else {
                                None
                            },
                        );
                    }
                }
                MessageContent::ToolUse(tool_use) => {
                    let tool_call = llama_cpp::ToolCall {
                        id: tool_use.id.to_string(),
                        content: llama_cpp::ToolCallContent::Function {
                            function: llama_cpp::FunctionContent {
                                name: tool_use.name.to_string(),
                                arguments: serde_json::to_string(&tool_use.input)
                                    .unwrap_or_default(),
                            },
                        },
                    };

                    if let Some(llama_cpp::ChatMessage::Assistant {
                        tool_calls,
                        reasoning_content: message_reasoning_content,
                        ..
                    }) = messages.last_mut()
                    {
                        append_reasoning_content(
                            message_reasoning_content,
                            reasoning_content.take(),
                        );
                        tool_calls.push(tool_call);
                    } else {
                        messages.push(llama_cpp::ChatMessage::Assistant {
                            content: None,
                            reasoning_content: reasoning_content.take(),
                            tool_calls: vec![tool_call],
                        });
                    }
                }
                MessageContent::ToolResult(tool_result) => {
                    let content: Vec<llama_cpp::MessagePart> = tool_result
                        .content
                        .iter()
                        .filter_map(|part| match part {
                            LanguageModelToolResultContent::Text(text) => {
                                Some(llama_cpp::MessagePart::Text {
                                    text: text.to_string(),
                                })
                            }
                            LanguageModelToolResultContent::Image(image) => {
                                if supports_images {
                                    Some(llama_cpp::MessagePart::Image {
                                        image_url: llama_cpp::ImageUrl {
                                            url: image.to_base64_url(),
                                            detail: None,
                                        },
                                    })
                                } else {
                                    None
                                }
                            }
                        })
                        .collect();

                    messages.push(llama_cpp::ChatMessage::Tool {
                        content: content.into(),
                        tool_call_id: tool_result.tool_use_id.to_string(),
                    });
                }
            }
        }
    }

    let tools: Vec<llama_cpp::ToolDefinition> = if supports_tools {
        request
            .tools
            .into_iter()
            .map(|tool| llama_cpp::ToolDefinition::Function {
                function: llama_cpp::FunctionDefinition {
                    name: tool.name,
                    description: Some(tool.description),
                    parameters: Some(tool.input_schema),
                },
            })
            .collect()
    } else {
        Vec::new()
    };
    // Only send `tool_choice` with actual tools; some OpenAI-compatible servers
    // reject it otherwise.
    let tool_choice = if tools.is_empty() {
        None
    } else {
        request.tool_choice.map(|choice| match choice {
            LanguageModelToolChoice::Auto => llama_cpp::ToolChoice::Auto,
            LanguageModelToolChoice::Any => llama_cpp::ToolChoice::Required,
            LanguageModelToolChoice::None => llama_cpp::ToolChoice::None,
        })
    };

    llama_cpp::ChatCompletionRequest {
        model: model_name.to_string(),
        messages,
        stream: true,
        // Let the server decide the output length (its `n_predict` default).
        max_tokens: None,
        stop: if request.stop.is_empty() {
            None
        } else {
            Some(request.stop)
        },
        // llama.cpp models often ship recommended sampler settings, so override
        // temperature only when the request sets one.
        temperature: request.temperature,
        tools,
        tool_choice,
        stream_options: Some(llama_cpp::StreamOptions {
            include_usage: true,
        }),
    }
}
