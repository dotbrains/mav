use super::*;

use serde_json::json;

fn available_model(reasoning_effort: Option<open_ai::ReasoningEffort>) -> AvailableModel {
    AvailableModel {
        name: "custom-model".to_string(),
        display_name: None,
        max_tokens: 128_000,
        max_output_tokens: None,
        max_completion_tokens: None,
        reasoning_effort,
        capabilities: ModelCapabilities {
            chat_completions: false,
            ..Default::default()
        },
    }
}

#[test]
fn configured_reasoning_effort_supports_thinking() {
    assert_eq!(
        default_thinking_reasoning_effort(&available_model(Some(open_ai::ReasoningEffort::High))),
        Some(open_ai::ReasoningEffort::High)
    );
}

#[test]
fn missing_or_none_reasoning_effort_does_not_support_thinking() {
    assert_eq!(
        default_thinking_reasoning_effort(&available_model(None)),
        None
    );
    assert_eq!(
        default_thinking_reasoning_effort(&available_model(Some(open_ai::ReasoningEffort::None))),
        None
    );
}

#[test]
fn supported_thinking_effort_levels_use_configured_effort_as_default() {
    let effort_levels =
        supported_thinking_effort_levels(&available_model(Some(open_ai::ReasoningEffort::High)));
    let values = effort_levels
        .iter()
        .map(|level| level.value.as_ref())
        .collect::<Vec<_>>();

    assert_eq!(values, ["minimal", "low", "medium", "high", "xhigh", "max"]);
    assert_eq!(
        effort_levels
            .iter()
            .find(|level| level.is_default)
            .map(|level| level.value.as_ref()),
        Some("high")
    );
}

#[test]
fn supported_thinking_effort_levels_hide_missing_or_none_effort() {
    assert!(supported_thinking_effort_levels(&available_model(None)).is_empty());
    assert!(
        supported_thinking_effort_levels(&available_model(Some(open_ai::ReasoningEffort::None)))
            .is_empty()
    );
}

#[test]
fn chat_completion_reasoning_effort_honors_request_and_configured_effort() {
    let model = available_model(Some(open_ai::ReasoningEffort::Medium));
    let mut request = LanguageModelRequest {
        thinking_allowed: true,
        ..Default::default()
    };

    assert_eq!(
        chat_completion_reasoning_effort(&request, &model),
        Some(open_ai::ReasoningEffort::Medium)
    );

    request.thinking_effort = Some("high".to_string());
    assert_eq!(
        chat_completion_reasoning_effort(&request, &model),
        Some(open_ai::ReasoningEffort::High)
    );

    request.thinking_effort = Some("not-supported".to_string());
    assert_eq!(
        chat_completion_reasoning_effort(&request, &model),
        Some(open_ai::ReasoningEffort::Medium)
    );

    request.thinking_allowed = false;
    assert_eq!(
        chat_completion_reasoning_effort(&request, &model),
        Some(open_ai::ReasoningEffort::None)
    );
}

#[test]
fn chat_completion_reasoning_effort_omits_missing_effort() {
    let model = available_model(None);
    let request = LanguageModelRequest {
        thinking_allowed: false,
        ..Default::default()
    };

    assert_eq!(chat_completion_reasoning_effort(&request, &model), None);
}

#[test]
fn chat_completion_reasoning_effort_preserves_explicit_none() {
    let model = available_model(Some(open_ai::ReasoningEffort::None));
    let request = LanguageModelRequest {
        thinking_allowed: true,
        thinking_effort: Some("high".to_string()),
        ..Default::default()
    };

    assert_eq!(
        chat_completion_reasoning_effort(&request, &model),
        Some(open_ai::ReasoningEffort::None)
    );
}

#[test]
fn chat_completion_max_tokens_parameter_defaults_to_max_completion_tokens() {
    let model = available_model(Some(open_ai::ReasoningEffort::Medium));

    assert_eq!(
        chat_completion_max_tokens_parameter(&model),
        crate::provider::open_ai::ChatCompletionMaxTokensParameter::MaxCompletionTokens
    );
}

#[test]
fn chat_completion_max_tokens_parameter_uses_max_tokens_when_configured() {
    let mut model = available_model(Some(open_ai::ReasoningEffort::Medium));
    model.capabilities.max_tokens_parameter = true;

    assert_eq!(
        chat_completion_max_tokens_parameter(&model),
        crate::provider::open_ai::ChatCompletionMaxTokensParameter::MaxTokens
    );
}

#[test]
fn response_request_includes_reasoning_when_effort_is_configured() {
    let model = available_model(Some(open_ai::ReasoningEffort::High));
    let request = LanguageModelRequest {
        thinking_allowed: true,
        ..Default::default()
    };

    let request = into_open_ai_response(
        request,
        &model.name,
        model.capabilities.parallel_tool_calls,
        model.capabilities.prompt_cache_key,
        model.max_output_tokens,
        default_thinking_reasoning_effort(&model),
        supports_none_reasoning_effort(&model),
    );
    let serialized = serde_json::to_value(request).unwrap();

    assert_eq!(
        serialized["reasoning"],
        json!({ "effort": "high", "summary": "auto" })
    );
    assert_eq!(
        serialized["include"],
        json!(["reasoning.encrypted_content"])
    );
}

#[test]
fn response_request_omits_reasoning_when_effort_is_missing() {
    let model = available_model(None);
    let request = LanguageModelRequest {
        thinking_allowed: true,
        ..Default::default()
    };

    let request = into_open_ai_response(
        request,
        &model.name,
        model.capabilities.parallel_tool_calls,
        model.capabilities.prompt_cache_key,
        model.max_output_tokens,
        default_thinking_reasoning_effort(&model),
        supports_none_reasoning_effort(&model),
    );
    let serialized = serde_json::to_value(request).unwrap();

    assert_eq!(serialized.get("reasoning"), None);
    assert_eq!(serialized.get("include"), None);
}

#[test]
fn chat_completion_request_includes_selected_reasoning_effort() {
    let mut model = available_model(Some(open_ai::ReasoningEffort::Medium));
    model.capabilities.chat_completions = true;
    let request = LanguageModelRequest {
        thinking_allowed: true,
        thinking_effort: Some("high".to_string()),
        ..Default::default()
    };
    let reasoning_effort = chat_completion_reasoning_effort(&request, &model);

    let request = into_open_ai(
        request,
        &model.name,
        model.capabilities.parallel_tool_calls,
        model.capabilities.prompt_cache_key,
        model.max_output_tokens,
        chat_completion_max_tokens_parameter(&model),
        reasoning_effort,
        model.capabilities.interleaved_reasoning,
    );
    let serialized = serde_json::to_value(request).unwrap();

    assert_eq!(serialized["reasoning_effort"], json!("high"));
}

#[test]
fn configured_reasoning_effort_supports_none_reasoning_effort() {
    assert!(supports_none_reasoning_effort(&available_model(Some(
        open_ai::ReasoningEffort::Medium
    ))));
    assert!(supports_none_reasoning_effort(&available_model(Some(
        open_ai::ReasoningEffort::None
    ))));
    assert!(!supports_none_reasoning_effort(&available_model(None)));
}

#[test]
fn response_thinking_effort_preserves_explicit_none() {
    let model = available_model(Some(open_ai::ReasoningEffort::None));
    let mut request = LanguageModelRequest {
        thinking_allowed: true,
        thinking_effort: Some("high".to_string()),
        ..Default::default()
    };

    disable_response_thinking_for_none_effort(&mut request, &model);
    assert!(!request.thinking_allowed);
    assert_eq!(request.thinking_effort, None);
}
