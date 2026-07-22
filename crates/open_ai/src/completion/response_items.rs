use super::request::service_tier_for;
use super::response_helpers::{
    response_message_metadata_from_details, response_message_phase_from_details,
};
use super::*;

pub fn into_open_ai_response(
    request: LanguageModelRequest,
    model_id: &str,
    supports_parallel_tool_calls: bool,
    supports_prompt_cache_key: bool,
    max_output_tokens: Option<u64>,
    default_reasoning_effort: Option<ReasoningEffort>,
    supports_none_reasoning_effort: bool,
) -> ResponseRequest {
    let stream = !model_id.starts_with("o1-");

    let LanguageModelRequest {
        thread_id,
        prompt_id: _,
        intent: _,
        messages,
        tools,
        tool_choice,
        stop: _,
        temperature,
        thinking_allowed,
        thinking_effort,
        speed,
        compact_at_tokens,
    } = request;

    let service_tier = service_tier_for(speed);

    let mut input_items = Vec::new();
    let mut replayed_reasoning_item_indexes = HashMap::default();
    for (index, message) in messages.into_iter().enumerate() {
        append_message_to_response_items(
            message,
            index,
            &mut replayed_reasoning_item_indexes,
            &mut input_items,
        );
    }

    let tools: Vec<_> = tools
        .into_iter()
        .map(|tool| crate::responses::ToolDefinition::Function {
            name: tool.name,
            description: Some(tool.description),
            parameters: Some(tool.input_schema),
            strict: None,
        })
        .collect();

    let default_reasoning_effort =
        default_reasoning_effort.filter(|effort| *effort != ReasoningEffort::None);
    let reasoning_effort = if thinking_allowed {
        thinking_effort
            .as_deref()
            .and_then(|effort| effort.parse::<ReasoningEffort>().ok())
            .filter(|effort| *effort != ReasoningEffort::None)
            .or(default_reasoning_effort)
    } else if supports_none_reasoning_effort {
        Some(ReasoningEffort::None)
    } else {
        None
    };

    let reasoning = reasoning_effort.map(|effort| crate::responses::ReasoningConfig {
        effort,
        summary: if effort == ReasoningEffort::None {
            None
        } else {
            Some(crate::responses::ReasoningSummaryMode::Auto)
        },
    });

    let include = if reasoning
        .as_ref()
        .is_some_and(|reasoning| reasoning.effort != ReasoningEffort::None)
        || input_items
            .iter()
            .any(|item| matches!(item, ResponseInputItem::Reasoning(_)))
    {
        vec![ResponseIncludable::ReasoningEncryptedContent]
    } else {
        Vec::new()
    };

    ResponseRequest {
        model: model_id.into(),
        instructions: None,
        input: input_items,
        store: Some(false),
        include,
        stream,
        temperature,
        top_p: None,
        max_output_tokens,
        parallel_tool_calls: if tools.is_empty() {
            None
        } else {
            Some(supports_parallel_tool_calls)
        },
        tool_choice: tool_choice.map(|choice| match choice {
            LanguageModelToolChoice::Auto => crate::ToolChoice::Auto,
            LanguageModelToolChoice::Any => crate::ToolChoice::Required,
            LanguageModelToolChoice::None => crate::ToolChoice::None,
        }),
        tools,
        prompt_cache_key: if supports_prompt_cache_key {
            thread_id
        } else {
            None
        },
        reasoning,
        service_tier,
        context_management: compact_at_tokens
            .map(|compact_threshold| vec![ContextManagement::Compaction { compact_threshold }]),
    }
}

