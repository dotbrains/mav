use super::tests::*;
use super::*;

#[gpui::test]
async fn test_thread_search_scrolls_to_later_user_message_match(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("First reply, no fruit here.".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("First question", cx))
        .await
        .unwrap();
    cx.run_until_parked();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Second reply, still no fruit.".into()),
    )]);
    thread
        .update(cx, |thread, cx| thread.send_raw("Where is the papaya?", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let thread_view = active_thread(&conversation_view, cx);
    let papaya_entry_ix = thread.read_with(cx, |thread, _| {
        thread
            .entries()
            .iter()
            .rposition(|entry| matches!(entry, AgentThreadEntry::UserMessage(_)))
            .expect("a user message entry should exist")
    });

    thread_view.update_in(cx, |view, window, cx| {
        view.toggle_search(&crate::ToggleSearch, window, cx);
    });
    cx.run_until_parked();
    let bar = thread_view
        .read_with(cx, |view, _| view.thread_search_bar.clone())
        .expect("thread_search_bar should be set after toggle_search");
    bar.update_in(cx, |bar, window, cx| {
        bar.query_editor.update(cx, |editor, cx| {
            editor.set_text("papaya", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    bar.read_with(cx, |bar, _| {
        assert_eq!(
            bar.match_count(),
            1,
            "only the second user message matches 'papaya'"
        );
    });

    thread_view.read_with(cx, |view, _| {
        assert_eq!(
            view.list_state.logical_scroll_top().item_ix,
            papaya_entry_ix,
            "list should scroll to the user-message entry that owns the match",
        );
    });
}

/// Passive rescans (streaming updates, unrelated expansion toggles, query
/// refinement) must not yank the list back to the active match.
#[gpui::test]
async fn test_thread_search_passive_rescan_preserves_scroll(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("First reply, no fruit here.".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("First question", cx))
        .await
        .unwrap();
    cx.run_until_parked();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Second reply, still no fruit.".into()),
    )]);
    thread
        .update(cx, |thread, cx| thread.send_raw("Where is the papaya?", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let thread_view = active_thread(&conversation_view, cx);
    let papaya_entry_ix = thread.read_with(cx, |thread, _| {
        thread
            .entries()
            .iter()
            .rposition(|entry| matches!(entry, AgentThreadEntry::UserMessage(_)))
            .expect("a user message entry should exist")
    });

    thread_view.update_in(cx, |view, window, cx| {
        view.toggle_search(&crate::ToggleSearch, window, cx);
    });
    cx.run_until_parked();
    let bar = thread_view
        .read_with(cx, |view, _| view.thread_search_bar.clone())
        .expect("thread_search_bar should be set after toggle_search");
    bar.update_in(cx, |bar, window, cx| {
        bar.query_editor.update(cx, |editor, cx| {
            editor.set_text("papaya", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    thread_view.read_with(cx, |view, _| {
        assert_eq!(
            view.list_state.logical_scroll_top().item_ix,
            papaya_entry_ix,
        );
    });

    thread_view.update(cx, |view, _| {
        view.list_state.scroll_to(gpui::ListOffset {
            item_ix: 0,
            offset_in_item: gpui::px(0.),
        });
    });
    bar.update_in(cx, |bar, window, cx| {
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    thread_view.read_with(cx, |view, _| {
        assert_eq!(
            view.list_state.logical_scroll_top().item_ix,
            0,
            "a passive rescan must not scroll back to the active match",
        );
    });
}
