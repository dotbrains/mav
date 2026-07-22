use super::*;

#[test]
fn into_open_ai_response_maps_compact_at_tokens_to_context_management() {
    let request = LanguageModelRequest {
        messages: vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text("Hello".into())],
            cache: false,
            reasoning_details: None,
        }],
        compact_at_tokens: Some(100_000),
        ..Default::default()
    };

    let response = into_open_ai_response(request, "gpt-5.1", true, true, None, None, false);

    assert_eq!(
        serde_json::to_value(&response).unwrap()["context_management"],
        json!([{ "type": "compaction", "compact_threshold": 100_000 }])
    );
}

#[test]
fn into_open_ai_response_omits_context_management_without_compact_at_tokens() {
    let request = LanguageModelRequest {
        messages: vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec![MessageContent::Text("Hello".into())],
            cache: false,
            reasoning_details: None,
        }],
        ..Default::default()
    };

    let response = into_open_ai_response(request, "gpt-5.1", true, true, None, None, false);

    assert!(
        serde_json::to_value(&response)
            .unwrap()
            .get("context_management")
            .is_none()
    );
}

#[test]
fn into_open_ai_response_replays_encrypted_compaction_block() {
    let request = LanguageModelRequest {
        messages: vec![LanguageModelRequestMessage {
            role: Role::Assistant,
            content: vec![
                MessageContent::Compaction(CompactionContent::Encrypted {
                    id: Some("cmp_1".into()),
                    encrypted_content: "encrypted-blob".into(),
                }),
                MessageContent::Text("Done.".into()),
            ],
            cache: false,
            reasoning_details: None,
        }],
        ..Default::default()
    };

    let response = into_open_ai_response(request, "gpt-5.1", true, true, None, None, false);

    assert_eq!(
        serde_json::to_value(&response).unwrap()["input"],
        json!([
            {
                "type": "compaction",
                "id": "cmp_1",
                "encrypted_content": "encrypted-blob"
            },
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    { "type": "output_text", "text": "Done.", "annotations": [] }
                ]
            }
        ])
    );
}

#[test]
fn responses_stream_maps_compaction_output_item() {
    let item: ResponseOutputItem = serde_json::from_value(json!({
        "type": "compaction",
        "id": "cmp_1",
        "encrypted_content": "encrypted-blob"
    }))
    .unwrap();
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: item.clone(),
        },
        ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item,
        },
    ];

    let mapped = map_response_events(events);

    assert_eq!(
        mapped,
        vec![
            LanguageModelCompletionEvent::Compaction(CompactionContent::Pending),
            LanguageModelCompletionEvent::Compaction(CompactionContent::Encrypted {
                id: Some("cmp_1".into()),
                encrypted_content: "encrypted-blob".into(),
            }),
        ]
    );
}
