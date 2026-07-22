use super::*;
use responses_mapper::append_reasoning_details_to_response_items;

pub(super) fn into_copilot_chat(
    model: &CopilotChatModel,
    request: LanguageModelRequest,
) -> Result<CopilotChatRequest> {
    let temperature = request.temperature;
    let tool_choice = request.tool_choice;
    let thinking_allowed = request.thinking_allowed;

    let mut request_messages: Vec<LanguageModelRequestMessage> = Vec::new();
    for message in request.messages {
        if let Some(last_message) = request_messages.last_mut() {
            if last_message.role == message.role {
                last_message.content.extend(message.content);
            } else {
                request_messages.push(message);
            }
        } else {
            request_messages.push(message);
        }
    }

    let mut messages: Vec<ChatMessage> = Vec::new();
    for message in request_messages {
        match message.role {
            Role::User => {
                for content in &message.content {
                    if let MessageContent::ToolResult(tool_result) = content {
                        let parts: Vec<ChatMessagePart> = tool_result
                        .content
                        .iter()
                        .map(|part| match part {
                            LanguageModelToolResultContent::Text(text) => {
                                ChatMessagePart::Text {
                                    text: text.to_string(),
                                }
                            }
                            LanguageModelToolResultContent::Image(image) => {
                                if model.supports_vision() {
                                    ChatMessagePart::Image {
                                        image_url: ImageUrl {
                                            url: image.to_base64_url(),
                                        },
                                    }
                                } else {
                                    debug_panic!(
                                        "This should be caught at {} level",
                                        tool_result.tool_name
                                    );
                                    ChatMessagePart::Text {
                                        text: "[Tool responded with an image, but this model does not support vision]".to_string(),
                                    }
                                }
                            }
                        })
                        .collect();

                        let content = match parts.as_slice() {
                            [ChatMessagePart::Text { text }] => {
                                ChatMessageContent::Plain(text.clone())
                            }
                            _ => ChatMessageContent::Multipart(parts),
                        };

                        messages.push(ChatMessage::Tool {
                            tool_call_id: tool_result.tool_use_id.to_string(),
                            content,
                        });
                    }
                }

                let mut content_parts = Vec::new();
                for content in &message.content {
                    match content {
                        MessageContent::Text(text) | MessageContent::Thinking { text, .. }
                            if !text.is_empty() =>
                        {
                            if let Some(ChatMessagePart::Text { text: text_content }) =
                                content_parts.last_mut()
                            {
                                text_content.push_str(text);
                            } else {
                                content_parts.push(ChatMessagePart::Text {
                                    text: text.to_string(),
                                });
                            }
                        }
                        MessageContent::Image(image) if model.supports_vision() => {
                            content_parts.push(ChatMessagePart::Image {
                                image_url: ImageUrl {
                                    url: image.to_base64_url(),
                                },
                            });
                        }
                        _ => {}
                    }
                }

                if !content_parts.is_empty() {
                    messages.push(ChatMessage::User {
                        content: content_parts.into(),
                    });
                }
            }
            Role::Assistant => {
                let mut tool_calls = Vec::new();
                for content in &message.content {
                    if let MessageContent::ToolUse(tool_use) = content {
                        tool_calls.push(ToolCall {
                            id: tool_use.id.to_string(),
                            content: ToolCallContent::Function {
                                function: FunctionContent {
                                    name: tool_use.name.to_string(),
                                    arguments: serde_json::to_string(&tool_use.input)?,
                                    thought_signature: tool_use.thought_signature.clone(),
                                },
                            },
                        });
                    }
                }

                let text_content = {
                    let mut buffer = String::new();
                    for string in message.content.iter().filter_map(|content| match content {
                        MessageContent::Text(text) => Some(text.as_str()),
                        MessageContent::Thinking { .. }
                        | MessageContent::ToolUse(_)
                        | MessageContent::RedactedThinking(_)
                        | MessageContent::ToolResult(_)
                        | MessageContent::Image(_)
                        | MessageContent::Compaction(_) => None,
                    }) {
                        buffer.push_str(string);
                    }

                    buffer
                };

                // Extract reasoning_opaque and reasoning_text from reasoning_details
                let (reasoning_opaque, reasoning_text) =
                    if let Some(details) = &message.reasoning_details {
                        let opaque = details
                            .get("reasoning_opaque")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let text = details
                            .get("reasoning_text")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        (opaque, text)
                    } else {
                        (None, None)
                    };

                messages.push(ChatMessage::Assistant {
                    content: if text_content.is_empty() {
                        ChatMessageContent::empty()
                    } else {
                        text_content.into()
                    },
                    tool_calls,
                    reasoning_opaque,
                    reasoning_text,
                });
            }
            Role::System => messages.push(ChatMessage::System {
                content: message.string_contents(),
            }),
        }
    }

    let tools = request
        .tools
        .iter()
        .map(|tool| Tool::Function {
            function: Function {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.input_schema.clone(),
            },
        })
        .collect::<Vec<_>>();

    Ok(CopilotChatRequest {
        n: 1,
        stream: model.uses_streaming(),
        temperature: temperature.unwrap_or(0.1),
        model: model.id().to_string(),
        messages,
        tools,
        tool_choice: tool_choice.map(|choice| match choice {
            LanguageModelToolChoice::Auto => ToolChoice::Auto,
            LanguageModelToolChoice::Any => ToolChoice::Required,
            LanguageModelToolChoice::None => ToolChoice::None,
        }),
        thinking_budget: if thinking_allowed && model.supports_thinking() {
            compute_thinking_budget(
                model.min_thinking_budget(),
                model.max_thinking_budget(),
                model.max_output_tokens() as u32,
            )
        } else {
            None
        },
    })
}

