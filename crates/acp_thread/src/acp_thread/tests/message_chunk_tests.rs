use super::*;

#[gpui::test]
async fn test_push_user_content_block(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    // Test creating a new user message
    thread.update(cx, |thread, cx| {
        thread.push_user_content_block(None, "Hello, ".into(), cx);
    });

    thread.update(cx, |thread, cx| {
        assert_eq!(thread.entries.len(), 1);
        if let AgentThreadEntry::UserMessage(user_msg) = &thread.entries[0] {
            assert_eq!(user_msg.protocol_id, None);
            assert_eq!(user_msg.client_id, None);
            assert_eq!(user_msg.content.to_markdown(cx), "Hello, ");
        } else {
            panic!("Expected UserMessage");
        }
    });

    // Test appending to existing user message
    let message_1_id = ClientUserMessageId::new();
    thread.update(cx, |thread, cx| {
        thread.push_user_content_block(Some(message_1_id.clone()), "world!".into(), cx);
    });

    thread.update(cx, |thread, cx| {
        assert_eq!(thread.entries.len(), 1);
        if let AgentThreadEntry::UserMessage(user_msg) = &thread.entries[0] {
            assert_eq!(user_msg.protocol_id, None);
            assert_eq!(user_msg.client_id, Some(message_1_id));
            assert_eq!(user_msg.content.to_markdown(cx), "Hello, world!");
        } else {
            panic!("Expected UserMessage");
        }
    });

    // Test creating new user message after assistant message
    thread.update(cx, |thread, cx| {
        thread.push_assistant_content_block("Assistant response".into(), false, cx);
    });

    let message_2_id = ClientUserMessageId::new();
    thread.update(cx, |thread, cx| {
        thread.push_user_content_block(Some(message_2_id.clone()), "New user message".into(), cx);
    });

    thread.update(cx, |thread, cx| {
        assert_eq!(thread.entries.len(), 3);
        if let AgentThreadEntry::UserMessage(user_msg) = &thread.entries[2] {
            assert_eq!(user_msg.protocol_id, None);
            assert_eq!(user_msg.client_id, Some(message_2_id));
            assert_eq!(user_msg.content.to_markdown(cx), "New user message");
        } else {
            panic!("Expected UserMessage at index 2");
        }
    });
}

