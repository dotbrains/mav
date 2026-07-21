use super::tests::*;
use super::*;

#[gpui::test]
async fn test_thread_search_refreshes_on_new_thread_entry(cx: &mut TestAppContext) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("First reply mentions banana once.".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("Tell me about banana", cx))
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

    let count_before = bar.read_with(cx, |bar, _| bar.match_count());
    assert!(
        count_before >= 2,
        "expected at least two initial matches, got {count_before}",
    );

    bar.update_in(cx, |bar, window, cx| {
        bar.select_next_match(&super::thread_search_bar::SelectNextThreadMatch, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        bar.read_with(cx, |bar, _| bar.active_match_index()),
        Some(1),
        "setup precondition: second match should be active before the refresh",
    );

    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Banana banana: two more banana hits here.".into()),
    )]);
    thread
        .update(cx, |thread, cx| thread.send_raw("More banana please", cx))
        .await
        .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(super::thread_search_bar::SEARCH_UPDATE_DEBOUNCE * 2);
    cx.run_until_parked();

    let (count_after, active_after) =
        bar.read_with(cx, |bar, _| (bar.match_count(), bar.active_match_index()));
    assert!(
        count_after > count_before,
        "thread subscription should refresh matches after new content \
         streamed in: before={count_before}, after={count_after}",
    );
    assert_eq!(
        active_after,
        Some(1),
        "refreshing matches should preserve the active result when it still exists",
    );
}

/// Regression test for re-entering `ThreadView` during search navigation.
#[gpui::test]
async fn test_thread_search_select_next_from_thread_view_update_does_not_panic(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Banana banana banana, the banana fits the banana bread.".into()),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("Need banana help", cx))
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

    let initial_match_count = bar.read_with(cx, |bar, _| bar.match_count());
    assert!(
        initial_match_count >= 2,
        "setup precondition: expected at least 2 matches, got {}",
        initial_match_count,
    );

    thread_view.update_in(cx, |view, window, cx| {
        let bar = view
            .thread_search_bar
            .clone()
            .expect("bar should still be set");
        bar.update(cx, |bar, cx| {
            bar.select_next_match(&super::thread_search_bar::SelectNextThreadMatch, window, cx);
        });
    });
    cx.run_until_parked();

    let active_after = bar.read_with(cx, |bar, _| bar.active_match_index());
    assert_eq!(
        active_after,
        Some(1),
        "select_next_match should have advanced from match 0 to match 1",
    );
}

