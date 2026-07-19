use language_model::LanguageModelRequest;

use super::{reasoning_effort_for_request, supported_thinking_effort_levels};

#[test]
fn grok_43_supports_selectable_thinking_effort_levels() {
    let effort_levels = supported_thinking_effort_levels(&x_ai::Model::Grok43);
    let values = effort_levels
        .iter()
        .map(|level| level.value.as_ref())
        .collect::<Vec<_>>();

    assert_eq!(values, ["low", "medium", "high"]);
    assert_eq!(
        effort_levels
            .iter()
            .find(|level| level.is_default)
            .map(|level| level.value.as_ref()),
        Some("low")
    );
}

#[test]
fn grok_43_request_uses_selected_reasoning_effort() {
    let request = LanguageModelRequest {
        thinking_allowed: true,
        thinking_effort: Some("high".to_string()),
        ..Default::default()
    };

    assert_eq!(
        reasoning_effort_for_request(&request, &x_ai::Model::Grok43),
        Some(open_ai::ReasoningEffort::High)
    );
}

#[test]
fn grok_43_request_uses_none_when_thinking_is_disabled() {
    let request = LanguageModelRequest {
        thinking_allowed: false,
        ..Default::default()
    };

    assert_eq!(
        reasoning_effort_for_request(&request, &x_ai::Model::Grok43),
        Some(open_ai::ReasoningEffort::None)
    );
}
