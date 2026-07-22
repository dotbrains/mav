use super::*;

#[test]
fn into_open_ai_response_sends_none_reasoning_when_thinking_is_disabled() -> Result<()> {
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text("Hello".into())],
            cache: false,
            reasoning_details: None,
        }],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: false,
        thinking_effort: Some("high".into()),
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(
        request,
        "gpt-5.1",
        true,
        true,
        None,
        Some(ReasoningEffort::Medium),
        true,
    );

    let serialized = serde_json::to_value(&response)?;
    assert_eq!(serialized["reasoning"], json!({ "effort": "none" }));
    assert_eq!(serialized.get("include"), None);

    Ok(())
}

#[test]
fn into_open_ai_response_uses_default_effort_when_selected_effort_is_none() -> Result<()> {
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text("Hello".into())],
            cache: false,
            reasoning_details: None,
        }],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: true,
        thinking_effort: Some("none".into()),
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(
        request,
        "gpt-5.1",
        true,
        true,
        None,
        Some(ReasoningEffort::Medium),
        true,
    );

    let serialized = serde_json::to_value(&response)?;
    assert_eq!(
        serialized["reasoning"],
        json!({ "effort": "medium", "summary": "auto" })
    );

    Ok(())
}

#[test]
fn into_open_ai_response_replays_assistant_phase() {
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![LanguageModelRequestMessage {
            role: Role::Assistant,
            content: vec![MessageContent::Text("Done.".into())],
            cache: false,
            reasoning_details: Some(Arc::new(json!({
                "phase": "final_answer",
                "reasoning_items": [
                    {
                        "id": "rs_123",
                        "summary": [],
                        "encrypted_content": "ENC",
                        "status": "completed"
                    }
                ]
            }))),
        }],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: true,
        thinking_effort: None,
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(
        request,
        "gpt-5.3-codex",
        true,
        true,
        None,
        Some(ReasoningEffort::Medium),
        false,
    );

    let serialized = serde_json::to_value(&response).unwrap();
    assert_eq!(
        serialized["input"],
        json!([
            {
                "type": "reasoning",
                "id": "rs_123",
                "summary": [],
                "encrypted_content": "ENC",
                "status": "completed"
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    { "type": "output_text", "text": "Done.", "annotations": [] }
                ],
                "phase": "final_answer"
            }
        ])
    );
}

#[test]
fn into_open_ai_response_deduplicates_replayed_reasoning_items() {
    let first_reasoning_details = json!({
        "phase": "final_answer",
        "reasoning_items": [
            {
                "id": "rs_123",
                "summary": [],
                "encrypted_content": "ENC_OLD",
                "status": "in_progress"
            }
        ]
    });
    let second_reasoning_details = json!({
        "phase": "final_answer",
        "reasoning_items": [
            {
                "id": "rs_123",
                "summary": [
                    {
                        "type": "summary_text",
                        "text": "Later metadata has the complete summary."
                    }
                ],
                "encrypted_content": "ENC_NEW",
                "status": "completed"
            }
        ]
    });
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![MessageContent::Text("First.".into())],
                cache: false,
                reasoning_details: Some(Arc::new(first_reasoning_details)),
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![MessageContent::Text("Second.".into())],
                cache: false,
                reasoning_details: Some(Arc::new(second_reasoning_details)),
            },
        ],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: true,
        thinking_effort: None,
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(
        request,
        "gpt-5.3-codex",
        true,
        true,
        None,
        Some(ReasoningEffort::Medium),
        false,
    );

    let serialized = serde_json::to_value(&response).unwrap();
    assert_eq!(
        serialized["input"],
        json!([
            {
                "type": "reasoning",
                "id": "rs_123",
                "summary": [
                    {
                        "type": "summary_text",
                        "text": "Later metadata has the complete summary."
                    }
                ],
                "encrypted_content": "ENC_NEW",
                "status": "completed"
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    { "type": "output_text", "text": "First.", "annotations": [] }
                ],
                "phase": "final_answer"
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    { "type": "output_text", "text": "Second.", "annotations": [] }
                ],
                "phase": "final_answer"
            }
        ])
    );
}

#[test]
fn into_open_ai_response_replays_reasoning_details_but_not_thinking_text() {
    let request = LanguageModelRequest {
        thread_id: None,
        prompt_id: None,
        intent: None,
        messages: vec![LanguageModelRequestMessage {
            role: Role::Assistant,
            content: vec![
                MessageContent::Thinking {
                    text: "This is a reasoning summary, not assistant output.".into(),
                    signature: None,
                },
                MessageContent::Text("This is visible assistant output.".into()),
            ],
            cache: false,
            reasoning_details: Some(Arc::new(json!({
                "reasoning_items": [
                    {
                        "id": "rs_123",
                        "summary": [
                            {
                                "type": "summary_text",
                                "text": "This is the reasoning summary to preserve."
                            }
                        ],
                        "encrypted_content": "ENC",
                        "status": "completed"
                    }
                ]
            }))),
        }],
        tools: Vec::new(),
        tool_choice: None,
        stop: Vec::new(),
        temperature: None,
        thinking_allowed: false,
        thinking_effort: None,
        speed: None,
        compact_at_tokens: None,
    };

    let response = into_open_ai_response(request, "custom-model", false, false, None, None, false);
    let serialized = serde_json::to_value(&response).unwrap();

    assert_eq!(
        serialized["input"],
        json!([
            {
                "type": "reasoning",
                "id": "rs_123",
                "summary": [
                    {
                        "type": "summary_text",
                        "text": "This is the reasoning summary to preserve."
                    }
                ],
                "encrypted_content": "ENC",
                "status": "completed"
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    {
                        "type": "output_text",
                        "text": "This is visible assistant output.",
                        "annotations": []
                    }
                ]
            }
        ])
    );
    assert_eq!(
        serialized["include"],
        json!(["reasoning.encrypted_content"])
    );
}