/// `editor::Cancel` should dismiss thread search before reaching workspace handlers.
#[gpui::test]
async fn test_thread_search_editor_cancel_dismisses_bar(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(StubAgentConnection::new()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let thread_view = active_thread(&conversation_view, cx);
    thread_view.update_in(cx, |view, window, cx| {
        view.toggle_search(&crate::ToggleSearch, window, cx);
    });
    cx.run_until_parked();

    let visible_before = thread_view.read_with(cx, |view, _| view.thread_search_visible);
    assert!(
        visible_before,
        "search bar should be visible after toggle_search"
    );

    let bar = thread_view
        .read_with(cx, |view, _| view.thread_search_bar.clone())
        .expect("bar should be set");
    let query_focus = bar.read_with(cx, |bar, cx| bar.query_editor.focus_handle(cx));
    cx.update(|window, cx| {
        window.focus(&query_focus, cx);
    });
    cx.run_until_parked();

    conversation_view.update_in(cx, |_, window, cx| {
        window.dispatch_action(editor::actions::Cancel.boxed_clone(), cx);
    });
    cx.run_until_parked();

    let visible_after = thread_view.read_with(cx, |view, _| view.thread_search_visible);
    assert!(
        !visible_after,
        "editor::Cancel should have dismissed the bar before reaching the workspace",
    );
}

/// JetBrains keymaps route Shift+Enter through `editor::NewlineBelow`.
#[gpui::test]
async fn test_thread_search_shift_enter_navigates_with_jetbrains_keymap(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        search::init(cx);

        let mut default_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/default-linux.json",
            cx,
        )
        .unwrap();
        for binding in &mut default_bindings {
            binding.set_meta(settings::KeybindSource::Default.meta());
        }
        cx.bind_keys(default_bindings);

        let mut jetbrains_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/linux/jetbrains.json",
            cx,
        )
        .unwrap();
        for binding in &mut jetbrains_bindings {
            binding.set_meta(settings::KeybindSource::Base.meta());
        }
        cx.bind_keys(jetbrains_bindings);
    });

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new(
            "Banana banana banana, multiple banana mentions in this reply.".into(),
        ),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("Need banana help", cx))
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

    let initial_count = bar.read_with(cx, |bar, _| bar.match_count());
    assert!(
        initial_count >= 2,
        "test precondition: need at least 2 matches across the thread, got {}",
        initial_count,
    );
    assert_eq!(
        bar.read_with(cx, |bar, _| bar.active_match_index()),
        Some(0),
        "first match should be active after the bar populates its match list",
    );

    let query_focus = bar.read_with(cx, |bar, cx| bar.query_editor.focus_handle(cx));
    cx.update(|window, cx| {
        window.focus(&query_focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            query_focus.contains_focused(window, cx),
            "query editor must be focused before simulating shift-enter",
        );
    });

    cx.simulate_keystrokes("shift-enter");
    cx.run_until_parked();

    let query_text_after = bar.read_with(cx, |bar, cx| bar.query_editor.read(cx).text(cx));
    assert!(
        !query_text_after.contains('\n'),
        "shift-enter must not insert a newline into the query buffer; got {:?}",
        query_text_after,
    );

    let active_after = bar.read_with(cx, |bar, _| bar.active_match_index());
    assert_eq!(
        active_after,
        Some(initial_count - 1),
        "shift-enter should have wrapped active match from 0 to {} (got {:?})",
        initial_count - 1,
        active_after,
    );
}

/// `f3`/`shift-f3` are bound in the broad `AcpThread` context, so they must
/// navigate matches even when focus is outside the search bar.
#[gpui::test]
async fn test_thread_search_navigates_from_outside_search_bar(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        search::init(cx);
        let mut default_bindings = settings::KeymapFile::load_asset_allow_partial_failure(
            "keymaps/default-linux.json",
            cx,
        )
        .unwrap();
        for binding in &mut default_bindings {
            binding.set_meta(settings::KeybindSource::Default.meta());
        }
        cx.bind_keys(default_bindings);
    });

    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new(
            "Banana banana banana, multiple banana mentions in this reply.".into(),
        ),
    )]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection.clone()), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let thread = active_thread(&conversation_view, cx).read_with(cx, |view, _| view.thread.clone());
    thread
        .update(cx, |thread, cx| thread.send_raw("Need banana help", cx))
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

    let initial_count = bar.read_with(cx, |bar, _| bar.match_count());
    assert!(
        initial_count >= 2,
        "test precondition: need at least 2 matches, got {}",
        initial_count,
    );
    assert_eq!(
        bar.read_with(cx, |bar, _| bar.active_match_index()),
        Some(0)
    );

    let thread_focus = thread_view.read_with(cx, |view, cx| view.focus_handle(cx));
    cx.update(|window, cx| window.focus(&thread_focus, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let bar_focused = bar.read_with(cx, |bar, cx| {
            bar.query_editor
                .focus_handle(cx)
                .contains_focused(window, cx)
        });
        assert!(!bar_focused, "search bar must not be focused for this test");
    });

    cx.simulate_keystrokes("f3");
    cx.run_until_parked();
    assert_eq!(
        bar.read_with(cx, |bar, _| bar.active_match_index()),
        Some(1),
        "f3 from outside the bar should advance to the next match",
    );

    cx.simulate_keystrokes("shift-f3");
    cx.run_until_parked();
    assert_eq!(
        bar.read_with(cx, |bar, _| bar.active_match_index()),
        Some(0),
        "shift-f3 from outside the bar should return to the previous match",
    );
}
