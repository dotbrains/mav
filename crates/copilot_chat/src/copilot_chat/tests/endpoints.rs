use super::*;

#[test]
fn test_supports_response_method() {
    // Test the supports_response() method which determines endpoint routing.
    let model_with_responses_only = Model {
        billing: ModelBilling {
            is_premium: false,
            multiplier: 1.0,
            restricted_to: None,
        },
        capabilities: ModelCapabilities {
            family: "test".to_string(),
            limits: ModelLimits::default(),
            supports: ModelSupportedFeatures {
                streaming: true,
                tool_calls: true,
                parallel_tool_calls: false,
                vision: false,
                thinking: false,
                adaptive_thinking: false,
                max_thinking_budget: None,
                min_thinking_budget: None,
                reasoning_effort: vec![],
            },
            model_type: "chat".to_string(),
            tokenizer: None,
        },
        id: "test-model".to_string(),
        name: "Test Model".to_string(),
        policy: None,
        vendor: ModelVendor::OpenAI,
        is_chat_default: false,
        is_chat_fallback: false,
        model_picker_enabled: true,
        supported_endpoints: vec![ModelSupportedEndpoint::Responses],
    };

    let model_with_chat_completions = Model {
        supported_endpoints: vec![ModelSupportedEndpoint::ChatCompletions],
        ..model_with_responses_only.clone()
    };

    let model_with_both = Model {
        supported_endpoints: vec![
            ModelSupportedEndpoint::ChatCompletions,
            ModelSupportedEndpoint::Responses,
        ],
        ..model_with_responses_only.clone()
    };

    let model_with_messages = Model {
        supported_endpoints: vec![ModelSupportedEndpoint::Messages],
        ..model_with_responses_only.clone()
    };

    // Only /responses endpoint -> supports_response = true
    assert!(model_with_responses_only.supports_response());

    // Only /chat/completions endpoint -> supports_response = false
    assert!(!model_with_chat_completions.supports_response());

    // Both endpoints (has /chat/completions) -> supports_response = false
    assert!(model_with_both.supports_response());

    // Only /v1/messages endpoint -> supports_response = false (doesn't have /responses)
    assert!(!model_with_messages.supports_response());
}

#[test]
fn test_tool_choice_required_serializes_as_required() {
    // Regression test: ToolChoice::Required must serialize as "required" (not "any")
    // for OpenAI-compatible APIs. Reverting the rename would break this.
    assert_eq!(
        serde_json::to_string(&ToolChoice::Required).unwrap(),
        "\"required\""
    );
    assert_eq!(
        serde_json::to_string(&ToolChoice::Auto).unwrap(),
        "\"auto\""
    );
    assert_eq!(
        serde_json::to_string(&ToolChoice::None).unwrap(),
        "\"none\""
    );
}
