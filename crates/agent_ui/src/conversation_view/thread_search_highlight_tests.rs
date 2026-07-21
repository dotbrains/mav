use super::tests::*;
use super::*;

#[gpui::test]
async fn test_thread_search_dismiss_clears_highlights(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Mango is a tropical fruit.".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());

    thread
        .update(cx, |thread, cx| thread.send_raw("Tell me about mango", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let thread_view = active_thread(&conversation_view, cx);
    thread_view.update_in(cx, |view, window, cx| {
        view.toggle_search(&crate::ToggleSearch, window, cx);
    });
    cx.run_until_parked();

    let bar = thread_view
        .read_with(cx, |view, _| view.thread_search_bar.clone())
        .unwrap();
    bar.update_in(cx, |bar, window, cx| {
        bar.query_editor.update(cx, |editor, cx| {
            editor.set_text("mango", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    let entries = thread.read_with(cx, |thread, _| thread.entries().len());
    assert!(entries > 0);

    bar.update(cx, |bar, cx| bar.clear_highlights(cx));
    cx.run_until_parked();

    bar.read_with(cx, |bar, _| {
        assert_eq!(bar.match_count(), 0);
        assert!(bar.active_match_index().is_none());
    });
}

#[gpui::test]
async fn test_thread_search_release_clears_markdown_highlights(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Mango is a tropical fruit.".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("Tell me about mango", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let assistant_markdown = thread.read_with(cx, |thread, _| {
        thread
            .entries()
            .iter()
            .find_map(|entry| match entry {
                AgentThreadEntry::AssistantMessage(message) => {
                    message.chunks.iter().find_map(|chunk| match chunk {
                        AssistantMessageChunk::Message { block, .. } => block.markdown().cloned(),
                        AssistantMessageChunk::Thought { .. } => None,
                    })
                }
                _ => None,
            })
            .expect("assistant message should have markdown")
    });

    let entry_view_state = active_thread(&conversation_view, cx)
        .read_with(cx, |view, _| view.entry_view_state.clone());
    let on_activate_match: Arc<dyn Fn(usize, &mut Window, &mut App)> = Arc::new(|_, _, _| {});
    let bar = cx.update(|window, cx| {
        cx.new(|cx| {
            super::thread_search_bar::ThreadSearchBar::new(
                thread.clone(),
                entry_view_state,
                on_activate_match,
                window,
                cx,
            )
        })
    });

    bar.update_in(cx, |bar, window, cx| {
        bar.query_editor.update(cx, |editor, cx| {
            editor.set_text("mango", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    assert!(
        assistant_markdown.read_with(cx, |markdown, _| !markdown.search_highlights().is_empty()),
        "search should have highlighted the assistant markdown before release",
    );

    drop(bar);
    cx.update(|_, _| {});
    cx.run_until_parked();

    assert!(
        assistant_markdown.read_with(cx, |markdown, _| markdown.search_highlights().is_empty()),
        "releasing the search bar should clear retained markdown highlights",
    );
}

/// Past user-message hits must be painted on the inner `Editor`.
#[gpui::test]
async fn test_thread_search_highlights_user_message_editor(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Sure, I can help with that.".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| {
            thread.send_raw("Where do I find a kumquat?", cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let thread_view = active_thread(&conversation_view, cx);
    thread_view.update_in(cx, |view, window, cx| {
        view.toggle_search(&crate::ToggleSearch, window, cx);
    });
    cx.run_until_parked();

    let bar = thread_view
        .read_with(cx, |view, _| view.thread_search_bar.clone())
        .expect("thread_search_bar should be set after toggle_search");
    bar.update_in(cx, |bar, window, cx| {
        bar.query_editor.update(cx, |editor, cx| {
            editor.set_text("kumquat", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    let match_count = bar.read_with(cx, |bar, _| bar.match_count());
    assert_eq!(
        match_count, 1,
        "expected exactly one match for 'kumquat' (in the user message)",
    );

    let user_message_editor = thread_view.read_with(cx, |view, cx| {
        view.entry_view_state
            .read(cx)
            .entry(0)
            .and_then(|entry| entry.message_editor())
            .map(|message_editor| message_editor.read(cx).editor().clone())
            .expect("entry 0 should be a user message with a message editor")
    });
    let has_highlight = user_message_editor.read_with(cx, |editor, _cx| {
        editor.has_background_highlights(editor::HighlightKey::BufferSearchHighlights)
    });
    assert!(
        has_highlight,
        "user message editor should carry BufferSearchHighlights after the bar's matcher ran",
    );

    bar.update(cx, |bar, cx| bar.clear_highlights(cx));
    cx.run_until_parked();
    let has_highlight_after_clear = user_message_editor.read_with(cx, |editor, _cx| {
        editor.has_background_highlights(editor::HighlightKey::BufferSearchHighlights)
    });
    assert!(
        !has_highlight_after_clear,
        "clear_highlights should remove the editor-backed highlights",
    );
}
