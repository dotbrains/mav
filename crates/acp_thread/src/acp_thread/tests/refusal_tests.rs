use super::*;

#[gpui::test]
async fn test_tool_result_refusal(cx: &mut TestAppContext) {
    use std::sync::atomic::AtomicUsize;
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;

    // Create a connection that simulates refusal after tool result
    let prompt_count = Arc::new(AtomicUsize::new(0));
    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        let prompt_count = prompt_count.clone();
        move |_request, thread, mut cx| {
            let count = prompt_count.fetch_add(1, SeqCst);
            async move {
                if count == 0 {
                    // First prompt: Generate a tool call with result
                    thread.update(&mut cx, |thread, cx| {
                        thread
                            .handle_session_update(
                                acp::SessionUpdate::ToolCall(
                                    acp::ToolCall::new("tool1", "Test Tool")
                                        .kind(acp::ToolKind::Fetch)
                                        .status(acp::ToolCallStatus::Completed)
                                        .raw_input(serde_json::json!({"query": "test"}))
                                        .raw_output(
                                            serde_json::json!({"result": "inappropriate content"}),
                                        ),
                                ),
                                cx,
                            )
                            .unwrap();
                    })?;

                    // Now return refusal because of the tool result
                    Ok(acp::PromptResponse::new(acp::StopReason::Refusal))
                } else {
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
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

    // Track if we see a Refusal event
    let saw_refusal_event = Arc::new(std::sync::Mutex::new(false));
    let saw_refusal_event_captured = saw_refusal_event.clone();
    thread.update(cx, |_thread, cx| {
        cx.subscribe(
            &thread,
            move |_thread, _event_thread, event: &AcpThreadEvent, _cx| {
                if matches!(event, AcpThreadEvent::Refusal) {
                    *saw_refusal_event_captured.lock().unwrap() = true;
                }
            },
        )
        .detach();
    });

    // Send a user message - this will trigger tool call and then refusal
    let send_task = thread.update(cx, |thread, cx| thread.send(vec!["Hello".into()], cx));
    cx.background_executor.spawn(send_task).detach();
    cx.run_until_parked();

    // Verify that:
    // 1. A Refusal event WAS emitted (because it's a tool result refusal, not user prompt)
    // 2. The user message was NOT truncated
    assert!(
        *saw_refusal_event.lock().unwrap(),
        "Refusal event should be emitted for tool result refusals"
    );

    thread.read_with(cx, |thread, _| {
        let entries = thread.entries();
        assert!(entries.len() >= 2, "Should have user message and tool call");

        // Verify user message is still there
        assert!(
            matches!(entries[0], AgentThreadEntry::UserMessage(_)),
            "User message should not be truncated"
        );

        // Verify tool call is there with result
        if let AgentThreadEntry::ToolCall(tool_call) = &entries[1] {
            assert!(
                tool_call.raw_output.is_some(),
                "Tool call should have output"
            );
        } else {
            panic!("Expected tool call at index 1");
        }
    });
}

#[gpui::test]
async fn test_user_prompt_refusal_emits_event(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, None, cx).await;

    let refuse_next = Arc::new(AtomicBool::new(false));
    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        let refuse_next = refuse_next.clone();
        move |_request, _thread, _cx| {
            if refuse_next.load(SeqCst) {
                async move { Ok(acp::PromptResponse::new(acp::StopReason::Refusal)) }.boxed_local()
            } else {
                async move { Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)) }.boxed_local()
            }
        }
    }));

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    // Track if we see a Refusal event
    let saw_refusal_event = Arc::new(std::sync::Mutex::new(false));
    let saw_refusal_event_captured = saw_refusal_event.clone();
    thread.update(cx, |_thread, cx| {
        cx.subscribe(
            &thread,
            move |_thread, _event_thread, event: &AcpThreadEvent, _cx| {
                if matches!(event, AcpThreadEvent::Refusal) {
                    *saw_refusal_event_captured.lock().unwrap() = true;
                }
            },
        )
        .detach();
    });

    // Send a message that will be refused
    refuse_next.store(true, SeqCst);
    cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx)))
        .await
        .unwrap();

    // Verify that a Refusal event WAS emitted for user prompt refusal
    assert!(
        *saw_refusal_event.lock().unwrap(),
        "Refusal event should be emitted for user prompt refusals"
    );

    // Verify the message was truncated (user prompt refusal)
    thread.read_with(cx, |thread, cx| {
        assert_eq!(thread.to_markdown(cx), "");
    });
}

#[gpui::test]
async fn test_refusal(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(path!("/"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/").as_ref()], cx).await;

    let refuse_next = Arc::new(AtomicBool::new(false));
    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        let refuse_next = refuse_next.clone();
        move |request, thread, mut cx| {
            let refuse_next = refuse_next.clone();
            async move {
                if refuse_next.load(SeqCst) {
                    return Ok(acp::PromptResponse::new(acp::StopReason::Refusal));
                }

                let acp::ContentBlock::Text(content) = &request.prompt[0] else {
                    panic!("expected text content block");
                };
                thread.update(&mut cx, |thread, cx| {
                    thread
                        .handle_session_update(
                            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                                content.text.to_uppercase().into(),
                            )),
                            cx,
                        )
                        .unwrap();
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

    cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx)))
        .await
        .unwrap();
    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User

                hello

                ## Assistant

                HELLO

            "}
        );
    });

    // Simulate refusing the second message. The message should be truncated
    // when a user prompt is refused.
    refuse_next.store(true, SeqCst);
    cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["world".into()], cx)))
        .await
        .unwrap();
    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc! {"
                ## User

                hello

                ## Assistant

                HELLO

            "}
        );
    });
}
