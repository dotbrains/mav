use open_ai::{MessageContent, RequestMessage};

pub(super) fn message_role_to_string(msg: &RequestMessage) -> String {
    match msg {
        RequestMessage::User { .. } => "user".to_string(),
        RequestMessage::Assistant { .. } => "assistant".to_string(),
        RequestMessage::System { .. } => "system".to_string(),
        RequestMessage::Tool { .. } => "tool".to_string(),
    }
}

pub(super) fn message_content_to_string(msg: &RequestMessage) -> String {
    match msg {
        RequestMessage::User { content } => content_to_string(content),
        RequestMessage::Assistant { content, .. } => {
            content.as_ref().map(content_to_string).unwrap_or_default()
        }
        RequestMessage::System { content } => content_to_string(content),
        RequestMessage::Tool { content, .. } => content_to_string(content),
    }
}

fn content_to_string(content: &MessageContent) -> String {
    match content {
        MessageContent::Plain(text) => text.clone(),
        MessageContent::Multipart(parts) => parts
            .iter()
            .filter_map(|part| match part {
                open_ai::MessagePart::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<String>>()
            .join("\n"),
    }
}
