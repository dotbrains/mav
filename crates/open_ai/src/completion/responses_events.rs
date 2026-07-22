use super::response_helpers::{
    ResponseMessageMetadata, normalize_response_message_phase, response_error_message,
    response_failure_message, response_output_contains_refusal,
    response_reasoning_input_item_from_output, token_usage_from_response_usage,
};
use super::*;

pub struct OpenAiResponseEventMapper {
    function_calls_by_item: HashMap<String, PendingResponseFunctionCall>,
    reasoning_items: Vec<ResponseReasoningInputItem>,
    current_message_phase: Option<String>,
    pending_stop_reason: Option<StopReason>,
}

#[derive(Default)]
struct PendingResponseFunctionCall {
    call_id: String,
    name: Arc<str>,
    arguments: String,
}

impl OpenAiResponseEventMapper {
    pub fn new() -> Self {
        Self {
            function_calls_by_item: HashMap::default(),
            reasoning_items: Vec::new(),
            current_message_phase: None,
            pending_stop_reason: None,
        }
    }

    pub fn map_stream(
        mut self,
        events: Pin<Box<dyn Send + Stream<Item = Result<ResponsesStreamEvent>>>>,
    ) -> impl Stream<Item = Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>
    {
        events.flat_map(move |event| {
            futures::stream::iter(match event {
                Ok(event) => self.map_event(event),
                Err(error) => vec![Err(LanguageModelCompletionError::from(anyhow!(error)))],
            })
        })
    }

