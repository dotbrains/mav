use super::*;

#[test]
fn responses_stream_maps_reasoning_summary_deltas() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(response_reasoning_item(
                "rs_123",
                vec![],
                None,
                None,
            )),
        },
        ResponsesStreamEvent::ReasoningSummaryPartAdded {
            item_id: "rs_123".into(),
            output_index: 0,
            summary_index: 0,
        },
        ResponsesStreamEvent::ReasoningSummaryTextDelta {
            item_id: "rs_123".into(),
            output_index: 0,
            delta: "Thinking about".into(),
        },
        ResponsesStreamEvent::ReasoningSummaryTextDelta {
            item_id: "rs_123".into(),
            output_index: 0,
            delta: " the answer".into(),
        },
        ResponsesStreamEvent::ReasoningSummaryTextDone {
            item_id: "rs_123".into(),
            output_index: 0,
            text: "Thinking about the answer".into(),
        },
        ResponsesStreamEvent::ReasoningSummaryPartDone {
            item_id: "rs_123".into(),
            output_index: 0,
            summary_index: 0,
        },
        ResponsesStreamEvent::ReasoningSummaryPartAdded {
            item_id: "rs_123".into(),
            output_index: 0,
            summary_index: 1,
        },
        ResponsesStreamEvent::ReasoningSummaryTextDelta {
            item_id: "rs_123".into(),
            output_index: 0,
            delta: "Second part".into(),
        },
        ResponsesStreamEvent::ReasoningSummaryTextDone {
            item_id: "rs_123".into(),
            output_index: 0,
            text: "Second part".into(),
        },
        ResponsesStreamEvent::ReasoningSummaryPartDone {
            item_id: "rs_123".into(),
            output_index: 0,
            summary_index: 1,
        },
        ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(response_reasoning_item(
                "rs_123",
                vec![
                    ReasoningSummaryPart::SummaryText {
                        text: "Thinking about the answer".into(),
                    },
                    ReasoningSummaryPart::SummaryText {
                        text: "Second part".into(),
                    },
                ],
                None,
                None,
            )),
        },
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 1,
            sequence_number: None,
            item: response_item_message("msg_456"),
        },
        ResponsesStreamEvent::OutputTextDelta {
            item_id: "msg_456".into(),
            output_index: 1,
            content_index: Some(0),
            delta: "The answer is 42".into(),
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);

    let thinking_events: Vec<_> = mapped
        .iter()
        .filter(|e| matches!(e, LanguageModelCompletionEvent::Thinking { .. }))
        .collect();
    assert_eq!(
        thinking_events.len(),
        4,
        "expected 4 thinking events, got {:?}",
        thinking_events
    );
    assert!(
        matches!(&thinking_events[0], LanguageModelCompletionEvent::Thinking { text, .. } if text == "Thinking about")
    );
    assert!(
        matches!(&thinking_events[1], LanguageModelCompletionEvent::Thinking { text, .. } if text == " the answer")
    );
    assert!(
        matches!(&thinking_events[2], LanguageModelCompletionEvent::Thinking { text, .. } if text == "\n\n"),
        "expected separator between summary parts"
    );
    assert!(
        matches!(&thinking_events[3], LanguageModelCompletionEvent::Thinking { text, .. } if text == "Second part")
    );

    assert!(
        mapped
            .iter()
            .any(|e| matches!(e, LanguageModelCompletionEvent::Text(t) if t == "The answer is 42"))
    );
}

#[test]
fn responses_stream_maps_reasoning_from_done_only() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(response_reasoning_item(
                "rs_789",
                vec![],
                None,
                None,
            )),
        },
        ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(response_reasoning_item(
                "rs_789",
                vec![ReasoningSummaryPart::SummaryText {
                    text: "Summary without deltas".into(),
                }],
                None,
                None,
            )),
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    assert!(
        !mapped
            .iter()
            .any(|e| matches!(e, LanguageModelCompletionEvent::Thinking { .. })),
        "OutputItemDone reasoning should not produce Thinking events"
    );
}

