use super::*;

pub(super) struct CopilotResponsesEventMapper {
    pending_stop_reason: Option<StopReason>,
    reasoning_items: Vec<copilot_responses::ResponseReasoningInputItem>,
}

impl CopilotResponsesEventMapper {
    pub fn new() -> Self {
        Self {
            pending_stop_reason: None,
            reasoning_items: Vec::new(),
        }
    }

    pub fn map_stream(
        mut self,
        events: Pin<Box<dyn Send + Stream<Item = Result<copilot_responses::StreamEvent>>>>,
    ) -> impl Stream<Item = Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>
    {
        events.flat_map(move |event| {
            futures::stream::iter(match event {
                Ok(event) => self.map_event(event),
                Err(error) => vec![Err(LanguageModelCompletionError::from(anyhow!(error)))],
            })
        })
    }

    fn map_event(
        &mut self,
        event: copilot_responses::StreamEvent,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        match event {
            copilot_responses::StreamEvent::OutputItemAdded { item, .. } => match item {
                copilot_responses::ResponseOutputItem::Message { id, .. } => {
                    vec![Ok(LanguageModelCompletionEvent::StartMessage {
                        message_id: id,
                    })]
                }
                _ => Vec::new(),
            },

            copilot_responses::StreamEvent::OutputTextDelta { delta, .. } => {
                if delta.is_empty() {
                    Vec::new()
                } else {
                    vec![Ok(LanguageModelCompletionEvent::Text(delta))]
                }
            }

            copilot_responses::StreamEvent::OutputItemDone { item, .. } => match item {
                copilot_responses::ResponseOutputItem::Message { .. } => Vec::new(),
                copilot_responses::ResponseOutputItem::FunctionCall {
                    call_id,
                    name,
                    arguments,
                    thought_signature,
                    ..
                } => {
                    let mut events = Vec::new();
                    match parse_tool_arguments(&arguments) {
                        Ok(input) => events.push(Ok(LanguageModelCompletionEvent::ToolUse(
                            LanguageModelToolUse {
                                id: call_id.into(),
                                name: name.as_str().into(),
                                is_input_complete: true,
                                input,
                                raw_input: arguments.clone(),
                                thought_signature,
                            },
                        ))),
                        Err(error) => {
                            events.push(Ok(LanguageModelCompletionEvent::ToolUseJsonParseError {
                                id: call_id.into(),
                                tool_name: name.as_str().into(),
                                raw_input: arguments.clone().into(),
                                json_parse_error: error.to_string(),
                            }))
                        }
                    }
                    // Record that we already emitted a tool-use stop so we can avoid duplicating
                    // a Stop event on Completed.
                    self.pending_stop_reason = Some(StopReason::ToolUse);
                    events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::ToolUse)));
                    events
                }
                copilot_responses::ResponseOutputItem::Reasoning {
                    id,
                    summary,
                    encrypted_content,
                } => {
                    let mut events = Vec::new();

                    if let Some(blocks) = summary.as_ref() {
                        let mut text = String::new();
                        for block in blocks {
                            text.push_str(&block.text);
                        }
                        if !text.is_empty() {
                            events.push(Ok(LanguageModelCompletionEvent::Thinking {
                                text,
                                signature: None,
                            }));
                        }
                    }

                    if let Some(reasoning_item) =
                        reasoning_input_item_from_output(&id, encrypted_content)
                    {
                        events.extend(self.capture_reasoning_item(reasoning_item));
                    }

                    events
                }
            },

            copilot_responses::StreamEvent::Completed { response } => {
                let mut events = Vec::new();
                if let Some(usage) = response.usage {
                    events.push(Ok(LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
                        input_tokens: usage.input_tokens.unwrap_or(0),
                        output_tokens: usage.output_tokens.unwrap_or(0),
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                    })));
                }
                if self.pending_stop_reason.take() != Some(StopReason::ToolUse) {
                    events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn)));
                }
                events
            }

            copilot_responses::StreamEvent::Incomplete { response } => {
                let reason = response
                    .incomplete_details
                    .as_ref()
                    .and_then(|details| details.reason.as_ref());
                let stop_reason = match reason {
                    Some(copilot_responses::IncompleteReason::MaxOutputTokens) => {
                        StopReason::MaxTokens
                    }
                    Some(copilot_responses::IncompleteReason::ContentFilter) => StopReason::Refusal,
                    _ => self
                        .pending_stop_reason
                        .take()
                        .unwrap_or(StopReason::EndTurn),
                };

                let mut events = Vec::new();
                if let Some(usage) = response.usage {
                    events.push(Ok(LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
                        input_tokens: usage.input_tokens.unwrap_or(0),
                        output_tokens: usage.output_tokens.unwrap_or(0),
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                    })));
                }
                events.push(Ok(LanguageModelCompletionEvent::Stop(stop_reason)));
                events
            }

            copilot_responses::StreamEvent::Failed { response } => {
                let provider = PROVIDER_NAME;
                let (status_code, message) = match response.error {
                    Some(error) => {
                        let status_code = StatusCode::from_str(&error.code)
                            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                        (status_code, error.message)
                    }
                    None => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "response.failed".to_string(),
                    ),
                };
                vec![Err(LanguageModelCompletionError::HttpResponseError {
                    provider,
                    status_code,
                    message,
                })]
            }

            copilot_responses::StreamEvent::GenericError { error } => vec![Err(
                LanguageModelCompletionError::Other(anyhow!(error.message)),
            )],

            copilot_responses::StreamEvent::Created { .. }
            | copilot_responses::StreamEvent::Unknown => Vec::new(),
        }
    }

    fn capture_reasoning_item(
        &mut self,
        reasoning_item: copilot_responses::ResponseReasoningInputItem,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        if self.reasoning_items.contains(&reasoning_item) {
            return Vec::new();
        }

        if let Some(id) = reasoning_item.id.as_ref()
            && let Some(existing_reasoning_item) = self
                .reasoning_items
                .iter_mut()
                .find(|existing_reasoning_item| existing_reasoning_item.id.as_ref() == Some(id))
        {
            *existing_reasoning_item = reasoning_item;
        } else {
            self.reasoning_items.push(reasoning_item);
        }

        self.emit_response_message_metadata()
    }

    fn emit_response_message_metadata(
        &self,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        let details = serde_json::to_value(CopilotResponseMessageMetadata {
            reasoning_items: self.reasoning_items.clone(),
        });

        match details {
            Ok(details) => vec![Ok(LanguageModelCompletionEvent::ReasoningDetails(details))],
            Err(error) => vec![Err(LanguageModelCompletionError::Other(anyhow!(error)))],
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct CopilotResponseMessageMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reasoning_items: Vec<copilot_responses::ResponseReasoningInputItem>,
}

pub(super) fn append_reasoning_details_to_response_items(
    reasoning_details: Option<&serde_json::Value>,
    replayed_reasoning_item_indexes: &mut HashMap<String, usize>,
    input_items: &mut Vec<copilot_responses::ResponseInputItem>,
) {
    let Some(reasoning_details) = reasoning_details else {
        return;
    };

    let Some(metadata) =
        serde_json::from_value::<CopilotResponseMessageMetadata>(reasoning_details.clone()).ok()
    else {
        return;
    };

    for mut reasoning_item in metadata.reasoning_items {
        reasoning_item.summary.clear();
        if let Some(id) = reasoning_item.id.as_ref() {
            if let Some(index) = replayed_reasoning_item_indexes.get(id) {
                input_items[*index] =
                    copilot_responses::ResponseInputItem::Reasoning(reasoning_item);
                return;
            }

            replayed_reasoning_item_indexes.insert(id.clone(), input_items.len());
        }

        input_items.push(copilot_responses::ResponseInputItem::Reasoning(
            reasoning_item,
        ));
    }
}

fn reasoning_input_item_from_output(
    id: &str,
    encrypted_content: Option<String>,
) -> Option<copilot_responses::ResponseReasoningInputItem> {
    if encrypted_content.is_none() {
        return None;
    }
    Some(copilot_responses::ResponseReasoningInputItem {
        id: Some(id.to_string()),
        summary: Vec::new(),
        encrypted_content,
    })
}