    pub fn map_event(
        &mut self,
        event: ResponsesStreamEvent,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        match event {
            ResponsesStreamEvent::OutputItemAdded { item, .. } => {
                let mut events = Vec::new();

                match &item {
                    ResponseOutputItem::Message(message) => {
                        if let Some(id) = &message.id {
                            events.push(Ok(LanguageModelCompletionEvent::StartMessage {
                                message_id: id.clone(),
                            }));
                        }
                        events.extend(self.capture_message_phase(message));
                    }
                    ResponseOutputItem::FunctionCall(function_call) => {
                        if let Some(item_id) = function_call.id.clone() {
                            let call_id = function_call
                                .call_id
                                .clone()
                                .or_else(|| function_call.id.clone())
                                .unwrap_or_else(|| item_id.clone());
                            let entry = PendingResponseFunctionCall {
                                call_id,
                                name: Arc::<str>::from(
                                    function_call.name.clone().unwrap_or_default(),
                                ),
                                arguments: function_call.arguments.clone(),
                            };
                            self.function_calls_by_item.insert(item_id, entry);
                        }
                    }
                    ResponseOutputItem::Compaction(_) => {
                        events.push(Ok(LanguageModelCompletionEvent::Compaction(
                            CompactionContent::Pending,
                        )));
                    }
                    ResponseOutputItem::Reasoning(_) | ResponseOutputItem::Unknown => {}
                }
                events
            }
            ResponsesStreamEvent::ReasoningSummaryTextDelta { delta, .. } => {
                if delta.is_empty() {
                    Vec::new()
                } else {
                    vec![Ok(LanguageModelCompletionEvent::Thinking {
                        text: delta,
                        signature: None,
                    })]
                }
            }
            ResponsesStreamEvent::OutputTextDelta { delta, .. } => {
                if delta.is_empty() {
                    Vec::new()
                } else {
                    vec![Ok(LanguageModelCompletionEvent::Text(delta))]
                }
            }
            ResponsesStreamEvent::RefusalDelta { .. }
            | ResponsesStreamEvent::RefusalDone { .. } => {
                self.pending_stop_reason = Some(StopReason::Refusal);
                Vec::new()
            }
            ResponsesStreamEvent::FunctionCallArgumentsDelta { item_id, delta, .. } => {
                if let Some(entry) = self.function_calls_by_item.get_mut(&item_id) {
                    entry.arguments.push_str(&delta);
                    if let Ok(input) = serde_json::from_str::<serde_json::Value>(
                        &fix_streamed_json(&entry.arguments),
                    ) {
                        return vec![Ok(LanguageModelCompletionEvent::ToolUse(
                            LanguageModelToolUse {
                                id: LanguageModelToolUseId::from(entry.call_id.clone()),
                                name: entry.name.clone(),
                                is_input_complete: false,
                                input,
                                raw_input: entry.arguments.clone(),
                                thought_signature: None,
                            },
                        ))];
                    }
                }
                Vec::new()
            }
            ResponsesStreamEvent::FunctionCallArgumentsDone {
                item_id, arguments, ..
            } => {
                if let Some(mut entry) = self.function_calls_by_item.remove(&item_id) {
                    if !arguments.is_empty() {
                        entry.arguments = arguments;
                    }
                    let raw_input = entry.arguments.clone();
                    self.pending_stop_reason = Some(StopReason::ToolUse);
                    match parse_tool_arguments(&entry.arguments) {
                        Ok(input) => {
                            vec![Ok(LanguageModelCompletionEvent::ToolUse(
                                LanguageModelToolUse {
                                    id: LanguageModelToolUseId::from(entry.call_id.clone()),
                                    name: entry.name.clone(),
                                    is_input_complete: true,
                                    input,
                                    raw_input,
                                    thought_signature: None,
                                },
                            ))]
                        }
                        Err(error) => {
                            vec![Ok(LanguageModelCompletionEvent::ToolUseJsonParseError {
                                id: LanguageModelToolUseId::from(entry.call_id.clone()),
                                tool_name: entry.name.clone(),
                                raw_input: Arc::<str>::from(raw_input),
                                json_parse_error: error.to_string(),
                            })]
                        }
                    }
                } else {
                    Vec::new()
                }
            }
            ResponsesStreamEvent::Completed { response } => {
                self.handle_completion(response, StopReason::EndTurn)
            }
            ResponsesStreamEvent::Incomplete { response } => {
                let reason = response
                    .incomplete_details
                    .as_ref()
                    .and_then(|details| details.reason.as_deref());
                let mut stop_reason = match reason {
                    Some("max_tokens" | "max_output_tokens") => StopReason::MaxTokens,
                    Some("content_filter") => {
                        self.pending_stop_reason = Some(StopReason::Refusal);
                        StopReason::Refusal
                    }
                    _ => self
                        .pending_stop_reason
                        .take()
                        .unwrap_or(StopReason::EndTurn),
                };

                let mut events = Vec::new();
                events.extend(self.capture_reasoning_items_from_output(&response.output));
                if response_output_contains_refusal(&response.output)
                    && !matches!(stop_reason, StopReason::MaxTokens)
                {
                    self.pending_stop_reason = Some(StopReason::Refusal);
                    stop_reason = StopReason::Refusal;
                }
                if self.pending_stop_reason.is_none() {
                    events.extend(self.emit_tool_calls_from_output(&response.output));
                }
                if let Some(usage) = response.usage.as_ref() {
                    events.push(Ok(LanguageModelCompletionEvent::UsageUpdate(
                        token_usage_from_response_usage(usage),
                    )));
                }
                events.push(Ok(LanguageModelCompletionEvent::Stop(stop_reason)));
                events
            }
            ResponsesStreamEvent::Failed { response } => {
                let message = response_failure_message(&response);
                vec![Err(LanguageModelCompletionError::Other(anyhow!(message)))]
            }
            ResponsesStreamEvent::Error { error } => {
                vec![Err(LanguageModelCompletionError::Other(anyhow!(
                    response_error_message(&error)
                )))]
            }
            ResponsesStreamEvent::GenericError { error } => {
                let error = error.into_response_error();
                vec![Err(LanguageModelCompletionError::Other(anyhow!(
                    response_error_message(&error)
                )))]
            }
            ResponsesStreamEvent::ReasoningSummaryPartAdded { summary_index, .. } => {
                if summary_index > 0 {
                    vec![Ok(LanguageModelCompletionEvent::Thinking {
                        text: "\n\n".to_string(),
                        signature: None,
                    })]
                } else {
                    Vec::new()
                }
            }
            ResponsesStreamEvent::OutputItemDone { item, .. } => match item {
                ResponseOutputItem::Reasoning(reasoning) => self.capture_reasoning_item(&reasoning),
                ResponseOutputItem::Message(message) => self.capture_message_phase(&message),
                ResponseOutputItem::Compaction(compaction) => {
                    vec![Ok(LanguageModelCompletionEvent::Compaction(
                        CompactionContent::Encrypted {
                            id: compaction.id,
                            encrypted_content: compaction.encrypted_content,
                        },
                    ))]
                }
                ResponseOutputItem::FunctionCall(_) | ResponseOutputItem::Unknown => Vec::new(),
            },
            ResponsesStreamEvent::OutputTextDone { .. }
            | ResponsesStreamEvent::ContentPartAdded { .. }
            | ResponsesStreamEvent::ContentPartDone { .. }
            | ResponsesStreamEvent::ReasoningSummaryTextDone { .. }
            | ResponsesStreamEvent::ReasoningSummaryPartDone { .. }
            | ResponsesStreamEvent::Created { .. }
            | ResponsesStreamEvent::InProgress { .. }
            | ResponsesStreamEvent::Unknown => Vec::new(),
        }
    }

