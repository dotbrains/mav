use super::tests::*;
use super::*;

#[gpui::test]
async fn test_scroll_to_most_recent_user_prompt(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response 1".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    let thread = conversation_view
        .read_with(cx, |view, cx| {
            view.active_thread().map(|r| r.read(cx).thread.clone())
        })
        .unwrap();

    thread
        .update(cx, |thread, cx| thread.send_raw("Prompt 1", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response 2".into()),
    )]);
    thread
        .update(cx, |thread, cx| thread.send_raw("Prompt 2", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    active_thread(&conversation_view, cx).update(cx, |view, cx| {
        view.scroll_to_top(cx);
    });
    cx.run_until_parked();

    active_thread(&conversation_view, cx).update(cx, |view, cx| {
        view.scroll_to_most_recent_user_prompt(cx);
        assert_eq!(view.list_state.logical_scroll_top().item_ix, 2);
    });
}

#[gpui::test]
async fn test_scroll_to_most_recent_user_prompt_falls_back_to_bottom_without_user_messages(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;
    active_thread(&conversation_view, cx).update(cx, |view, cx| {
        view.scroll_to_most_recent_user_prompt(cx);
        assert_eq!(view.list_state.logical_scroll_top().item_ix, 0);
    });
}

#[gpui::test]
async fn test_manually_editing_title_updates_acp_thread_title(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let active = active_thread(&conversation_view, cx);
    let title_editor = cx.read(|cx| active.read(cx).title_editor.clone());
    let thread = cx.read(|cx| active.read(cx).thread.clone());

    title_editor.read_with(cx, |editor, cx| {
        assert!(!editor.read_only(cx));
    });
    cx.focus(&conversation_view);
    cx.focus(&title_editor);
    cx.dispatch_action(editor::actions::DeleteLine);
    cx.simulate_input("My Custom Title");
    cx.run_until_parked();

    title_editor.read_with(cx, |editor, cx| {
        assert_eq!(editor.text(cx), "My Custom Title");
    });
    thread.read_with(cx, |thread, _cx| {
        assert_eq!(thread.title(), Some("My Custom Title".into()));
    });
}

#[gpui::test]
async fn test_max_tokens_error_is_rendered(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Some prompt", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    let session_id = conversation_view.read_with(cx, |view, cx| {
        view.active_thread()
            .unwrap()
            .read(cx)
            .thread
            .read(cx)
            .session_id()
            .clone()
    });

    cx.run_until_parked();

    cx.update(|_, _cx| {
        connection.end_turn(session_id, acp::StopReason::MaxTokens);
    });
    cx.run_until_parked();

    conversation_view.read_with(cx, |conversation_view, cx| {
        let state = conversation_view.active_thread().unwrap();
        assert!(matches!(
            &state.read(cx).thread_error,
            Some(ThreadError::MaxOutputTokens)
        ));
    });
}
