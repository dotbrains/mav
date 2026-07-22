use super::*;

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct ResponseMessageMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) phase: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) reasoning_items: Vec<ResponseReasoningInputItem>,
}

pub(super) fn response_message_metadata_from_details(
    details: &serde_json::Value,
) -> Option<ResponseMessageMetadata> {
    serde_json::from_value::<ResponseMessageMetadata>(details.clone()).ok()
}

pub(super) fn response_message_phase_from_details(
    details: Option<&serde_json::Value>,
) -> Option<String> {
    let metadata = response_message_metadata_from_details(details?)?;
    metadata
        .phase
        .as_deref()
        .and_then(normalize_response_message_phase)
        .map(str::to_string)
}

pub(super) fn normalize_response_message_phase(phase: &str) -> Option<&'static str> {
    match phase {
        "commentary" => Some(super::RESPONSE_MESSAGE_PHASE_COMMENTARY),
        "final_answer" => Some(super::RESPONSE_MESSAGE_PHASE_FINAL_ANSWER),
        _ => None,
    }
}

pub(super) fn response_failure_message(response: &ResponsesSummary) -> String {
    if let Some(error) = response.error.as_ref() {
        return response_error_message(error);
    }

    response
        .status
        .as_deref()
        .map(|status| format!("response.{status}"))
        .unwrap_or_else(|| "response.failed".to_string())
}

pub(super) fn response_error_message(error: &ResponseError) -> String {
    let code = error.code.as_deref().filter(|code| !code.trim().is_empty());
    let message = error.message.trim();

    match (code, message.is_empty()) {
        (Some(code), false) => format!("{code}: {message}"),
        (Some(code), true) => code.to_string(),
        (None, false) => message.to_string(),
        (None, true) => "response error".to_string(),
    }
}

pub(super) fn response_output_contains_refusal(output: &[ResponseOutputItem]) -> bool {
    output.iter().any(|item| {
        if let ResponseOutputItem::Message(message) = item {
            message.content.iter().any(response_content_is_refusal)
        } else {
            false
        }
    })
}

fn response_content_is_refusal(content: &serde_json::Value) -> bool {
    let content_type = content
        .get("type")
        .and_then(|content_type| content_type.as_str());
    let refusal = content
        .get("refusal")
        .and_then(|refusal| refusal.as_str())
        .unwrap_or_default();

    content_type == Some("refusal") || !refusal.is_empty()
}

pub(super) fn token_usage_from_response_usage(usage: &ResponsesUsage) -> TokenUsage {
    let cache_read_input_tokens = usage.input_tokens_details.cached_tokens;

    TokenUsage {
        input_tokens: usage
            .input_tokens
            .unwrap_or_default()
            .saturating_sub(cache_read_input_tokens),
        output_tokens: usage.output_tokens.unwrap_or_default(),
        cache_creation_input_tokens: 0,
        cache_read_input_tokens,
    }
}

pub(super) fn response_reasoning_input_item_from_output(
    reasoning: &ResponseReasoningItem,
) -> ResponseReasoningInputItem {
    let encrypted_content = reasoning.encrypted_content.clone();

    let summary = reasoning
        .summary
        .iter()
        .filter_map(|part| match part {
            crate::responses::ReasoningSummaryPart::SummaryText { text } => {
                Some(ResponseReasoningSummaryPart::SummaryText { text: text.clone() })
            }
            crate::responses::ReasoningSummaryPart::Unknown => None,
        })
        .collect();

    ResponseReasoningInputItem {
        id: reasoning.id.clone(),
        summary,
        content: reasoning.content.clone(),
        encrypted_content,
        status: reasoning.status.clone(),
    }
}