    fn handle_completion(
        &mut self,
        response: ResponsesSummary,
        default_reason: StopReason,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        let mut events = Vec::new();

        events.extend(self.capture_reasoning_items_from_output(&response.output));

        if response_output_contains_refusal(&response.output) {
            self.pending_stop_reason = Some(StopReason::Refusal);
        }

        if self.pending_stop_reason.is_none() {
            events.extend(self.emit_tool_calls_from_output(&response.output));
        }

        if let Some(usage) = response.usage.as_ref() {
            events.push(Ok(LanguageModelCompletionEvent::UsageUpdate(
                token_usage_from_response_usage(usage),
            )));
        }

        let stop_reason = self.pending_stop_reason.take().unwrap_or(default_reason);
        events.push(Ok(LanguageModelCompletionEvent::Stop(stop_reason)));
        events
    }

    fn emit_tool_calls_from_output(
        &mut self,
        output: &[ResponseOutputItem],
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        let mut events = Vec::new();
        for item in output {
            if let ResponseOutputItem::FunctionCall(function_call) = item {
                let Some(call_id) = function_call
                    .call_id
                    .clone()
                    .or_else(|| function_call.id.clone())
                else {
                    log::error!(
                        "Function call item missing both call_id and id: {:?}",
                        function_call
                    );
                    continue;
                };
                let name: Arc<str> = Arc::from(function_call.name.clone().unwrap_or_default());
                let arguments = &function_call.arguments;
                self.pending_stop_reason = Some(StopReason::ToolUse);
                match parse_tool_arguments(arguments) {
                    Ok(input) => {
                        events.push(Ok(LanguageModelCompletionEvent::ToolUse(
                            LanguageModelToolUse {
                                id: LanguageModelToolUseId::from(call_id.clone()),
                                name: name.clone(),
                                is_input_complete: true,
                                input,
                                raw_input: arguments.clone(),
                                thought_signature: None,
                            },
                        )));
                    }
                    Err(error) => {
                        events.push(Ok(LanguageModelCompletionEvent::ToolUseJsonParseError {
                            id: LanguageModelToolUseId::from(call_id.clone()),
                            tool_name: name.clone(),
                            raw_input: Arc::<str>::from(arguments.clone()),
                            json_parse_error: error.to_string(),
                        }));
                    }
                }
            }
        }
        events
    }

    fn capture_reasoning_items_from_output(
        &mut self,
        output: &[ResponseOutputItem],
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        let mut events = Vec::new();
        for item in output {
            if let ResponseOutputItem::Reasoning(reasoning) = item {
                events.extend(self.capture_reasoning_item(reasoning));
            }
        }
        events
    }

    fn capture_message_phase(
        &mut self,
        message: &ResponseOutputMessage,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        self.current_message_phase = message
            .phase
            .as_deref()
            .and_then(normalize_response_message_phase)
            .map(str::to_string);

        if self.current_message_phase.is_none() && self.reasoning_items.is_empty() {
            return Vec::new();
        }

        self.emit_response_message_metadata()
    }

    fn capture_reasoning_item(
        &mut self,
        reasoning: &ResponseReasoningItem,
    ) -> Vec<Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
        let reasoning_item = response_reasoning_input_item_from_output(reasoning);

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
        let details = serde_json::to_value(ResponseMessageMetadata {
            phase: self.current_message_phase.clone(),
            reasoning_items: self.reasoning_items.clone(),
        });

        match details {
            Ok(details) => vec![Ok(LanguageModelCompletionEvent::ReasoningDetails(details))],
            Err(error) => vec![Err(LanguageModelCompletionError::Other(anyhow!(error)))],
        }
    }
}