fn append_message_to_response_items(
    message: LanguageModelRequestMessage,
    index: usize,
    replayed_reasoning_item_indexes: &mut HashMap<String, usize>,
    input_items: &mut Vec<ResponseInputItem>,
) {
    let mut content_parts: Vec<ResponseInputContent> = Vec::new();

    let LanguageModelRequestMessage {
        role,
        content,
        reasoning_details,
        ..
    } = message;
    let phase = if role == Role::Assistant {
        response_message_phase_from_details(reasoning_details.as_deref())
    } else {
        None
    };

    if role == Role::Assistant {
        append_reasoning_details_to_response_items(
            reasoning_details.as_deref(),
            replayed_reasoning_item_indexes,
            input_items,
        );
    }

    for content in content {
        match content {
            MessageContent::Text(text) => {
                push_response_text_part(&role, text, &mut content_parts);
            }
            MessageContent::Thinking { .. } | MessageContent::RedactedThinking(_) => {}
            MessageContent::Compaction(CompactionContent::Encrypted {
                id,
                encrypted_content,
            }) => {
                flush_response_parts(
                    &role,
                    index,
                    phase.as_deref(),
                    &mut content_parts,
                    input_items,
                );
                input_items.push(ResponseInputItem::Compaction(ResponseCompactionItem {
                    id,
                    encrypted_content,
                }));
            }
            // Summary compaction blocks come from other providers, and a
            // Pending block is a streaming-only UI signal; neither is replayed.
            MessageContent::Compaction(
                CompactionContent::Summary { .. } | CompactionContent::Pending,
            ) => {}
            MessageContent::Image(image) => {
                push_response_image_part(&role, image, &mut content_parts);
            }
            MessageContent::ToolUse(tool_use) => {
                flush_response_parts(
                    &role,
                    index,
                    phase.as_deref(),
                    &mut content_parts,
                    input_items,
                );
                let call_id = tool_use.id.to_string();
                input_items.push(ResponseInputItem::FunctionCall(ResponseFunctionCallItem {
                    call_id,
                    name: tool_use.name.to_string(),
                    arguments: tool_use.raw_input,
                }));
            }
            MessageContent::ToolResult(tool_result) => {
                flush_response_parts(
                    &role,
                    index,
                    phase.as_deref(),
                    &mut content_parts,
                    input_items,
                );
                let output = match tool_result.content.as_slice() {
                    [LanguageModelToolResultContent::Text(text)] => {
                        ResponseFunctionCallOutputContent::Text(text.to_string())
                    }
                    _ => {
                        let parts = tool_result
                            .content
                            .into_iter()
                            .map(|part| match part {
                                LanguageModelToolResultContent::Text(text) => {
                                    ResponseInputContent::Text {
                                        text: text.to_string(),
                                    }
                                }
                                LanguageModelToolResultContent::Image(image) => {
                                    ResponseInputContent::Image {
                                        image_url: image.to_base64_url(),
                                    }
                                }
                            })
                            .collect();
                        ResponseFunctionCallOutputContent::List(parts)
                    }
                };
                input_items.push(ResponseInputItem::FunctionCallOutput(
                    ResponseFunctionCallOutputItem {
                        call_id: tool_result.tool_use_id.to_string(),
                        output,
                    },
                ));
            }
        }
    }

    flush_response_parts(
        &role,
        index,
        phase.as_deref(),
        &mut content_parts,
        input_items,
    );
}

fn append_reasoning_details_to_response_items(
    reasoning_details: Option<&serde_json::Value>,
    replayed_reasoning_item_indexes: &mut HashMap<String, usize>,
    input_items: &mut Vec<ResponseInputItem>,
) {
    let Some(reasoning_details) = reasoning_details else {
        return;
    };

    let Some(metadata) = response_message_metadata_from_details(reasoning_details) else {
        return;
    };

    for reasoning_item in metadata.reasoning_items {
        push_replayed_reasoning_item(reasoning_item, replayed_reasoning_item_indexes, input_items);
    }
}

fn push_replayed_reasoning_item(
    reasoning_item: ResponseReasoningInputItem,
    replayed_reasoning_item_indexes: &mut HashMap<String, usize>,
    input_items: &mut Vec<ResponseInputItem>,
) {
    if let Some(id) = reasoning_item.id.as_ref() {
        if let Some(index) = replayed_reasoning_item_indexes.get(id) {
            input_items[*index] = ResponseInputItem::Reasoning(reasoning_item);
            return;
        }

        replayed_reasoning_item_indexes.insert(id.clone(), input_items.len());
    }

    input_items.push(ResponseInputItem::Reasoning(reasoning_item));
}

fn push_response_text_part(
    role: &Role,
    text: impl Into<String>,
    parts: &mut Vec<ResponseInputContent>,
) {
    let text = text.into();
    if text.trim().is_empty() {
        return;
    }

    match role {
        Role::Assistant => parts.push(ResponseInputContent::OutputText {
            text,
            annotations: Vec::new(),
        }),
        _ => parts.push(ResponseInputContent::Text { text }),
    }
}

fn push_response_image_part(
    role: &Role,
    image: LanguageModelImage,
    parts: &mut Vec<ResponseInputContent>,
) {
    match role {
        Role::Assistant => parts.push(ResponseInputContent::OutputText {
            text: "[image omitted]".to_string(),
            annotations: Vec::new(),
        }),
        _ => parts.push(ResponseInputContent::Image {
            image_url: image.to_base64_url(),
        }),
    }
}

fn flush_response_parts(
    role: &Role,
    _index: usize,
    phase: Option<&str>,
    parts: &mut Vec<ResponseInputContent>,
    input_items: &mut Vec<ResponseInputItem>,
) {
    if parts.is_empty() {
        return;
    }

    let item = ResponseInputItem::Message(ResponseMessageItem {
        role: match role {
            Role::User => crate::Role::User,
            Role::Assistant => crate::Role::Assistant,
            Role::System => crate::Role::System,
        },
        content: parts.clone(),
        phase: match role {
            Role::Assistant => phase.map(str::to_string),
            Role::User | Role::System => None,
        },
    });

    input_items.push(item);
    parts.clear();
}