#[gpui::test]
async fn test_user_message_chunks_use_protocol_message_id_boundaries(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::UserMessageChunk(
                    acp::ContentChunk::new("First ".into()).message_id("msg_user_1"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::UserMessageChunk(
                    acp::ContentChunk::new("message".into()).message_id("msg_user_1"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::UserMessageChunk(
                    acp::ContentChunk::new("Second message".into()).message_id("msg_user_2"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::UserMessageChunk(
                    acp::ContentChunk::new("Echo".into()).message_id("msg_user_3"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::UserMessageChunk(
                    acp::ContentChunk::new("Echo".into()).message_id("msg_user_3"),
                ),
                cx,
            )
            .unwrap();
    });

    thread.update(cx, |thread, cx| {
        assert_eq!(thread.entries.len(), 3);

        let AgentThreadEntry::UserMessage(first_message) = &thread.entries[0] else {
            panic!("expected first entry to be a user message")
        };
        assert_eq!(first_message.content.to_markdown(cx), "First message");
        assert_eq!(
            first_message
                .protocol_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("msg_user_1")
        );

        let AgentThreadEntry::UserMessage(second_message) = &thread.entries[1] else {
            panic!("expected second entry to be a user message")
        };
        assert_eq!(second_message.content.to_markdown(cx), "Second message");
        assert_eq!(
            second_message
                .protocol_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("msg_user_2")
        );

        let AgentThreadEntry::UserMessage(third_message) = &thread.entries[2] else {
            panic!("expected third entry to be a user message")
        };
        assert_eq!(third_message.content.to_markdown(cx), "EchoEcho");
        assert_eq!(
            third_message
                .protocol_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("msg_user_3")
        );
    });
}

#[gpui::test]
async fn test_protocol_user_chunk_does_not_merge_into_optimistic_prompt(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread.update(cx, |thread, cx| {
        thread.push_user_content_block_with_protocol_id(
            None,
            true,
            None,
            "Typed prompt".into(),
            false,
            cx,
        );
        thread
            .handle_session_update(
                acp::SessionUpdate::UserMessageChunk(
                    acp::ContentChunk::new("Agent user chunk".into())
                        .message_id("agent_user_chunk"),
                ),
                cx,
            )
            .unwrap();
    });

    thread.update(cx, |thread, cx| {
        assert_eq!(thread.entries.len(), 2);

        let AgentThreadEntry::UserMessage(optimistic_message) = &thread.entries[0] else {
            panic!("expected first entry to be optimistic user message")
        };
        assert!(optimistic_message.is_optimistic);
        assert_eq!(optimistic_message.content.to_markdown(cx), "Typed prompt");
        assert!(optimistic_message.protocol_id.is_none());
        assert!(optimistic_message.client_id.is_none());

        let AgentThreadEntry::UserMessage(agent_message) = &thread.entries[1] else {
            panic!("expected second entry to be protocol user chunk")
        };
        assert!(!agent_message.is_optimistic);
        assert_eq!(agent_message.content.to_markdown(cx), "Agent user chunk");
        assert_eq!(
            agent_message
                .protocol_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("agent_user_chunk")
        );
    });
}

#[gpui::test]
async fn test_assistant_chunks_use_protocol_message_id_boundaries(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::AgentThoughtChunk(
                    acp::ContentChunk::new("Thinking ".into()).message_id("msg_thought_1"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::AgentThoughtChunk(
                    acp::ContentChunk::new("hard".into()).message_id("msg_thought_1"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::AgentThoughtChunk(
                    acp::ContentChunk::new("A separate thought".into()).message_id("msg_thought_2"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::AgentMessageChunk(
                    acp::ContentChunk::new("Answer ".into()).message_id("msg_agent_1"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::AgentMessageChunk(
                    acp::ContentChunk::new("done".into()).message_id("msg_agent_1"),
                ),
                cx,
            )
            .unwrap();
        thread
            .handle_session_update(
                acp::SessionUpdate::AgentMessageChunk(
                    acp::ContentChunk::new("Follow-up".into()).message_id("msg_agent_2"),
                ),
                cx,
            )
            .unwrap();
    });

    thread.update(cx, |thread, cx| {
        assert_eq!(thread.entries.len(), 1);
        let AgentThreadEntry::AssistantMessage(message) = &thread.entries[0] else {
            panic!("expected assistant entry")
        };
        assert_eq!(message.chunks.len(), 4);

        let AssistantMessageChunk::Thought { id, block } = &message.chunks[0] else {
            panic!("expected first chunk to be a thought")
        };
        assert_eq!(block.to_markdown(cx), "Thinking hard");
        assert_eq!(
            id.as_ref().map(ToString::to_string).as_deref(),
            Some("msg_thought_1")
        );

        let AssistantMessageChunk::Thought { id, block } = &message.chunks[1] else {
            panic!("expected second chunk to be a thought")
        };
        assert_eq!(block.to_markdown(cx), "A separate thought");
        assert_eq!(
            id.as_ref().map(ToString::to_string).as_deref(),
            Some("msg_thought_2")
        );

        let AssistantMessageChunk::Message { id, block } = &message.chunks[2] else {
            panic!("expected third chunk to be a message")
        };
        assert_eq!(block.to_markdown(cx), "Answer done");
        assert_eq!(
            id.as_ref().map(ToString::to_string).as_deref(),
            Some("msg_agent_1")
        );

        let AssistantMessageChunk::Message { id, block } = &message.chunks[3] else {
            panic!("expected fourth chunk to be a message")
        };
        assert_eq!(block.to_markdown(cx), "Follow-up");
        assert_eq!(
            id.as_ref().map(ToString::to_string).as_deref(),
            Some("msg_agent_2")
        );
    });
}

#[gpui::test]
async fn test_thinking_concatenation(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(
        FakeAgentConnection::new().on_user_message(|_, thread, mut cx| {
            async move {
                thread.update(&mut cx, |thread, cx| {
                    thread
                        .handle_session_update(
                            acp::SessionUpdate::AgentThoughtChunk(acp::ContentChunk::new(
                                "Thinking ".into(),
                            )),
                            cx,
                        )
                        .unwrap();
                    thread
                        .handle_session_update(
                            acp::SessionUpdate::AgentThoughtChunk(acp::ContentChunk::new(
                                "hard!".into(),
                            )),
                            cx,
                        )
                        .unwrap();
                })?;
                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
            .boxed_local()
        }),
    );

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread
        .update(cx, |thread, cx| thread.send_raw("Hello from Mav!", cx))
        .await
        .unwrap();

    let output = thread.read_with(cx, |thread, cx| thread.to_markdown(cx));
    assert_eq!(
        output,
        indoc! {r#"
        ## User

        Hello from Mav!

        ## Assistant

        <thinking>
        Thinking hard!
        </thinking>

        "#}
    );
}
