use super::*;

/// Tests that when a follow-up message is sent during generation,
/// the first turn completing does NOT clear `running_turn` because
/// it now belongs to the second turn.
#[gpui::test]
async fn test_follow_up_message_during_generation_does_not_clear_turn(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    // First handler waits for this signal before completing
    let (first_complete_tx, first_complete_rx) = futures::channel::oneshot::channel::<()>();
    let first_complete_rx = RefCell::new(Some(first_complete_rx));

    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        move |params, _thread, _cx| {
            let first_complete_rx = first_complete_rx.borrow_mut().take();
            let is_first = params
                .prompt
                .iter()
                .any(|c| matches!(c, acp::ContentBlock::Text(t) if t.text.contains("first")));

            async move {
                if is_first {
                    // First handler waits until signaled
                    if let Some(rx) = first_complete_rx {
                        rx.await.ok();
                    }
                }
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

    // Send first message (turn_id=1) - handler will block
    let first_request = thread.update(cx, |thread, cx| thread.send_raw("first", cx));
    assert_eq!(thread.read_with(cx, |t, _| t.turn_id), 1);

    // Send second message (turn_id=2) while first is still blocked
    // This calls cancel() which takes turn 1's running_turn and sets turn 2's
    let second_request = thread.update(cx, |thread, cx| thread.send_raw("second", cx));
    assert_eq!(thread.read_with(cx, |t, _| t.turn_id), 2);

    let running_turn_after_second_send =
        thread.read_with(cx, |thread, _| thread.running_turn.as_ref().map(|t| t.id));
    assert_eq!(
        running_turn_after_second_send,
        Some(2),
        "running_turn should be set to turn 2 after sending second message"
    );

    // Now signal first handler to complete
    first_complete_tx.send(()).ok();

    // First request completes - should NOT clear running_turn
    // because running_turn now belongs to turn 2
    first_request.await.unwrap();

    let running_turn_after_first =
        thread.read_with(cx, |thread, _| thread.running_turn.as_ref().map(|t| t.id));
    assert_eq!(
        running_turn_after_first,
        Some(2),
        "first turn completing should not clear running_turn (belongs to turn 2)"
    );

    // Second request completes - SHOULD clear running_turn
    second_request.await.unwrap();

    let running_turn_after_second = thread.read_with(cx, |thread, _| thread.running_turn.is_some());
    assert!(
        !running_turn_after_second,
        "second turn completing should clear running_turn"
    );
}

#[gpui::test]
async fn test_stale_cancelled_response_does_not_cancel_current_compaction(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let (first_complete_tx, first_complete_rx) = futures::channel::oneshot::channel::<()>();
    let first_complete_rx = RefCell::new(Some(first_complete_rx));
    let compaction_id = ContextCompactionId("test-compaction".into());

    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        let compaction_id = compaction_id.clone();
        move |params, thread, mut cx| {
            let first_complete_rx = first_complete_rx.borrow_mut().take();
            let is_first = params.prompt.iter().any(|content| {
                matches!(content, acp::ContentBlock::Text(text) if text.text.contains("first"))
            });
            let compaction_id = compaction_id.clone();

            async move {
                if is_first {
                    if let Some(rx) = first_complete_rx {
                        rx.await
                            .expect("first completion sender should still be alive");
                    }

                    thread.update(&mut cx, |thread, cx| {
                        thread.push_context_compaction(
                            ContextCompaction {
                                id: compaction_id,
                                status: ContextCompactionStatus::InProgress,
                                summary: None,
                            },
                            cx,
                        );
                    })?;

                    Ok(acp::PromptResponse::new(acp::StopReason::Cancelled))
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

    let first_request = thread.update(cx, |thread, cx| thread.send_raw("first", cx));
    assert_eq!(thread.read_with(cx, |thread, _| thread.turn_id), 1);

    let second_request = thread.update(cx, |thread, cx| thread.send_raw("second", cx));
    assert_eq!(thread.read_with(cx, |thread, _| thread.turn_id), 2);

    first_complete_tx
        .send(())
        .expect("first completion receiver should still be alive");

    let response = first_request
        .await
        .expect("first request should complete")
        .expect("first request should have response");
    assert_eq!(response.stop_reason, acp::StopReason::Cancelled);

    thread.read_with(cx, |thread, _| {
        let compaction = thread
            .entries
            .iter()
            .find_map(|entry| {
                let AgentThreadEntry::ContextCompaction(compaction) = entry else {
                    return None;
                };
                (compaction.id == compaction_id).then_some(compaction)
            })
            .expect("compaction entry should exist");

        assert_eq!(
            compaction.status,
            ContextCompactionStatus::InProgress,
            "a stale cancelled response from an older turn should not cancel current compaction"
        );
    });

    second_request
        .await
        .expect("second request should complete");
}

#[gpui::test]
async fn test_send_omits_message_id_without_client_user_message_id_support(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let connection = Rc::new(FakeAgentConnection::new().without_truncate_support());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    let response = thread
        .update(cx, |thread, cx| thread.send_raw("test message", cx))
        .await;

    assert!(response.is_ok(), "send should not fail: {response:?}");
    thread.read_with(cx, |thread, _| {
        let AgentThreadEntry::UserMessage(message) = &thread.entries[0] else {
            panic!("expected first entry to be a user message")
        };
        assert_eq!(message.protocol_id, None);
        assert_eq!(message.client_id, None);
        assert!(message.is_optimistic);
    });
}

#[gpui::test]
async fn test_send_returns_cancelled_response_and_marks_tools_as_cancelled(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let connection = Rc::new(FakeAgentConnection::new().on_user_message(
        move |_params, thread, mut cx| {
            async move {
                thread
                    .update(&mut cx, |thread, cx| {
                        thread.handle_session_update(
                            acp::SessionUpdate::ToolCall(
                                acp::ToolCall::new(acp::ToolCallId::new("test-tool"), "Test Tool")
                                    .kind(acp::ToolKind::Fetch)
                                    .status(acp::ToolCallStatus::InProgress),
                            ),
                            cx,
                        )
                    })
                    .unwrap()
                    .unwrap();

                Ok(acp::PromptResponse::new(acp::StopReason::Cancelled))
            }
            .boxed_local()
        },
    ));

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    let response = thread
        .update(cx, |thread, cx| thread.send_raw("test message", cx))
        .await;

    let response = response
        .expect("send should succeed")
        .expect("should have response");
    assert_eq!(
        response.stop_reason,
        acp::StopReason::Cancelled,
        "response should have Cancelled stop_reason"
    );

    thread.read_with(cx, |thread, _| {
        let tool_entry = thread
            .entries
            .iter()
            .find_map(|e| {
                if let AgentThreadEntry::ToolCall(call) = e {
                    Some(call)
                } else {
                    None
                }
            })
            .expect("should have tool call entry");

        assert!(
            matches!(tool_entry.status, ToolCallStatus::Canceled),
            "tool should be marked as Canceled when response is Cancelled, got {:?}",
            tool_entry.status
        );
    });
}
