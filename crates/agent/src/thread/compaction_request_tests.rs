use super::*;

#[test]
fn test_truncate_text_utf8_boundary() {
    let message = LanguageModelRequestMessage {
        role: Role::User,
        content: vec![MessageContent::Text("hello 👋 world".to_string())],
        cache: false,
        reasoning_details: None,
    };

    let truncated = super::compaction::truncate_user_message_to_byte_budget(message, 8).unwrap();
    assert_eq!(
        truncated.content,
        vec![MessageContent::Text("hello ".to_string())]
    );
}

#[test]
fn test_truncate_keeps_fitting_images() {
    let image = LanguageModelImage {
        source: "image".into(),
    };
    let message = LanguageModelRequestMessage {
        role: Role::User,
        content: vec![
            MessageContent::Text("abc".to_string()),
            MessageContent::Image(image.clone()),
        ],
        cache: false,
        reasoning_details: None,
    };

    let truncated = super::compaction::truncate_user_message_to_byte_budget(message, 8).unwrap();
    assert_eq!(
        truncated.content,
        vec![
            MessageContent::Text("abc".to_string()),
            MessageContent::Image(image),
        ]
    );
}
