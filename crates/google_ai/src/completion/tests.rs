
use super::*;
use crate::{
    Content, FunctionCall, FunctionCallPart, GenerateContentCandidate, GenerateContentResponse,
    Part, Role as GoogleRole,
};
use language_model_core::LanguageModelRequestMessage;
use serde_json::json;

fn text_request() -> LanguageModelRequest {
    LanguageModelRequest {
        messages: vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text("Hello".to_string())],
            cache: false,
            reasoning_details: None,
        }],
        ..Default::default()
    }
}

#[test]
fn into_google_requests_thought_summaries_and_thinking_level() {
    let mut request = text_request();
    request.thinking_allowed = true;
    request.thinking_effort = Some("low".to_string());

    let request = into_google(
        request,
        "gemini-3.5-flash".to_string(),
        GoogleModelMode::Thinking {
            budget_tokens: None,
        },
    );

    let thinking_config = request.generation_config.unwrap().thinking_config.unwrap();
    assert_eq!(thinking_config.include_thoughts, Some(true));
    assert_eq!(thinking_config.thinking_level, Some(ThinkingLevel::Low));

    let serialized = serde_json::to_value(thinking_config).unwrap();
    assert_eq!(serialized["thinkingLevel"], "LOW");
    assert_eq!(serialized["includeThoughts"], true);
}

#[test]
fn into_google_turns_off_budget_thinking_when_supported() {
    let mut request = text_request();
    request.thinking_allowed = false;

    let request = into_google(
        request,
        "gemini-2.5-flash".to_string(),
        GoogleModelMode::Thinking {
            budget_tokens: None,
        },
    );

    let thinking_config = request.generation_config.unwrap().thinking_config.unwrap();
    assert_eq!(thinking_config.thinking_budget, Some(0));
    assert_eq!(thinking_config.include_thoughts, None);
}

#[test]
fn into_google_uses_minimal_level_when_gemini_3_flash_thinking_is_off() {
    let mut request = text_request();
    request.thinking_allowed = false;

    let request = into_google(
        request,
        "gemini-3.5-flash".to_string(),
        GoogleModelMode::Thinking {
            budget_tokens: None,
        },
    );

    let thinking_config = request.generation_config.unwrap().thinking_config.unwrap();
    assert_eq!(thinking_config.thinking_level, Some(ThinkingLevel::Minimal));
    assert_eq!(thinking_config.include_thoughts, None);
}

#[test]
fn into_google_replays_signed_thinking_as_thought_text_part() {
    let request = LanguageModelRequest {
        messages: vec![LanguageModelRequestMessage {
            role: Role::Assistant,
            content: vec![MessageContent::Thinking {
                text: "summary".to_string(),
                signature: Some("signature".to_string()),
            }],
            cache: false,
            reasoning_details: None,
        }],
        ..Default::default()
    };

    let request = into_google(
        request,
        "gemini-3.5-flash".to_string(),
        GoogleModelMode::Thinking {
            budget_tokens: None,
        },
    );

    let Part::TextPart(text_part) = &request.contents[0].parts[0] else {
        panic!("expected text part");
    };
    assert_eq!(text_part.text, "summary");
    assert!(text_part.thought);
    assert_eq!(text_part.thought_signature.as_deref(), Some("signature"));
}

