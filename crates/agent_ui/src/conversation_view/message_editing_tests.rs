use super::tests::*;
use super::*;

#[gpui::test]
async fn test_message_editing_cancel(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Original message to edit", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    let user_message_editor = conversation_view.read_with(cx, |view, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            None
        );

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
    conversation_view.read_with(cx, |view, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            Some(0)
        );
    });

    user_message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Edited message content", window, cx);
    });

    user_message_editor.update_in(cx, |_editor, window, cx| {
        window.dispatch_action(Box::new(editor::actions::Cancel), cx);
    });

    conversation_view.read_with(cx, |view, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            None
        );
    });

    user_message_editor.read_with(cx, |editor, cx| {
        assert_eq!(editor.text(cx), "Original message to edit");
    });
}

#[gpui::test]
async fn test_message_doesnt_send_if_empty(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("", window, cx);
    });

    let thread = cx.read(|cx| {
        conversation_view
            .read(cx)
            .active_thread()
            .unwrap()
            .read(cx)
            .thread
            .clone()
    });
    let entries_before = cx.read(|cx| thread.read(cx).entries().len());

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| {
        view.send(window, cx);
    });
    cx.run_until_parked();

    let entries_after = cx.read(|cx| thread.read(cx).entries().len());
    assert_eq!(
        entries_before, entries_after,
        "No message should be sent when editor is empty"
    );
}

#[gpui::test]
async fn test_message_editing_regenerate(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response".into()),
    )]);

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
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            None
        );
        assert_eq!(
            view.active_thread()
                .unwrap()
                .read(cx)
                .thread
                .read(cx)
                .entries()
                .len(),
            2
        );

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

    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("New Response".into()),
    )]);

    user_message_editor.update_in(cx, |_editor, window, cx| {
        window.dispatch_action(Box::new(Chat), cx);
    });

    cx.run_until_parked();

    conversation_view.read_with(cx, |view, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            None
        );

        let active = view.active_thread().unwrap().read(cx);
        let entries = active.thread.read(cx).entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].to_markdown(cx),
            "## User\n\nEdited message content\n\n"
        );
        assert_eq!(
            entries[1].to_markdown(cx),
            "## Assistant\n\nNew Response\n\n"
        );

        let new_editor = active.entry_view_state.read_with(cx, |state, _cx| {
            assert!(!state.entry(1).unwrap().has_content());
            state.entry(0).unwrap().message_editor().unwrap().clone()
        });

        assert_eq!(new_editor.read(cx).text(cx), "Edited message content");
    })
}

#[gpui::test]
async fn test_message_editing_while_generating(cx: &mut TestAppContext) {
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

    let (user_message_editor, session_id) = conversation_view.read_with(cx, |view, cx| {
        let thread = view.active_thread().unwrap().read(cx).thread.read(cx);
        assert_eq!(thread.entries().len(), 1);

        let editor = view
            .active_thread()
            .map(|active| &active.read(cx).entry_view_state)
            .as_ref()
            .unwrap()
            .read(cx)
            .entry(0)
            .unwrap()
            .message_editor()
            .unwrap()
            .clone();

        (editor, thread.session_id().clone())
    });

    cx.focus(&user_message_editor);

    conversation_view.read_with(cx, |view, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            Some(0)
        );
    });

    user_message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Edited message content", window, cx);
    });

    conversation_view.read_with(cx, |view, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            Some(0)
        );
    });

    cx.update(|_, cx| {
        connection.send_update(
            session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("Response".into())),
            cx,
        );
        connection.end_turn(session_id, acp::StopReason::EndTurn);
    });

    conversation_view.read_with(cx, |view, cx| {
        assert_eq!(
            view.active_thread()
                .and_then(|active| active.read(cx).editing_message),
            Some(0)
        );
    });

    cx.run_until_parked();

    cx.update(|window, cx| {
        assert!(user_message_editor.focus_handle(cx).is_focused(window));
        assert_eq!(
            conversation_view
                .read(cx)
                .active_thread()
                .and_then(|active| active.read(cx).editing_message),
            Some(0)
        );
        assert_eq!(
            user_message_editor.read(cx).text(cx),
            "Edited message content"
        );
    });
}
