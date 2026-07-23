use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use language_model::{LanguageModelImage, LanguageModelRequestMessage, MessageContent};

    fn tool_call_chunk(
        id: Option<&str>,
        name: Option<&str>,
        arguments: Option<&str>,
        finish_reason: Option<&str>,
    ) -> mistral::StreamResponse {
        mistral::StreamResponse {
            id: "resp".into(),
            object: "chat.completion.chunk".into(),
            created: 0,
            model: "test".into(),
            choices: vec![mistral::StreamChoice {
                index: 0,
                delta: mistral::StreamDelta {
                    role: None,
                    content: None,
                    tool_calls: if finish_reason.is_some() {
                        None
                    } else {
                        Some(vec![mistral::ToolCallChunk {
                            index: 0,
                            id: id.map(Into::into),
                            function: Some(mistral::FunctionChunk {
                                name: name.map(Into::into),
                                arguments: arguments.map(Into::into),
                            }),
                        }])
                    },
                },
                finish_reason: finish_reason.map(Into::into),
            }],
            usage: None,
        }
    }

    #[test]
    fn test_streaming_tool_call_ignores_null_id() {
        // Mistral's streaming API sometimes sends `"id": "null"` in continuation chunks.
        let mut mapper = MistralEventMapper::new();

        mapper.map_event(tool_call_chunk(
            Some("real_id_123"),
            Some("read_file"),
            Some("{\"path\":"),
            None,
        ));
        mapper.map_event(tool_call_chunk(
            Some("null"),
            None,
            Some("\"a.txt\"}"),
            None,
        ));
        let events = mapper.map_event(tool_call_chunk(None, None, None, Some("tool_calls")));

        let Ok(LanguageModelCompletionEvent::ToolUse(tool_use)) = &events[0] else {
            panic!("Expected first event to be ToolUse, got: {:?}", events[0]);
        };

        assert_eq!(tool_use.id.to_string(), "real_id_123");
        assert_eq!(tool_use.name.as_ref(), "read_file");
        assert_eq!(tool_use.input, serde_json::json!({"path": "a.txt"}));
    }

    #[test]
    fn test_into_mistral_basic_conversion() {
        let request = LanguageModelRequest {
            messages: vec![
                LanguageModelRequestMessage {
                    role: Role::System,
                    content: vec![MessageContent::Text("System prompt".into())],
                    cache: false,
                    reasoning_details: None,
                },
                LanguageModelRequestMessage {
                    role: Role::User,
                    content: vec![MessageContent::Text("Hello".into())],
                    cache: false,
                    reasoning_details: None,
                },
                // should skip empty assistant messages
                LanguageModelRequestMessage {
                    role: Role::Assistant,
                    content: vec![MessageContent::Text("".into())],
                    cache: false,
                    reasoning_details: None,
                },
            ],
            temperature: Some(0.5),
            tools: vec![],
            tool_choice: None,
            thread_id: Some("abcdef".into()),
            prompt_id: None,
            intent: None,
            stop: vec![],
            thinking_allowed: true,
            thinking_effort: None,
            speed: Default::default(),
            compact_at_tokens: None,
        };

        let (mistral_request, affinity) =
            into_mistral(request, mistral::Model::MistralSmallLatest, None);

        assert_eq!(mistral_request.model, "mistral-small-latest");
        assert_eq!(mistral_request.temperature, Some(0.5));
        assert_eq!(mistral_request.messages.len(), 2);
        assert!(mistral_request.stream);
        assert_eq!(affinity, Some("abcdef".into()));
    }

    #[test]
    fn test_into_mistral_with_image() {
        let request = LanguageModelRequest {
            messages: vec![LanguageModelRequestMessage {
                role: Role::User,
                content: vec![
                    MessageContent::Text("What's in this image?".into()),
                    MessageContent::Image(LanguageModelImage {
                        source: "base64data".into(),
                    }),
                ],
                cache: false,
                reasoning_details: None,
            }],
            tools: vec![],
            tool_choice: None,
            temperature: None,
            thread_id: None,
            prompt_id: None,
            intent: None,
            stop: vec![],
            thinking_allowed: true,
            thinking_effort: None,
            speed: None,
            compact_at_tokens: None,
        };

        let (mistral_request, _) = into_mistral(request, mistral::Model::MistralSmallLatest, None);

        assert_eq!(mistral_request.messages.len(), 1);
        assert!(matches!(
            &mistral_request.messages[0],
            mistral::RequestMessage::User {
                content: mistral::MessageContent::Multipart { .. }
            }
        ));

        if let mistral::RequestMessage::User {
            content: mistral::MessageContent::Multipart { content },
        } = &mistral_request.messages[0]
        {
            assert_eq!(content.len(), 2);
            assert!(matches!(
                &content[0],
                mistral::MessagePart::Text { text } if text == "What's in this image?"
            ));
            assert!(matches!(
                &content[1],
                mistral::MessagePart::ImageUrl { image_url } if image_url.starts_with("data:image/png;base64,")
            ));
        }
    }
}