pub(super) fn compute_thinking_budget(
    min_budget: Option<u32>,
    max_budget: Option<u32>,
    max_output_tokens: u32,
) -> Option<u32> {
    let configured_budget: u32 = 16000;
    let min_budget = min_budget.unwrap_or(1024);
    let max_budget = max_budget.unwrap_or(max_output_tokens.saturating_sub(1));
    let normalized = configured_budget.max(min_budget);
    Some(
        normalized
            .min(max_budget)
            .min(max_output_tokens.saturating_sub(1)),
    )
}

pub(super) fn intent_to_chat_location(intent: Option<CompletionIntent>) -> ChatLocation {
    match intent {
        Some(CompletionIntent::UserPrompt) => ChatLocation::Agent,
        Some(CompletionIntent::Subagent) => ChatLocation::Agent,
        Some(CompletionIntent::ToolResults) => ChatLocation::Agent,
        Some(CompletionIntent::ThreadSummarization) => ChatLocation::Panel,
        Some(CompletionIntent::ThreadContextSummarization) => ChatLocation::Panel,
        Some(CompletionIntent::CreateFile) => ChatLocation::Agent,
        Some(CompletionIntent::EditFile) => ChatLocation::Agent,
        Some(CompletionIntent::InlineAssist) => ChatLocation::Editor,
        Some(CompletionIntent::TerminalInlineAssist) => ChatLocation::Terminal,
        Some(CompletionIntent::GenerateGitCommitMessage) => ChatLocation::Other,
        None => ChatLocation::Panel,
    }
}

