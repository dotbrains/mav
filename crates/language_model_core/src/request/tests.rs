use super::*;

#[test]
fn test_language_model_tool_result_content_deserialization() {
    // Test plain string
    let json = serde_json::json!("hello world");
    let content: LanguageModelToolResultContent = serde_json::from_value(json).unwrap();
    assert_eq!(
        content,
        LanguageModelToolResultContent::Text(Arc::from("hello world"))
    );

    // Test wrapped text format: { "type": "text", "text": "..." }
    let json = serde_json::json!({"type": "text", "text": "hello"});
    let content: LanguageModelToolResultContent = serde_json::from_value(json).unwrap();
    assert_eq!(
        content,
        LanguageModelToolResultContent::Text(Arc::from("hello"))
    );

    // Test single-field text object: { "text": "..." }
    let json = serde_json::json!({"text": "hello"});
    let content: LanguageModelToolResultContent = serde_json::from_value(json).unwrap();
    assert_eq!(
        content,
        LanguageModelToolResultContent::Text(Arc::from("hello"))
    );

    // Test case-insensitive type field
    let json = serde_json::json!({"Type": "Text", "Text": "hello"});
    let content: LanguageModelToolResultContent = serde_json::from_value(json).unwrap();
    assert_eq!(
        content,
        LanguageModelToolResultContent::Text(Arc::from("hello"))
    );

    // Test image object
    let json = serde_json::json!({
        "source": "base64encodedimagedata",
    });
    let content: LanguageModelToolResultContent = serde_json::from_value(json).unwrap();
    match content {
        LanguageModelToolResultContent::Image(image) => {
            assert_eq!(image.source.as_ref(), "base64encodedimagedata");
        }
        _ => panic!("Expected Image variant"),
    }

    // Test wrapped image: { "image": { "source": "...", "size": ... } }
    let json = serde_json::json!({
        "image": {
            "source": "wrappedimagedata",
        }
    });
    let content: LanguageModelToolResultContent = serde_json::from_value(json).unwrap();
    match content {
        LanguageModelToolResultContent::Image(image) => {
            assert_eq!(image.source.as_ref(), "wrappedimagedata");
        }
        _ => panic!("Expected Image variant"),
    }

    // Test case insensitive
    let json = serde_json::json!({
        "Source": "caseinsensitive",
    });
    let content: LanguageModelToolResultContent = serde_json::from_value(json).unwrap();
    match content {
        LanguageModelToolResultContent::Image(image) => {
            assert_eq!(image.source.as_ref(), "caseinsensitive");
        }
        _ => panic!("Expected Image variant"),
    }

    // Test direct image object
    let json = serde_json::json!({
        "source": "directimage",
    });
    let content: LanguageModelToolResultContent = serde_json::from_value(json).unwrap();
    match content {
        LanguageModelToolResultContent::Image(image) => {
            assert_eq!(image.source.as_ref(), "directimage");
        }
        _ => panic!("Expected Image variant"),
    }
}

#[test]
fn test_language_model_tool_result_content_vec_deserialization() {
    // Legacy single-value shape is normalized to a Vec.
    let json = serde_json::json!({
        "tool_use_id": "abc",
        "tool_name": "echo",
        "is_error": false,
        "content": "hello",
        "output": null,
    });
    let result: LanguageModelToolResult = serde_json::from_value(json).unwrap();
    assert_eq!(
        result.content,
        vec![LanguageModelToolResultContent::Text(Arc::from("hello"))]
    );

    // Legacy wrapped single-value shape also works.
    let json = serde_json::json!({
        "tool_use_id": "abc",
        "tool_name": "echo",
        "is_error": false,
        "content": {"type": "text", "text": "hello"},
        "output": null,
    });
    let result: LanguageModelToolResult = serde_json::from_value(json).unwrap();
    assert_eq!(
        result.content,
        vec![LanguageModelToolResultContent::Text(Arc::from("hello"))]
    );

    // New array shape with text + image deserializes into a Vec.
    let json = serde_json::json!({
        "tool_use_id": "abc",
        "tool_name": "echo",
        "is_error": false,
        "content": [
            {"type": "text", "text": "foo"},
            {"source": "data", "size": {"width": 1, "height": 2}}
        ],
        "output": null,
    });
    let result: LanguageModelToolResult = serde_json::from_value(json).unwrap();
    assert_eq!(result.content.len(), 2);
    assert_eq!(
        result.content[0],
        LanguageModelToolResultContent::Text(Arc::from("foo"))
    );
    match &result.content[1] {
        LanguageModelToolResultContent::Image(image) => {
            assert_eq!(image.source.as_ref(), "data");
        }
        _ => panic!("Expected Image variant"),
    }

    // Round-tripping preserves multi-part content.
    let roundtripped: LanguageModelToolResult =
        serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap();
    assert_eq!(roundtripped, result);
}

#[test]
fn test_string_contents_includes_all_tool_result_text_parts() {
    let tool_result = LanguageModelToolResult {
        tool_use_id: LanguageModelToolUseId::from("id".to_string()),
        tool_name: Arc::from("tool"),
        is_error: false,
        content: vec![
            LanguageModelToolResultContent::Text(Arc::from("first ")),
            LanguageModelToolResultContent::Image(LanguageModelImage::empty()),
            LanguageModelToolResultContent::Text(Arc::from("second")),
        ],
        output: None,
    };
    let message = LanguageModelRequestMessage {
        role: Role::User,
        content: vec![
            MessageContent::Text("prefix ".to_string()),
            MessageContent::ToolResult(tool_result),
            MessageContent::Text(" suffix".to_string()),
        ],
        cache: false,
        reasoning_details: None,
    };
    assert_eq!(message.string_contents(), "prefix first second suffix");
}
