use super::*;

#[test]
fn test_multiple_anthropic_models_preserved() {
    // This test verifies that multiple Claude models from Anthropic
    // are all preserved and not incorrectly deduplicated.
    // This was the root cause of issue #47540.
    let json = r#"{
          "data": [
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "claude-sonnet-4",
                "limits": { "max_context_window_tokens": 200000, "max_output_tokens": 16384, "max_prompt_tokens": 90000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "claude-sonnet-4",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Claude Sonnet 4",
              "object": "model",
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-sonnet-4"
            },
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "claude-opus-4",
                "limits": { "max_context_window_tokens": 200000, "max_output_tokens": 16384, "max_prompt_tokens": 90000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "claude-opus-4",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Claude Opus 4",
              "object": "model",
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-opus-4"
            },
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "claude-sonnet-4.5",
                "limits": { "max_context_window_tokens": 200000, "max_output_tokens": 16384, "max_prompt_tokens": 90000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "claude-sonnet-4.5",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Claude Sonnet 4.5",
              "object": "model",
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-sonnet-4.5"
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    // All three Anthropic models should be preserved
    assert_eq!(schema.data.len(), 3);
    assert_eq!(schema.data[0].id, "claude-sonnet-4");
    assert_eq!(schema.data[1].id, "claude-opus-4");
    assert_eq!(schema.data[2].id, "claude-sonnet-4.5");
}

#[test]
fn test_models_with_same_family_both_preserved() {
    // Test that models sharing the same family (e.g., thinking variants)
    // are both preserved in the model list.
    let json = r#"{
          "data": [
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "claude-sonnet-4",
                "limits": { "max_context_window_tokens": 200000, "max_output_tokens": 16384, "max_prompt_tokens": 90000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "claude-sonnet-4",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Claude Sonnet 4",
              "object": "model",
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-sonnet-4"
            },
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "claude-sonnet-4",
                "limits": { "max_context_window_tokens": 200000, "max_output_tokens": 16384, "max_prompt_tokens": 90000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "claude-sonnet-4-thinking",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Claude Sonnet 4 (Thinking)",
              "object": "model",
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-sonnet-4-thinking"
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    // Both models should be preserved even though they share the same family
    assert_eq!(schema.data.len(), 2);
    assert_eq!(schema.data[0].id, "claude-sonnet-4");
    assert_eq!(schema.data[1].id, "claude-sonnet-4-thinking");
}

#[test]
fn test_mixed_vendor_models_all_preserved() {
    // Test that models from different vendors are all preserved.
    let json = r#"{
          "data": [
            {
              "billing": { "is_premium": false, "multiplier": 1 },
              "capabilities": {
                "family": "gpt-4o",
                "limits": { "max_context_window_tokens": 128000, "max_output_tokens": 16384, "max_prompt_tokens": 110000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "gpt-4o",
              "is_chat_default": true,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "GPT-4o",
              "object": "model",
              "preview": false,
              "vendor": "Azure OpenAI",
              "version": "gpt-4o"
            },
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "claude-sonnet-4",
                "limits": { "max_context_window_tokens": 200000, "max_output_tokens": 16384, "max_prompt_tokens": 90000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "claude-sonnet-4",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Claude Sonnet 4",
              "object": "model",
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-sonnet-4"
            },
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "gemini-2.0-flash",
                "limits": { "max_context_window_tokens": 1000000, "max_output_tokens": 8192, "max_prompt_tokens": 900000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "gemini-2.0-flash",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Gemini 2.0 Flash",
              "object": "model",
              "preview": false,
              "vendor": "Google",
              "version": "gemini-2.0-flash"
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    // All three models from different vendors should be preserved
    assert_eq!(schema.data.len(), 3);
    assert_eq!(schema.data[0].id, "gpt-4o");
    assert_eq!(schema.data[1].id, "claude-sonnet-4");
    assert_eq!(schema.data[2].id, "gemini-2.0-flash");
}

#[test]
fn test_model_with_messages_endpoint_deserializes() {
    // Anthropic Claude models use /v1/messages endpoint.
    // This test verifies such models deserialize correctly (issue #47540 root cause).
    let json = r#"{
          "data": [
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "claude-sonnet-4",
                "limits": { "max_context_window_tokens": 200000, "max_output_tokens": 16384, "max_prompt_tokens": 90000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "claude-sonnet-4",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Claude Sonnet 4",
              "object": "model",
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-sonnet-4",
              "supported_endpoints": ["/v1/messages"]
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    assert_eq!(schema.data.len(), 1);
    assert_eq!(schema.data[0].id, "claude-sonnet-4");
    assert_eq!(
        schema.data[0].supported_endpoints,
        vec![ModelSupportedEndpoint::Messages]
    );
}

#[test]
fn test_model_with_unknown_endpoint_deserializes() {
    // Future-proofing: unknown endpoints should deserialize to Unknown variant
    // instead of causing the entire model to fail deserialization.
    let json = r#"{
          "data": [
            {
              "billing": { "is_premium": false, "multiplier": 1 },
              "capabilities": {
                "family": "future-model",
                "limits": { "max_context_window_tokens": 128000, "max_output_tokens": 8192, "max_prompt_tokens": 120000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "future-model-v2",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Future Model v2",
              "object": "model",
              "preview": false,
              "vendor": "OpenAI",
              "version": "v2.0",
              "supported_endpoints": ["/v2/completions", "/chat/completions"]
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    assert_eq!(schema.data.len(), 1);
    assert_eq!(schema.data[0].id, "future-model-v2");
    assert_eq!(
        schema.data[0].supported_endpoints,
        vec![
            ModelSupportedEndpoint::Unknown,
            ModelSupportedEndpoint::ChatCompletions
        ]
    );
}

#[test]
fn test_model_with_multiple_endpoints() {
    // Test model with multiple supported endpoints (common for newer models).
    let json = r#"{
          "data": [
            {
              "billing": { "is_premium": true, "multiplier": 1 },
              "capabilities": {
                "family": "gpt-4o",
                "limits": { "max_context_window_tokens": 128000, "max_output_tokens": 16384, "max_prompt_tokens": 110000 },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "gpt-4o",
              "is_chat_default": true,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "GPT-4o",
              "object": "model",
              "preview": false,
              "vendor": "OpenAI",
              "version": "gpt-4o",
              "supported_endpoints": ["/chat/completions", "/responses"]
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    assert_eq!(schema.data.len(), 1);
    assert_eq!(schema.data[0].id, "gpt-4o");
    assert_eq!(
        schema.data[0].supported_endpoints,
        vec![
            ModelSupportedEndpoint::ChatCompletions,
            ModelSupportedEndpoint::Responses
        ]
    );
}