pub(super) fn into_copilot_responses(
    model: &CopilotChatModel,
    request: LanguageModelRequest,
) -> copilot_responses::Request {
    use copilot_responses as responses;

    let LanguageModelRequest {
        thread_id: _,
        prompt_id: _,
        intent: _,
        messages,
        tools,
        tool_choice,
        stop: _,
        temperature,
        thinking_allowed,
        thinking_effort,
        speed: _,
        compact_at_tokens: _,
    } = request;

    let mut input_items: Vec<responses::ResponseInputItem> = Vec::new();
    let mut replayed_reasoning_item_indexes = HashMap::default();

    for message in messages {
        match message.role {
            Role::User => {
                for content in &message.content {
                    if let MessageContent::ToolResult(tool_result) = content {
                        let output = match tool_result.content.as_slice() {
                            [LanguageModelToolResultContent::Text(text)] => {
                                responses::ResponseFunctionOutput::Text(text.to_string())
                            }
                            _ => {
                                let parts = tool_result
                                .content
                                .iter()
                                .map(|part| match part {
                                    LanguageModelToolResultContent::Text(text) => {
                                        responses::ResponseInputContent::InputText {
                                            text: text.to_string(),
                                        }
                                    }
                                    LanguageModelToolResultContent::Image(image) => {
                                        if model.supports_vision() {
                                            responses::ResponseInputContent::InputImage {
                                                image_url: Some(image.to_base64_url()),
                                                detail: Default::default(),
                                            }
                                        } else {
                                            debug_panic!(
                                                "This should be caught at {} level",
                                                tool_result.tool_name
                                            );
                                            responses::ResponseInputContent::InputText {
                                                text: "[Tool responded with an image, but this model does not support vision]".to_string(),
                                            }
                                        }
                                    }
                                })
                                .collect();
                                responses::ResponseFunctionOutput::Content(parts)
                            }
                        };

                        input_items.push(responses::ResponseInputItem::FunctionCallOutput {
                            call_id: tool_result.tool_use_id.to_string(),
                            output,
                            status: None,
                        });
                    }
                }

                let mut parts: Vec<responses::ResponseInputContent> = Vec::new();
                for content in &message.content {
                    match content {
                        MessageContent::Text(text) => {
                            parts.push(responses::ResponseInputContent::InputText {
                                text: text.clone(),
                            });
                        }

                        MessageContent::Image(image) => {
                            if model.supports_vision() {
                                parts.push(responses::ResponseInputContent::InputImage {
                                    image_url: Some(image.to_base64_url()),
                                    detail: Default::default(),
                                });
                            }
                        }
                        _ => {}
                    }
                }

                if !parts.is_empty() {
                    input_items.push(responses::ResponseInputItem::Message {
                        role: "user".into(),
                        content: Some(parts),
                        status: None,
                    });
                }
            }

            Role::Assistant => {
                append_reasoning_details_to_response_items(
                    message.reasoning_details.as_deref(),
                    &mut replayed_reasoning_item_indexes,
                    &mut input_items,
                );

                for content in &message.content {
                    if let MessageContent::ToolUse(tool_use) = content {
                        input_items.push(responses::ResponseInputItem::FunctionCall {
                            call_id: tool_use.id.to_string(),
                            name: tool_use.name.to_string(),
                            arguments: tool_use.raw_input.clone(),
                            status: None,
                            thought_signature: tool_use.thought_signature.clone(),
                        });
                    }
                }

                let mut parts: Vec<responses::ResponseInputContent> = Vec::new();
                for content in &message.content {
                    match content {
                        MessageContent::Text(text) => {
                            parts.push(responses::ResponseInputContent::OutputText {
                                text: text.clone(),
                            });
                        }
                        MessageContent::Image(_) => {
                            parts.push(responses::ResponseInputContent::OutputText {
                                text: "[image omitted]".to_string(),
                            });
                        }
                        _ => {}
                    }
                }

                if !parts.is_empty() {
                    input_items.push(responses::ResponseInputItem::Message {
                        role: "assistant".into(),
                        content: Some(parts),
                        status: Some("completed".into()),
                    });
                }
            }

            Role::System => {
                let mut parts: Vec<responses::ResponseInputContent> = Vec::new();
                for content in &message.content {
                    if let MessageContent::Text(text) = content {
                        parts.push(responses::ResponseInputContent::InputText {
                            text: text.clone(),
                        });
                    }
                }

                if !parts.is_empty() {
                    input_items.push(responses::ResponseInputItem::Message {
                        role: "system".into(),
                        content: Some(parts),
                        status: None,
                    });
                }
            }
        }
    }

    let converted_tools: Vec<responses::ToolDefinition> = tools
        .into_iter()
        .map(|tool| responses::ToolDefinition::Function {
            name: tool.name,
            description: Some(tool.description),
            parameters: Some(tool.input_schema),
            strict: None,
        })
        .collect();

    let mapped_tool_choice = tool_choice.map(|choice| match choice {
        LanguageModelToolChoice::Auto => responses::ToolChoice::Auto,
        LanguageModelToolChoice::Any => responses::ToolChoice::Required,
        LanguageModelToolChoice::None => responses::ToolChoice::None,
    });

    responses::Request {
        model: model.id().to_string(),
        input: input_items,
        stream: model.uses_streaming(),
        temperature,
        tools: converted_tools,
        tool_choice: mapped_tool_choice,
        reasoning: if thinking_allowed {
            let effort = thinking_effort
                .as_deref()
                .and_then(|e| e.parse::<copilot_responses::ReasoningEffort>().ok())
                .unwrap_or(copilot_responses::ReasoningEffort::Medium);
            Some(copilot_responses::ReasoningConfig {
                effort,
                summary: Some(copilot_responses::ReasoningSummary::Detailed),
            })
        } else {
            None
        },
        include: Some(vec![
            copilot_responses::ResponseIncludable::ReasoningEncryptedContent,
        ]),
        store: false,
    }
}
