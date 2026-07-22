use super::*;

pub(super) struct LlamaCppEventMapper {
    tool_calls_by_index: HashMap<usize, RawToolCall>,
}

impl LlamaCppEventMapper {
    pub(super) fn new() -> Self {
        Self {
            tool_calls_by_index: HashMap::default(),
        }
    }

    pub fn map_stream(
        mut self,
        events: Pin<Box<dyn Send + Stream<Item = Result<llama_cpp::ResponseStreamEvent>>>>,
    ) -> impl Stream<Item = Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>
    {
        events.flat_map(move |event| {
            futures::stream::iter(match event {
                Ok(event) => self.map_event(event),
                Err(error) => vec![Err(LanguageModelCompletionError::from(error))],
            })
        })
    }

    pub fn map_event(
        &mut self,
        event: llama_cpp::ResponseStreamEvent,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        let mut events = Vec::new();

        if let Some(usage) = event.usage {
            events.push(Ok(LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            })));
        }

        if let Some(choice) = event.choices.into_iter().next() {
            if let Some(reasoning_content) = choice.delta.reasoning_content {
                events.push(Ok(LanguageModelCompletionEvent::Thinking {
                    text: reasoning_content,
                    signature: None,
                }));
            }

            if let Some(content) = choice.delta.content {
                if !content.is_empty() {
                    events.push(Ok(LanguageModelCompletionEvent::Text(content)));
                }
            }

            if let Some(tool_calls) = choice.delta.tool_calls {
                for tool_call in tool_calls {
                    let entry = self.tool_calls_by_index.entry(tool_call.index).or_default();

                    if let Some(tool_id) = tool_call.id {
                        entry.id = tool_id;
                    }

                    if let Some(function) = tool_call.function {
                        if let Some(name) = function.name {
                            // Only the first chunk carries the function name;
                            // later chunks send an empty name with arguments.
                            if !name.is_empty() {
                                entry.name = name;
                            }
                        }

                        if let Some(arguments) = function.arguments {
                            entry.arguments.push_str(&arguments);
                        }
                    }
                }
            }

            if let Some(finish_reason) = choice.finish_reason.as_deref() {
                match finish_reason {
                    "stop" => {
                        events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn)));
                    }
                    "tool_calls" => {
                        events.extend(self.tool_calls_by_index.drain().map(|(_, tool_call)| {
                            match parse_tool_arguments(&tool_call.arguments) {
                                Ok(input) => Ok(LanguageModelCompletionEvent::ToolUse(
                                    LanguageModelToolUse {
                                        id: tool_call.id.into(),
                                        name: tool_call.name.into(),
                                        is_input_complete: true,
                                        input,
                                        raw_input: tool_call.arguments,
                                        thought_signature: None,
                                    },
                                )),
                                Err(error) => {
                                    Ok(LanguageModelCompletionEvent::ToolUseJsonParseError {
                                        id: tool_call.id.into(),
                                        tool_name: tool_call.name.into(),
                                        raw_input: tool_call.arguments.into(),
                                        json_parse_error: error.to_string(),
                                    })
                                }
                            }
                        }));

                        events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::ToolUse)));
                    }
                    "length" => {
                        events.push(Ok(LanguageModelCompletionEvent::Stop(
                            StopReason::MaxTokens,
                        )));
                    }
                    unexpected => {
                        log::warn!("Unexpected llama.cpp finish_reason: {unexpected:?}");
                        events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn)));
                    }
                }
            }
        }

        events
    }
}

#[derive(Default)]
struct RawToolCall {
    id: String,
    name: String,
    arguments: String,
}

pub(super) fn add_message_content_part(
    new_part: llama_cpp::MessagePart,
    role: Role,
    messages: &mut Vec<llama_cpp::ChatMessage>,
    reasoning_content: Option<String>,
) {
    match (role, messages.last_mut()) {
        (Role::User, Some(llama_cpp::ChatMessage::User { content }))
        | (Role::System, Some(llama_cpp::ChatMessage::System { content })) => {
            content.push_part(new_part);
        }
        (
            Role::Assistant,
            Some(llama_cpp::ChatMessage::Assistant {
                content: Some(content),
                reasoning_content: message_reasoning_content,
                ..
            }),
        ) => {
            append_reasoning_content(message_reasoning_content, reasoning_content);
            content.push_part(new_part);
        }
        _ => {
            messages.push(match role {
                Role::User => llama_cpp::ChatMessage::User {
                    content: llama_cpp::MessageContent::from(vec![new_part]),
                },
                Role::Assistant => llama_cpp::ChatMessage::Assistant {
                    content: Some(llama_cpp::MessageContent::from(vec![new_part])),
                    reasoning_content,
                    tool_calls: Vec::new(),
                },
                Role::System => llama_cpp::ChatMessage::System {
                    content: llama_cpp::MessageContent::from(vec![new_part]),
                },
            });
        }
    }
}

pub(super) fn append_reasoning_content(target: &mut Option<String>, content: Option<String>) {
    let Some(content) = content else {
        return;
    };
    if content.is_empty() {
        return;
    }
    target.get_or_insert_default().push_str(&content);
}
