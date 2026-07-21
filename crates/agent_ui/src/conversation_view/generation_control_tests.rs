use super::tests::*;
use super::*;

#[gpui::test]
async fn test_stale_stop_does_not_disable_follow_tail_during_regenerate(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Original message to edit", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    let user_message_editor = conversation_view.read_with(cx, |view, cx| {
        view.active_thread()
            .map(|active| &active.read(cx).entry_view_state)
            .as_ref()
            .unwrap()
            .read(cx)
            .entry(0)
            .unwrap()
            .message_editor()
            .unwrap()
            .clone()
    });

    cx.focus(&user_message_editor);
    user_message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Edited message content", window, cx);
    });

    user_message_editor.update_in(cx, |_editor, window, cx| {
        window.dispatch_action(Box::new(Chat), cx);
    });

    cx.run_until_parked();

    conversation_view.read_with(cx, |view, cx| {
        let active = view.active_thread().unwrap();
        let active = active.read(cx);

        assert_eq!(active.thread.read(cx).status(), ThreadStatus::Generating);
        assert!(
            active.list_state.is_following_tail(),
            "stale stop events from the cancelled turn must not disable follow-tail for the new turn"
        );
    });
}

struct GeneratingThreadSetup {
    conversation_view: Entity<ConversationView>,
    thread: Entity<AcpThread>,
    message_editor: Entity<MessageEditor>,
}

async fn setup_generating_thread(
    cx: &mut TestAppContext,
) -> (GeneratingThreadSetup, &mut VisualTestContext) {
    let connection = StubAgentConnection::new();

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    let (thread, session_id) = conversation_view.read_with(cx, |view, cx| {
        let thread = view
            .active_thread()
            .as_ref()
            .unwrap()
            .read(cx)
            .thread
            .clone();
        (thread.clone(), thread.read(cx).session_id().clone())
    });

    cx.run_until_parked();

    cx.update(|_, cx| {
        connection.send_update(
            session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("Response chunk".into())),
            cx,
        );
    });

    cx.run_until_parked();

    thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.status(), ThreadStatus::Generating);
    });

    (
        GeneratingThreadSetup {
            conversation_view,
            thread,
            message_editor,
        },
        cx,
    )
}

#[gpui::test]
async fn test_escape_cancels_generation_from_conversation_focus(cx: &mut TestAppContext) {
    init_test(cx);

    let (setup, cx) = setup_generating_thread(cx).await;

    let focus_handle = setup
        .conversation_view
        .read_with(cx, |view, cx| view.focus_handle(cx));
    cx.update(|window, cx| {
        window.focus(&focus_handle, cx);
    });

    setup.conversation_view.update_in(cx, |_, window, cx| {
        window.dispatch_action(menu::Cancel.boxed_clone(), cx);
    });

    cx.run_until_parked();

    setup.thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.status(), ThreadStatus::Idle);
    });
}

#[gpui::test]
async fn test_escape_cancels_generation_from_editor_focus(cx: &mut TestAppContext) {
    init_test(cx);

    let (setup, cx) = setup_generating_thread(cx).await;

    let editor_focus_handle = setup
        .message_editor
        .read_with(cx, |editor, cx| editor.focus_handle(cx));
    cx.update(|window, cx| {
        window.focus(&editor_focus_handle, cx);
    });

    setup.message_editor.update_in(cx, |_, window, cx| {
        window.dispatch_action(editor::actions::Cancel.boxed_clone(), cx);
    });

    cx.run_until_parked();

    setup.thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.status(), ThreadStatus::Idle);
    });
}

#[gpui::test]
async fn test_escape_when_idle_is_noop(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(StubAgentConnection::new()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let thread = conversation_view.read_with(cx, |view, cx| {
        view.active_thread().unwrap().read(cx).thread.clone()
    });

    thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.status(), ThreadStatus::Idle);
    });

    let focus_handle = conversation_view.read_with(cx, |view, _cx| view.focus_handle.clone());
    cx.update(|window, cx| {
        window.focus(&focus_handle, cx);
    });

    conversation_view.update_in(cx, |_, window, cx| {
        window.dispatch_action(menu::Cancel.boxed_clone(), cx);
    });

    cx.run_until_parked();

    thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.status(), ThreadStatus::Idle);
    });
}

#[gpui::test]
async fn test_interrupt(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Message 1", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    let (thread, session_id) = conversation_view.read_with(cx, |view, cx| {
        let thread = view.active_thread().unwrap().read(cx).thread.clone();

        (thread.clone(), thread.read(cx).session_id().clone())
    });

    cx.run_until_parked();

    cx.update(|_, cx| {
        connection.send_update(
            session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("Message 1 resp".into())),
            cx,
        );
    });

    cx.run_until_parked();

    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc::indoc! {"
                    ## User

                    Message 1

                    ## Assistant

                    Message 1 resp

                "}
        )
    });

    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Message 2", window, cx);
    });
    active_thread(&conversation_view, cx)
        .update_in(cx, |view, window, cx| view.interrupt_and_send(window, cx));

    cx.update(|_, cx| {
        connection.send_update(
            session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("onse".into())),
            cx,
        );
    });

    cx.run_until_parked();

    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc::indoc! {"
                    ## User

                    Message 1

                    ## Assistant

                    Message 1 response

                    ## User

                    Message 2

                "}
        )
    });

    cx.update(|_, cx| {
        connection.send_update(
            session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                "Message 2 response".into(),
            )),
            cx,
        );
        connection.end_turn(session_id.clone(), acp::StopReason::EndTurn);
    });

    cx.run_until_parked();

    thread.read_with(cx, |thread, cx| {
        assert_eq!(
            thread.to_markdown(cx),
            indoc::indoc! {"
                    ## User

                    Message 1

                    ## Assistant

                    Message 1 response

                    ## User

                    Message 2

                    ## Assistant

                    Message 2 response

                "}
        )
    });
}