#[test]
fn thought_text_part_deserializes_and_maps_to_thinking_event() {
    let part: Part = serde_json::from_value(json!({
        "text": "checking the constraints",
        "thought": true,
        "thoughtSignature": "thought-signature"
    }))
    .unwrap();

    let mut mapper = GoogleEventMapper::new();
    let response = GenerateContentResponse {
        candidates: Some(vec![GenerateContentCandidate {
            index: Some(0),
            content: Content {
                parts: vec![part],
                role: GoogleRole::Model,
            },
            finish_reason: None,
            finish_message: None,
            safety_ratings: None,
            citation_metadata: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let events = mapper.map_event(response);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        Ok(LanguageModelCompletionEvent::Thinking { text, signature })
            if text == "checking the constraints"
                && signature.as_deref() == Some("thought-signature")
    ));
}

#[test]
fn signed_non_thought_text_part_preserves_signature() {
    let part: Part = serde_json::from_value(json!({
        "text": "visible text",
        "thoughtSignature": "visible-signature"
    }))
    .unwrap();

    let Part::TextPart(text_part) = part else {
        panic!("expected text part");
    };
    assert_eq!(text_part.text, "visible text");
    assert!(!text_part.thought);
    assert_eq!(
        text_part.thought_signature.as_deref(),
        Some("visible-signature")
    );
}

#[test]
fn signed_non_thought_text_part_maps_signature_carrier() {
    let part: Part = serde_json::from_value(json!({
        "text": "visible text",
        "thoughtSignature": "visible-signature"
    }))
    .unwrap();

    let mut mapper = GoogleEventMapper::new();
    let response = GenerateContentResponse {
        candidates: Some(vec![GenerateContentCandidate {
            index: Some(0),
            content: Content {
                parts: vec![part],
                role: GoogleRole::Model,
            },
            finish_reason: None,
            finish_message: None,
            safety_ratings: None,
            citation_metadata: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let events = mapper.map_event(response);
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        Ok(LanguageModelCompletionEvent::Thinking { text, signature })
            if text.is_empty() && signature.as_deref() == Some("visible-signature")
    ));
    assert!(matches!(
        &events[1],
        Ok(LanguageModelCompletionEvent::Text(text)) if text == "visible text"
    ));
}

#[test]
fn safety_finish_reason_is_refusal() {
    let mut mapper = GoogleEventMapper::new();
    let response = GenerateContentResponse {
        candidates: Some(vec![GenerateContentCandidate {
            index: Some(0),
            content: Content {
                parts: Vec::new(),
                role: GoogleRole::Model,
            },
            finish_reason: Some("SAFETY".to_string()),
            finish_message: None,
            safety_ratings: None,
            citation_metadata: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    mapper.map_event(response);
    assert_eq!(mapper.stop_reason, StopReason::Refusal);
}

#[test]
fn test_function_call_with_signature_creates_tool_use_with_signature() {
    let mut mapper = GoogleEventMapper::new();

    let response = GenerateContentResponse {
        candidates: Some(vec![GenerateContentCandidate {
            index: Some(0),
            content: Content {
                parts: vec![Part::FunctionCallPart(FunctionCallPart {
                    function_call: FunctionCall {
                        name: "test_function".to_string(),
                        args: json!({"arg": "value"}),
                        id: None,
                    },
                    thought_signature: Some("test_signature_123".to_string()),
                })],
                role: GoogleRole::Model,
            },
            finish_reason: None,
            finish_message: None,
            safety_ratings: None,
            citation_metadata: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let events = mapper.map_event(response);
    assert_eq!(events.len(), 2);

    if let Ok(LanguageModelCompletionEvent::ToolUse(tool_use)) = &events[0] {
        assert_eq!(tool_use.name.as_ref(), "test_function");
        assert_eq!(
            tool_use.thought_signature.as_deref(),
            Some("test_signature_123")
        );
    } else {
        panic!("Expected ToolUse event");
    }
}

#[test]
fn test_function_call_without_signature_has_none() {
    let mut mapper = GoogleEventMapper::new();

    let response = GenerateContentResponse {
        candidates: Some(vec![GenerateContentCandidate {
            index: Some(0),
            content: Content {
                parts: vec![Part::FunctionCallPart(FunctionCallPart {
                    function_call: FunctionCall {
                        name: "test_function".to_string(),
                        args: json!({"arg": "value"}),
                        id: None,
                    },
                    thought_signature: None,
                })],
                role: GoogleRole::Model,
            },
            finish_reason: None,
            finish_message: None,
            safety_ratings: None,
            citation_metadata: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let events = mapper.map_event(response);
    assert_eq!(events.len(), 2);

    if let Ok(LanguageModelCompletionEvent::ToolUse(tool_use)) = &events[0] {
        assert!(tool_use.thought_signature.is_none());
    } else {
        panic!("Expected ToolUse event");
    }
}

#[test]
fn test_empty_string_signature_normalized_to_none() {
    let mut mapper = GoogleEventMapper::new();

    let response = GenerateContentResponse {
        candidates: Some(vec![GenerateContentCandidate {
            index: Some(0),
            content: Content {
                parts: vec![Part::FunctionCallPart(FunctionCallPart {
                    function_call: FunctionCall {
                        name: "test_function".to_string(),
                        args: json!({"arg": "value"}),
                        id: None,
                    },
                    thought_signature: Some("".to_string()),
                })],
                role: GoogleRole::Model,
            },
            finish_reason: None,
            finish_message: None,
            safety_ratings: None,
            citation_metadata: None,
        }]),
        prompt_feedback: None,
        usage_metadata: None,
    };

    let events = mapper.map_event(response);
    if let Ok(LanguageModelCompletionEvent::ToolUse(tool_use)) = &events[0] {
        assert!(tool_use.thought_signature.is_none());
    } else {
        panic!("Expected ToolUse event");
    }
}
