use super::*;
use serde_json::json;

#[test]
fn test_function_call_part_with_signature_serializes_correctly() {
    let part = FunctionCallPart {
        function_call: FunctionCall {
            name: "test_function".to_string(),
            args: json!({"arg": "value"}),
            id: None,
        },
        thought_signature: Some("test_signature".to_string()),
    };

    let serialized = serde_json::to_value(&part).unwrap();

    assert_eq!(serialized["functionCall"]["name"], "test_function");
    assert_eq!(serialized["functionCall"]["args"]["arg"], "value");
    assert_eq!(serialized["thoughtSignature"], "test_signature");
}

#[test]
fn test_function_call_part_without_signature_omits_field() {
    let part = FunctionCallPart {
        function_call: FunctionCall {
            name: "test_function".to_string(),
            args: json!({"arg": "value"}),
            id: None,
        },
        thought_signature: None,
    };

    let serialized = serde_json::to_value(&part).unwrap();

    assert_eq!(serialized["functionCall"]["name"], "test_function");
    assert_eq!(serialized["functionCall"]["args"]["arg"], "value");
    // thoughtSignature field should not be present when None
    assert!(serialized.get("thoughtSignature").is_none());
}

#[test]
fn test_function_call_part_deserializes_with_signature() {
    let json = json!({
        "functionCall": {
            "name": "test_function",
            "args": {"arg": "value"}
        },
        "thoughtSignature": "test_signature"
    });

    let part: FunctionCallPart = serde_json::from_value(json).unwrap();

    assert_eq!(part.function_call.name, "test_function");
    assert_eq!(part.thought_signature, Some("test_signature".to_string()));
}

#[test]
fn test_function_call_part_deserializes_without_signature() {
    let json = json!({
        "functionCall": {
            "name": "test_function",
            "args": {"arg": "value"}
        }
    });

    let part: FunctionCallPart = serde_json::from_value(json).unwrap();

    assert_eq!(part.function_call.name, "test_function");
    assert_eq!(part.thought_signature, None);
}

#[test]
fn test_function_call_part_round_trip() {
    let original = FunctionCallPart {
        function_call: FunctionCall {
            name: "test_function".to_string(),
            args: json!({"arg": "value", "nested": {"key": "val"}}),
            id: None,
        },
        thought_signature: Some("round_trip_signature".to_string()),
    };

    let serialized = serde_json::to_value(&original).unwrap();
    let deserialized: FunctionCallPart = serde_json::from_value(serialized).unwrap();

    assert_eq!(deserialized.function_call.name, original.function_call.name);
    assert_eq!(deserialized.function_call.args, original.function_call.args);
    assert_eq!(deserialized.thought_signature, original.thought_signature);
}

#[test]
fn test_function_call_part_with_empty_signature_serializes() {
    let part = FunctionCallPart {
        function_call: FunctionCall {
            name: "test_function".to_string(),
            args: json!({"arg": "value"}),
            id: None,
        },
        thought_signature: Some("".to_string()),
    };

    let serialized = serde_json::to_value(&part).unwrap();

    // Empty string should still be serialized (normalization happens at a higher level)
    assert_eq!(serialized["thoughtSignature"], "");
}
