use super::*;

pub(super) struct OpenRouterEventMapper {
    tool_calls_by_index: HashMap<usize, RawToolCall>,
    reasoning_details: Option<serde_json::Value>,
}

impl OpenRouterEventMapper {
    pub fn new() -> Self {
        Self {
            tool_calls_by_index: HashMap::default(),
            reasoning_details: None,
        }
    }

    pub fn map_stream(
        mut self,
        events: Pin<
            Box<
                dyn Send + Stream<Item = Result<ResponseStreamEvent, open_router::OpenRouterError>>,
            >,
        >,
    ) -> impl Stream<Item = Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>
    {
        events.flat_map(move |event| {
            futures::stream::iter(match event {
                Ok(event) => self.map_event(event),
                Err(error) => vec![Err(error.into())],
            })
        })
    }

    pub fn map_event(
        &mut self,
        event: ResponseStreamEvent,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        let mut events = Vec::new();

        if let Some(usage) = event.usage {
            let cache_creation_input_tokens = usage
                .prompt_tokens_details
                .as_ref()
                .map_or(0, |details| details.cache_write_tokens);
            let cache_read_input_tokens = usage
                .prompt_tokens_details
                .as_ref()
                .map_or(0, |details| details.cached_tokens);
            let input_tokens = usage.prompt_tokens.saturating_sub(
                cache_creation_input_tokens.saturating_add(cache_read_input_tokens),
            );

            events.push(Ok(LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
                input_tokens,
                output_tokens: usage.completion_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
            })));
        }

        let Some(choice) = event.choices.first() else {
            return events;
        };

        if let Some(details) = choice.delta.reasoning_details.clone() {
            // Emit reasoning_details immediately
            events.push(Ok(LanguageModelCompletionEvent::ReasoningDetails(
                details.clone(),
            )));
            self.reasoning_details = Some(details);
        }

        if let Some(reasoning) = choice.delta.reasoning.clone() {
            events.push(Ok(LanguageModelCompletionEvent::Thinking {
                text: reasoning,
                signature: None,
            }));
        }

        if let Some(content) = choice.delta.content.clone() {
            // OpenRouter send empty content string with the reasoning content
            // This is a workaround for the OpenRouter API bug
            if !content.is_empty() {
                events.push(Ok(LanguageModelCompletionEvent::Text(content)));
            }
        }

        if let Some(tool_calls) = choice.delta.tool_calls.as_ref() {
            for tool_call in tool_calls {
                let entry = self.tool_calls_by_index.entry(tool_call.index).or_default();

                if let Some(tool_id) = tool_call.id.clone() {
                    entry.id = tool_id;
                }

                if let Some(function) = tool_call.function.as_ref() {
                    if let Some(name) = function.name.clone() {
                        entry.name = name;
                    }

                    if let Some(arguments) = function.arguments.clone() {
                        entry.arguments.push_str(&arguments);
                    }

                    if let Some(signature) = function.thought_signature.clone() {
                        entry.thought_signature = Some(signature);
                    }
                }

                if !entry.id.is_empty() && !entry.name.is_empty() {
                    if let Ok(input) = serde_json::from_str::<serde_json::Value>(
                        &fix_streamed_json(&entry.arguments),
                    ) {
                        events.push(Ok(LanguageModelCompletionEvent::ToolUse(
                            LanguageModelToolUse {
                                id: entry.id.clone().into(),
                                name: entry.name.as_str().into(),
                                is_input_complete: false,
                                input,
                                raw_input: entry.arguments.clone(),
                                thought_signature: entry.thought_signature.clone(),
                            },
                        )));
                    }
                }
            }
        }

        match choice.finish_reason.as_deref() {
            Some("stop") => {
                // Don't emit reasoning_details here - already emitted immediately when captured
                events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn)));
            }
            Some("tool_calls") => {
                events.extend(self.tool_calls_by_index.drain().map(|(_, tool_call)| {
                    match parse_tool_arguments(&tool_call.arguments) {
                        Ok(input) => Ok(LanguageModelCompletionEvent::ToolUse(
                            LanguageModelToolUse {
                                id: tool_call.id.clone().into(),
                                name: tool_call.name.as_str().into(),
                                is_input_complete: true,
                                input,
                                raw_input: tool_call.arguments.clone(),
                                thought_signature: tool_call.thought_signature.clone(),
                            },
                        )),
                        Err(error) => Ok(LanguageModelCompletionEvent::ToolUseJsonParseError {
                            id: tool_call.id.clone().into(),
                            tool_name: tool_call.name.as_str().into(),
                            raw_input: tool_call.arguments.clone().into(),
                            json_parse_error: error.to_string(),
                        }),
                    }
                }));

                // Don't emit reasoning_details here - already emitted immediately when captured
                events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::ToolUse)));
            }
            Some(stop_reason) => {
                log::error!("Unexpected OpenRouter stop_reason: {stop_reason:?}",);
                // Don't emit reasoning_details here - already emitted immediately when captured
                events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn)));
            }
            None => {}
        }

        events
    }
}

#[derive(Default)]
struct RawToolCall {
    id: String,
    name: String,
    arguments: String,
    thought_signature: Option<String>,
}
