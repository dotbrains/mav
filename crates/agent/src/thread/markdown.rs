use super::*;

pub(crate) fn messages_to_markdown(messages: &[Arc<Message>]) -> String {
    let mut markdown = String::new();
    for (ix, message) in messages.iter().enumerate() {
        if ix > 0 {
            markdown.push('\n');
        }
        match &**message {
            Message::User(_) => markdown.push_str("## User\n\n"),
            Message::Agent(_) => markdown.push_str("## Assistant\n\n"),
            Message::Resume | Message::Compaction(_) => {}
        }
        markdown.push_str(&message.to_markdown());
    }
    markdown
}
