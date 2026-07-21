use super::*;

#[gpui::test]
async fn test_send_command_does_not_echo_user_message(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let received_prompt: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        let received_prompt = received_prompt.clone();
        move |request, thread, mut cx| {
            let received_prompt = received_prompt.clone();
            async move {
                if let Some(acp::ContentBlock::Text(text)) = request.prompt.first() {
                    *received_prompt.borrow_mut() = Some(text.text.clone());
                }
                // Simulate a native command producing its own thread entry
                // (here a compaction) rather than echoing a user message.
                thread.update(&mut cx, |thread, cx| {
                    thread.push_context_compaction(
                        ContextCompaction {
                            id: ContextCompactionId("c1".into()),
                            status: ContextCompactionStatus::Completed,
                            summary: None,
                        },
                        cx,
                    );
                })?;
                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
            .boxed_local()
        }
    }));

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread.send_command(vec!["/compact".into()], cx)
        })
    })
    .await
    .unwrap();

    // The command turn ran: the connection received the typed command.
    assert_eq!(received_prompt.borrow().as_deref(), Some("/compact"));

    thread.update(cx, |thread, _cx| {
        assert!(
            !thread
                .entries
                .iter()
                .any(|entry| matches!(entry, AgentThreadEntry::UserMessage(_))),
            "send_command must not echo a user message"
        );
        // The command's own entry (here a compaction) is still shown.
        assert!(
            thread
                .entries
                .iter()
                .any(|entry| matches!(entry, AgentThreadEntry::ContextCompaction(_))),
            "the command's own thread entry should still be present"
        );
    });
}

#[gpui::test]
async fn test_ignore_echoed_user_message_chunks_during_active_turn(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(
        FakeAgentConnection::new()
            .without_truncate_support()
            .on_user_message(|request, thread, mut cx| {
                async move {
                    let prompt = request.prompt.first().cloned().unwrap_or_else(|| "".into());

                    thread.update(&mut cx, |thread, cx| {
                        thread
                            .handle_session_update(
                                acp::SessionUpdate::UserMessageChunk(acp::ContentChunk::new(
                                    prompt,
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
    assert_eq!(output.matches("Hello from Mav!").count(), 1);
    thread.read_with(cx, |thread, _cx| {
        let Some(AgentThreadEntry::UserMessage(message)) = thread.entries.first() else {
            panic!("expected optimistic user message");
        };
        assert_eq!(message.protocol_id, None);
        assert_eq!(message.client_id, None);
        assert!(message.is_optimistic);
    });
}
