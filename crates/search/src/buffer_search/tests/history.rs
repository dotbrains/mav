use super::*;

#[gpui::test]
async fn test_search_query_history(cx: &mut TestAppContext) {
    let (_editor, search_bar, cx) = init_test(cx);

    // Add 3 search items into the history.
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("a", None, true, window, cx)
        })
        .await
        .unwrap();
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("b", None, true, window, cx)
        })
        .await
        .unwrap();
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("c", Some(SearchOptions::CASE_SENSITIVE), true, window, cx)
        })
        .await
        .unwrap();
    // Ensure that the latest search is active.
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "c");
        assert_eq!(search_bar.search_options, SearchOptions::CASE_SENSITIVE);
    });

    // Next history query after the latest should preserve the current query.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.next_history_query(&NextHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "c");
        assert_eq!(search_bar.search_options, SearchOptions::CASE_SENSITIVE);
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.next_history_query(&NextHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "c");
        assert_eq!(search_bar.search_options, SearchOptions::CASE_SENSITIVE);
    });

    // Previous query should navigate backwards through history.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "b");
        assert_eq!(search_bar.search_options, SearchOptions::CASE_SENSITIVE);
    });

    // Further previous items should go over the history in reverse order.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "a");
        assert_eq!(search_bar.search_options, SearchOptions::CASE_SENSITIVE);
    });

    // Previous items should never go behind the first history item.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "a");
        assert_eq!(search_bar.search_options, SearchOptions::CASE_SENSITIVE);
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "a");
        assert_eq!(search_bar.search_options, SearchOptions::CASE_SENSITIVE);
    });

    // Next items should go over the history in the original order.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.next_history_query(&NextHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "b");
        assert_eq!(search_bar.search_options, SearchOptions::CASE_SENSITIVE);
    });

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("ba", None, true, window, cx)
        })
        .await
        .unwrap();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "ba");
        assert_eq!(search_bar.search_options, SearchOptions::NONE);
    });

    // New search input should add another entry to history and move the selection to the end of the history.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "c");
        assert_eq!(search_bar.search_options, SearchOptions::NONE);
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "b");
        assert_eq!(search_bar.search_options, SearchOptions::NONE);
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.next_history_query(&NextHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "c");
        assert_eq!(search_bar.search_options, SearchOptions::NONE);
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.next_history_query(&NextHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "ba");
        assert_eq!(search_bar.search_options, SearchOptions::NONE);
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.next_history_query(&NextHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), "ba");
        assert_eq!(search_bar.search_options, SearchOptions::NONE);
    });
}

#[perf]
#[gpui::test]
async fn test_search_query_history_autoscroll(cx: &mut TestAppContext) {
    let (_editor, search_bar, cx) = init_test(cx);

    // Add a long multi-line query that exceeds the editor's max
    // visible height (4 lines), then a short query.
    let long_query = "line1\nline2\nline3\nline4\nline5\nline6";
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search(long_query, None, true, window, cx)
        })
        .await
        .unwrap();
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("short", None, true, window, cx)
        })
        .await
        .unwrap();

    // Navigate back to the long entry. Since "short" is single-line,
    // the history navigation is allowed.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.previous_history_query(&PreviousHistoryQuery, window, cx);
    });
    cx.background_executor.run_until_parked();
    search_bar.update(cx, |search_bar, cx| {
        assert_eq!(search_bar.query(cx), long_query);
    });

    // The cursor should be scrolled into view despite the content
    // exceeding the editor's max visible height.
    search_bar.update_in(cx, |search_bar, window, cx| {
            let snapshot = search_bar
                .query_editor
                .update(cx, |editor, cx| editor.snapshot(window, cx));
            let cursor_row = search_bar
                .query_editor
                .read(cx)
                .selections
                .newest_display(&snapshot)
                .head()
                .row();
            let scroll_top = search_bar
                .query_editor
                .update(cx, |editor, cx| editor.scroll_position(cx).y);
            let visible_lines = search_bar
                .query_editor
                .read(cx)
                .visible_line_count()
                .unwrap_or(0.0);
            let scroll_bottom = scroll_top + visible_lines;
            assert!(
                (cursor_row.0 as f64) < scroll_bottom,
                "cursor row {cursor_row:?} should be visible (scroll range {scroll_top}..{scroll_bottom})"
            );
        });
}
