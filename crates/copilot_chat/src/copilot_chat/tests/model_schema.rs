use super::*;

#[test]
fn test_resilient_model_schema_deserialize() {
    let json = r#"{
          "data": [
            {
              "billing": {
                "is_premium": false,
                "multiplier": 0
              },
              "capabilities": {
                "family": "gpt-4",
                "limits": {
                  "max_context_window_tokens": 32768,
                  "max_output_tokens": 4096,
                  "max_prompt_tokens": 32768
                },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "tokenizer": "cl100k_base",
                "type": "chat"
              },
              "id": "gpt-4",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": false,
              "name": "GPT 4",
              "object": "model",
              "preview": false,
              "vendor": "Azure OpenAI",
              "version": "gpt-4-0613"
            },
            {
                "some-unknown-field": 123
            },
            {
              "billing": {
                "is_premium": true,
                "multiplier": 1,
                "restricted_to": [
                  "pro",
                  "pro_plus",
                  "business",
                  "enterprise"
                ]
              },
              "capabilities": {
                "family": "claude-3.7-sonnet",
                "limits": {
                  "max_context_window_tokens": 200000,
                  "max_output_tokens": 16384,
                  "max_prompt_tokens": 90000,
                  "vision": {
                    "max_prompt_image_size": 3145728,
                    "max_prompt_images": 1,
                    "supported_media_types": ["image/jpeg", "image/png", "image/webp"]
                  }
                },
                "object": "model_capabilities",
                "supports": {
                  "parallel_tool_calls": true,
                  "streaming": true,
                  "tool_calls": true,
                  "vision": true
                },
                "tokenizer": "o200k_base",
                "type": "chat"
              },
              "id": "claude-3.7-sonnet",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Claude 3.7 Sonnet",
              "object": "model",
              "policy": {
                "state": "enabled",
                "terms": "Enable access to the latest Claude 3.7 Sonnet model from Anthropic. [Learn more about how GitHub Copilot serves Claude 3.7 Sonnet](https://docs.github.com/copilot/using-github-copilot/using-claude-sonnet-in-github-copilot)."
              },
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-3.7-sonnet"
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    assert_eq!(schema.data.len(), 2);
    assert_eq!(schema.data[0].id, "gpt-4");
    assert_eq!(schema.data[1].id, "claude-3.7-sonnet");
}

#[test]
fn test_unknown_vendor_resilience() {
    let json = r#"{
          "data": [
            {
              "billing": {
                "is_premium": false,
                "multiplier": 1
              },
              "capabilities": {
                "family": "future-model",
                "limits": {
                  "max_context_window_tokens": 128000,
                  "max_output_tokens": 8192,
                  "max_prompt_tokens": 120000
                },
                "object": "model_capabilities",
                "supports": { "streaming": true, "tool_calls": true },
                "type": "chat"
              },
              "id": "future-model-v1",
              "is_chat_default": false,
              "is_chat_fallback": false,
              "model_picker_enabled": true,
              "name": "Future Model v1",
              "object": "model",
              "preview": false,
              "vendor": "SomeNewVendor",
              "version": "v1.0"
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    assert_eq!(schema.data.len(), 1);
    assert_eq!(schema.data[0].id, "future-model-v1");
    assert_eq!(schema.data[0].vendor, ModelVendor::Unknown);
}

#[test]
fn test_max_token_count_returns_context_window_not_prompt_tokens() {
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
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    // max_token_count() should return context window (200000), not prompt tokens (90000)
    assert_eq!(schema.data[0].max_token_count(), 200000);

    // GPT-4o should return 128000 (context window), not 110000 (prompt tokens)
    assert_eq!(schema.data[1].max_token_count(), 128000);
}

#[test]
fn test_models_with_pending_policy_deserialize() {
    // This test verifies that models with policy states other than "enabled"
    // (such as "pending" or "requires_consent") are properly deserialized.
    // Note: These models will be filtered out by get_models() and won't appear
    // in the model picker until the user enables them on GitHub.
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
              "policy": {
                "state": "pending",
                "terms": "Enable access to Claude models from Anthropic."
              },
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
              "policy": {
                "state": "requires_consent",
                "terms": "Enable access to Claude models from Anthropic."
              },
              "preview": false,
              "vendor": "Anthropic",
              "version": "claude-opus-4"
            }
          ],
          "object": "list"
        }"#;

    let schema: ModelSchema = serde_json::from_str(json).unwrap();

    // Both models should deserialize successfully (filtering happens in get_models)
    assert_eq!(schema.data.len(), 2);
    assert_eq!(schema.data[0].id, "claude-sonnet-4");
    assert_eq!(schema.data[1].id, "claude-opus-4");
}