#[test]
fn responses_stream_preserves_encrypted_reasoning_details() {
    let mut reasoning_item = response_reasoning_item(
        "rs_123",
        vec![ReasoningSummaryPart::SummaryText {
            text: "Checked what information is needed.".into(),
        }],
        Some("ENC"),
        Some("completed".into()),
    );
    reasoning_item.content = vec![json!({
        "type": "reasoning_text",
        "text": "Internal reasoning text."
    })];

    let events = vec![
        ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(reasoning_item),
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    let details = mapped
        .iter()
        .find_map(|event| match event {
            LanguageModelCompletionEvent::ReasoningDetails(details) => Some(details),
            _ => None,
        })
        .expect("reasoning details");

    assert_eq!(
        details,
        &json!({
            "reasoning_items": [
                {
                    "id": "rs_123",
                    "summary": [
                        {
                            "type": "summary_text",
                            "text": "Checked what information is needed."
                        }
                    ],
                    "content": [
                        {
                            "type": "reasoning_text",
                            "text": "Internal reasoning text."
                        }
                    ],
                    "encrypted_content": "ENC",
                    "status": "completed",
                }
            ]
        })
    );
}

#[test]
fn responses_stream_replaces_reasoning_details_with_same_id() {
    let events = vec![
        ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(response_reasoning_item(
                "rs_123",
                Vec::new(),
                Some("ENC_OLD"),
                Some("in_progress".into()),
            )),
        },
        ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(response_reasoning_item(
                "rs_123",
                vec![ReasoningSummaryPart::SummaryText {
                    text: "Finished reasoning.".into(),
                }],
                Some("ENC_NEW"),
                Some("completed".into()),
            )),
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    let details = mapped
        .iter()
        .filter_map(|event| match event {
            LanguageModelCompletionEvent::ReasoningDetails(details) => Some(details),
            _ => None,
        })
        .next_back()
        .expect("reasoning details");

    assert_eq!(
        details,
        &json!({
            "reasoning_items": [
                {
                    "id": "rs_123",
                    "summary": [
                        {
                            "type": "summary_text",
                            "text": "Finished reasoning."
                        }
                    ],
                    "encrypted_content": "ENC_NEW",
                    "status": "completed"
                }
            ]
        })
    );
}

#[test]
fn responses_stream_reemits_reasoning_details_after_phase_less_message_start() {
    let events = vec![
        ResponsesStreamEvent::OutputItemDone {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(response_reasoning_item(
                "rs_123",
                Vec::new(),
                Some("ENC"),
                Some("completed".into()),
            )),
        },
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 1,
            sequence_number: None,
            item: ResponseOutputItem::Message(ResponseOutputMessage {
                id: Some("msg_123".into()),
                role: Some("assistant".into()),
                status: Some("in_progress".into()),
                content: vec![],
                phase: None,
            }),
        },
        ResponsesStreamEvent::OutputTextDelta {
            item_id: "msg_123".into(),
            output_index: 1,
            content_index: Some(0),
            delta: "Hello".into(),
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    let start_message_index = mapped
        .iter()
        .position(|event| matches!(event, LanguageModelCompletionEvent::StartMessage { .. }))
        .expect("start message");
    let details = mapped
        .iter()
        .skip(start_message_index + 1)
        .find_map(|event| match event {
            LanguageModelCompletionEvent::ReasoningDetails(details) => Some(details),
            _ => None,
        })
        .expect("reasoning details after start message");

    assert_eq!(
        details,
        &json!({
            "reasoning_items": [
                {
                    "id": "rs_123",
                    "summary": [],
                    "encrypted_content": "ENC",
                    "status": "completed"
                }
            ]
        })
    );
}

#[test]
fn responses_stream_preserves_assistant_phase_with_reasoning_details() {
    let events = vec![
        ResponsesStreamEvent::OutputItemAdded {
            output_index: 0,
            sequence_number: None,
            item: ResponseOutputItem::Message(ResponseOutputMessage {
                id: Some("msg_123".into()),
                role: Some("assistant".into()),
                status: Some("in_progress".into()),
                content: vec![],
                phase: Some("commentary".into()),
            }),
        },
        ResponsesStreamEvent::OutputTextDelta {
            item_id: "msg_123".into(),
            output_index: 0,
            content_index: Some(0),
            delta: "I will inspect the workspace.".into(),
        },
        ResponsesStreamEvent::OutputItemDone {
            output_index: 1,
            sequence_number: None,
            item: ResponseOutputItem::Reasoning(response_reasoning_item(
                "rs_123",
                Vec::new(),
                Some("ENC"),
                Some("completed".into()),
            )),
        },
        ResponsesStreamEvent::Completed {
            response: ResponseSummary::default(),
        },
    ];

    let mapped = map_response_events(events);
    let details = mapped
        .iter()
        .filter_map(|event| match event {
            LanguageModelCompletionEvent::ReasoningDetails(details) => Some(details),
            _ => None,
        })
        .next_back()
        .expect("reasoning details");

    assert_eq!(
        details,
        &json!({
            "phase": "commentary",
            "reasoning_items": [
                {
                    "id": "rs_123",
                    "summary": [],
                    "encrypted_content": "ENC",
                    "status": "completed"
                }
            ]
        })
    );
}
