use super::*;

pub(super) fn map_to_language_model_completion_events(
    events: Pin<Box<dyn Send + Stream<Item = Result<ResponseEvent>>>>,
    is_streaming: bool,
) -> impl Stream<Item = Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
    #[derive(Default)]
    struct RawToolCall {
        id: String,
        name: String,
        arguments: String,
        thought_signature: Option<String>,
    }

    struct State {
        events: Pin<Box<dyn Send + Stream<Item = Result<ResponseEvent>>>>,
        tool_calls_by_index: HashMap<usize, RawToolCall>,
        reasoning_opaque: Option<String>,
        reasoning_text: Option<String>,
    }

    futures::stream::unfold(
        State {
            events,
            tool_calls_by_index: HashMap::default(),
            reasoning_opaque: None,
            reasoning_text: None,
        },
        move |mut state| async move {
            if let Some(event) = state.events.next().await {
                match event {
                    Ok(event) => {
                        let Some(choice) = event.choices.first() else {
                            return Some((
                                vec![Err(anyhow!("Response contained no choices").into())],
                                state,
                            ));
                        };

                        let delta = if is_streaming {
                            choice.delta.as_ref()
                        } else {
                            choice.message.as_ref()
                        };

                        let Some(delta) = delta else {
                            return Some((
                                vec![Err(anyhow!("Response contained no delta").into())],
                                state,
                            ));
                        };

                        let mut events = Vec::new();
                        if let Some(content) = delta.content.clone() {
                            events.push(Ok(LanguageModelCompletionEvent::Text(content)));
                        }

                        // Capture reasoning data from the delta (e.g. for Gemini 3)
                        if let Some(opaque) = delta.reasoning_opaque.clone() {
                            state.reasoning_opaque = Some(opaque);
                        }
                        if let Some(text) = delta.reasoning_text.clone() {
                            state.reasoning_text = Some(text);
                        }

                        for (index, tool_call) in delta.tool_calls.iter().enumerate() {
                            let tool_index = tool_call.index.unwrap_or(index);
                            let entry = state.tool_calls_by_index.entry(tool_index).or_default();

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

                                if let Some(thought_signature) = function.thought_signature.clone()
                                {
                                    entry.thought_signature = Some(thought_signature);
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

                        if let Some(usage) = event.usage {
                            events.push(Ok(LanguageModelCompletionEvent::UsageUpdate(
                                TokenUsage {
                                    input_tokens: usage.prompt_tokens,
                                    output_tokens: usage.completion_tokens,
                                    cache_creation_input_tokens: 0,
                                    cache_read_input_tokens: 0,
                                },
                            )));
                        }

                        match choice.finish_reason.as_deref() {
                            Some("stop") => {
                                events.push(Ok(LanguageModelCompletionEvent::Stop(
                                    StopReason::EndTurn,
                                )));
                            }
                            Some("tool_calls") => {
                                // Gemini 3 models send reasoning_opaque/reasoning_text that must
                                // be preserved and sent back in subsequent requests. Emit as
                                // ReasoningDetails so the agent stores it in the message.
                                if state.reasoning_opaque.is_some()
                                    || state.reasoning_text.is_some()
                                {
                                    let mut details = serde_json::Map::new();
                                    if let Some(opaque) = state.reasoning_opaque.take() {
                                        details.insert(
                                            "reasoning_opaque".to_string(),
                                            serde_json::Value::String(opaque),
                                        );
                                    }
                                    if let Some(text) = state.reasoning_text.take() {
                                        details.insert(
                                            "reasoning_text".to_string(),
                                            serde_json::Value::String(text),
                                        );
                                    }
                                    events.push(Ok(
                                        LanguageModelCompletionEvent::ReasoningDetails(
                                            serde_json::Value::Object(details),
                                        ),
                                    ));
                                }

                                events.extend(state.tool_calls_by_index.drain().map(
                                    |(_, tool_call)| match parse_tool_arguments(
                                        &tool_call.arguments,
                                    ) {
                                        Ok(input) => Ok(LanguageModelCompletionEvent::ToolUse(
                                            LanguageModelToolUse {
                                                id: tool_call.id.into(),
                                                name: tool_call.name.as_str().into(),
                                                is_input_complete: true,
                                                input,
                                                raw_input: tool_call.arguments,
                                                thought_signature: tool_call.thought_signature,
                                            },
                                        )),
                                        Err(error) => Ok(
                                            LanguageModelCompletionEvent::ToolUseJsonParseError {
                                                id: tool_call.id.into(),
                                                tool_name: tool_call.name.as_str().into(),
                                                raw_input: tool_call.arguments.into(),
                                                json_parse_error: error.to_string(),
                                            },
                                        ),
                                    },
                                ));

                                events.push(Ok(LanguageModelCompletionEvent::Stop(
                                    StopReason::ToolUse,
                                )));
                            }
                            Some(stop_reason) => {
                                log::error!("Unexpected Copilot Chat stop_reason: {stop_reason:?}");
                                events.push(Ok(LanguageModelCompletionEvent::Stop(
                                    StopReason::EndTurn,
                                )));
                            }
                            None => {}
                        }

                        return Some((events, state));
                    }
                    Err(err) => return Some((vec![Err(anyhow!(err).into())], state)),
                }
            }

            None
        },
    )
    .flat_map(futures::stream::iter)
}
