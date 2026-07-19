
use super::*;

#[test]
fn test_from_cloud_failure_with_upstream_http_error() {
    let error = LanguageModelCompletionError::from_cloud_failure(
            String::from("anthropic").into(),
            "upstream_http_error".to_string(),
            r#"{"code":"upstream_http_error","message":"Received an error from the Anthropic API: upstream connect error or disconnect/reset before headers. reset reason: connection timeout","upstream_status":503}"#.to_string(),
            None,
        );

    match error {
        LanguageModelCompletionError::ServerOverloaded { provider, .. } => {
            assert_eq!(provider.0, "anthropic");
        }
        _ => panic!(
            "Expected ServerOverloaded error for 503 status, got: {:?}",
            error
        ),
    }

    let error = LanguageModelCompletionError::from_cloud_failure(
        String::from("anthropic").into(),
        "upstream_http_error".to_string(),
        r#"{"code":"upstream_http_error","message":"Internal server error","upstream_status":500}"#
            .to_string(),
        None,
    );

    match error {
        LanguageModelCompletionError::ApiInternalServerError { provider, message } => {
            assert_eq!(provider.0, "anthropic");
            assert_eq!(message, "Internal server error");
        }
        _ => panic!(
            "Expected ApiInternalServerError for 500 status, got: {:?}",
            error
        ),
    }
}

#[test]
fn test_from_cloud_failure_with_standard_format() {
    let error = LanguageModelCompletionError::from_cloud_failure(
        String::from("anthropic").into(),
        "upstream_http_503".to_string(),
        "Service unavailable".to_string(),
        None,
    );

    match error {
        LanguageModelCompletionError::ServerOverloaded { provider, .. } => {
            assert_eq!(provider.0, "anthropic");
        }
        _ => panic!("Expected ServerOverloaded error for upstream_http_503"),
    }
}

#[test]
fn test_upstream_http_error_connection_timeout() {
    let error = LanguageModelCompletionError::from_cloud_failure(
            String::from("anthropic").into(),
            "upstream_http_error".to_string(),
            r#"{"code":"upstream_http_error","message":"Received an error from the Anthropic API: upstream connect error or disconnect/reset before headers. reset reason: connection timeout","upstream_status":503}"#.to_string(),
            None,
        );

    match error {
        LanguageModelCompletionError::ServerOverloaded { provider, .. } => {
            assert_eq!(provider.0, "anthropic");
        }
        _ => panic!(
            "Expected ServerOverloaded error for connection timeout with 503 status, got: {:?}",
            error
        ),
    }

    let error = LanguageModelCompletionError::from_cloud_failure(
            String::from("anthropic").into(),
            "upstream_http_error".to_string(),
            r#"{"code":"upstream_http_error","message":"Received an error from the Anthropic API: upstream connect error or disconnect/reset before headers. reset reason: connection timeout","upstream_status":500}"#.to_string(),
            None,
        );

    match error {
        LanguageModelCompletionError::ApiInternalServerError { provider, message } => {
            assert_eq!(provider.0, "anthropic");
            assert_eq!(
                message,
                "Received an error from the Anthropic API: upstream connect error or disconnect/reset before headers. reset reason: connection timeout"
            );
        }
        _ => panic!(
            "Expected ApiInternalServerError for connection timeout with 500 status, got: {:?}",
            error
        ),
    }
}

#[test]
fn test_language_model_tool_use_serializes_with_signature() {
    use serde_json::json;

    let tool_use = LanguageModelToolUse {
        id: LanguageModelToolUseId::from("test_id"),
        name: "test_tool".into(),
        raw_input: json!({"arg": "value"}).to_string(),
        input: json!({"arg": "value"}),
        is_input_complete: true,
        thought_signature: Some("test_signature".to_string()),
    };

    let serialized = serde_json::to_value(&tool_use).unwrap();

    assert_eq!(serialized["id"], "test_id");
    assert_eq!(serialized["name"], "test_tool");
    assert_eq!(serialized["thought_signature"], "test_signature");
}

#[test]
fn test_language_model_tool_use_deserializes_with_missing_signature() {
    use serde_json::json;

    let json = json!({
        "id": "test_id",
        "name": "test_tool",
        "raw_input": "{\"arg\":\"value\"}",
        "input": {"arg": "value"},
        "is_input_complete": true
    });

    let tool_use: LanguageModelToolUse = serde_json::from_value(json).unwrap();

    assert_eq!(tool_use.id, LanguageModelToolUseId::from("test_id"));
    assert_eq!(tool_use.name.as_ref(), "test_tool");
    assert_eq!(tool_use.thought_signature, None);
}

#[test]
fn test_language_model_tool_use_round_trip_with_signature() {
    use serde_json::json;

    let original = LanguageModelToolUse {
        id: LanguageModelToolUseId::from("round_trip_id"),
        name: "round_trip_tool".into(),
        raw_input: json!({"key": "value"}).to_string(),
        input: json!({"key": "value"}),
        is_input_complete: true,
        thought_signature: Some("round_trip_sig".to_string()),
    };

    let serialized = serde_json::to_value(&original).unwrap();
    let deserialized: LanguageModelToolUse = serde_json::from_value(serialized).unwrap();

    assert_eq!(deserialized.id, original.id);
    assert_eq!(deserialized.name, original.name);
    assert_eq!(deserialized.thought_signature, original.thought_signature);
}

#[test]
fn test_language_model_tool_use_round_trip_without_signature() {
    use serde_json::json;

    let original = LanguageModelToolUse {
        id: LanguageModelToolUseId::from("no_sig_id"),
        name: "no_sig_tool".into(),
        raw_input: json!({"arg": "value"}).to_string(),
        input: json!({"arg": "value"}),
        is_input_complete: true,
        thought_signature: None,
    };

    let serialized = serde_json::to_value(&original).unwrap();
    let deserialized: LanguageModelToolUse = serde_json::from_value(serialized).unwrap();

    assert_eq!(deserialized.id, original.id);
    assert_eq!(deserialized.name, original.name);
    assert_eq!(deserialized.thought_signature, None);
}
