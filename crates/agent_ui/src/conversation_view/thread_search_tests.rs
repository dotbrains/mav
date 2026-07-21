use super::tests::*;
use super::*;

#[gpui::test]
async fn test_thread_search_finds_matches_across_entries(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new(
            "Yes, you can substitute banana for plantain in this recipe.".into(),
        ),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());

    thread
        .update(cx, |thread, cx| {
            thread.send_raw("Can I use banana here?", cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new(
            "Banana yogurt also works as a topping; whisk the banana smooth first.".into(),
        ),
    )]);

    thread
        .update(cx, |thread, cx| {
            thread.send_raw("What about as a topping?", cx)
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
            editor.set_text("banana", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    let (match_count, active_text) =
        bar.read_with(cx, |bar, cx| (bar.match_count(), bar.active_match_text(cx)));
    assert_eq!(
        match_count, 4,
        "expected 4 matches for case-insensitive 'banana'"
    );
    assert_eq!(active_text.as_deref(), Some("1/4"));

    thread_view.read_with(cx, |view, _| {
        assert_eq!(view.list_state.logical_scroll_top().item_ix, 0);
    });

    bar.update_in(cx, |bar, window, cx| {
        bar.select_next_match(&super::thread_search_bar::SelectNextThreadMatch, window, cx);
    });
    cx.run_until_parked();
    let active_text_2 = bar.read_with(cx, |bar, cx| bar.active_match_text(cx));
    assert_eq!(active_text_2.as_deref(), Some("2/4"));

    bar.update_in(cx, |bar, window, cx| {
        bar.select_prev_match(
            &super::thread_search_bar::SelectPreviousThreadMatch,
            window,
            cx,
        );
    });
    let active_text_3 = bar.read_with(cx, |bar, cx| bar.active_match_text(cx));
    assert_eq!(active_text_3.as_deref(), Some("1/4"));

    bar.update_in(cx, |bar, window, cx| {
        bar.query_editor.update(cx, |editor, cx| {
            editor.set_text("apple", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();
    let (match_count_apple, active_text_apple) =
        bar.read_with(cx, |bar, cx| (bar.match_count(), bar.active_match_text(cx)));
    assert_eq!(match_count_apple, 0);
    assert_eq!(active_text_apple.as_deref(), Some("0/0"));
}

#[gpui::test]
async fn test_thread_search_includes_expanded_thinking_blocks(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![
        acp::SessionUpdate::AgentThoughtChunk(acp::ContentChunk::new(
            "Hidden papaya reasoning.".into(),
        )),
        acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
            "Final answer without that fruit.".into(),
        )),
    ]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("Think this through", cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let (assistant_entry_ix, thought_chunk_ix) = thread.read_with(cx, |thread, _| {
        thread
            .entries()
            .iter()
            .enumerate()
            .find_map(|(entry_ix, entry)| match entry {
                AgentThreadEntry::AssistantMessage(message) => message
                    .chunks
                    .iter()
                    .position(|chunk| matches!(chunk, AssistantMessageChunk::Thought { .. }))
                    .map(|chunk_ix| (entry_ix, chunk_ix)),
                _ => None,
            })
            .expect("assistant thought chunk should exist")
    });

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
            editor.set_text("papaya", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        bar.read_with(cx, |bar, _| bar.match_count()),
        0,
        "collapsed thinking content should not be searched",
    );

    thread_view.update(cx, |view, cx| {
        view.entry_view_state.update(cx, |state, cx| {
            state.toggle_thinking_block_expansion((assistant_entry_ix, thought_chunk_ix), cx);
        });
    });
    bar.update_in(cx, |bar, window, cx| {
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        bar.read_with(cx, |bar, _| bar.match_count()),
        1,
        "expanded thinking content should be searchable",
    );
}

#[gpui::test]
async fn test_thread_search_includes_expanded_tool_call_content(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("search-tool-content");
    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(
        acp::ToolCall::new(tool_call_id.clone(), "Inspect output")
            .kind(acp::ToolKind::Other)
            .status(acp::ToolCallStatus::Completed)
            .content(vec!["Tool output mentions papaya once.".into()]),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("Run the tool", cx))
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
            editor.set_text("papaya", window, cx);
        });
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        bar.read_with(cx, |bar, _| bar.match_count()),
        0,
        "collapsed tool-call content should not be searched",
    );

    thread_view.update(cx, |view, cx| {
        view.entry_view_state.update(cx, |state, _cx| {
            state.expand_tool_call(tool_call_id);
        });
    });
    bar.update_in(cx, |bar, window, cx| {
        bar.update_matches(window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        bar.read_with(cx, |bar, _| bar.match_count()),
        1,
        "expanded tool-call content should be searchable",
    );
}
