use super::tests::*;
use super::*;

#[gpui::test]
async fn test_notification_for_stop_event(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });

    cx.deactivate_window();

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    assert!(
        cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some())
    );
}

#[gpui::test]
async fn test_no_notification_when_queued_message_will_be_auto_sent(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("first", window, cx);
    });

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    let session_id = conversation_view.read_with(cx, |view, cx| {
        view.active_thread()
            .unwrap()
            .read(cx)
            .thread
            .read(cx)
            .session_id()
            .clone()
    });

    active_thread(&conversation_view, cx).update_in(cx, |thread, window, cx| {
        thread.add_to_queue(
            vec![acp::ContentBlock::Text(acp::TextContent::new(
                "queued".to_string(),
            ))],
            vec![],
            window,
            cx,
        );
    });

    cx.deactivate_window();
    cx.run_until_parked();

    cx.update(|_, cx| {
        connection.send_update(
            session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("first response".into())),
            cx,
        );
        connection.end_turn(session_id, acp::StopReason::EndTurn);
    });

    cx.run_until_parked();

    assert_eq!(
        cx.windows()
            .iter()
            .filter(|window| window.downcast::<AgentNotification>().is_some())
            .count(),
        0,
        "No notification should fire when a queued message will be auto-sent on Stopped"
    );
}

#[gpui::test]
async fn test_notification_for_error(cx: &mut TestAppContext) {
    init_test(cx);

    let server = FakeAcpAgentServer::new();
    let (conversation_view, cx) = setup_conversation_view(server.clone(), cx).await;

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });

    cx.deactivate_window();
    server.fail_next_prompt();

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    assert!(
        cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some())
    );
}
