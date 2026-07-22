use super::*;

#[gpui::test]
async fn test_search_option_handling(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_test(cx);

    // show with options should make current search case sensitive
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.show(window, cx);
            search_bar.search("us", Some(SearchOptions::CASE_SENSITIVE), true, window, cx)
        })
        .await
        .unwrap();
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            display_points_of(editor.all_text_background_highlights(window, cx)),
            &[DisplayPoint::new(DisplayRow(2), 43)..DisplayPoint::new(DisplayRow(2), 45),]
        );
    });

    // search_suggested should restore default options
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.search_suggested(None, window, cx);
        assert_eq!(search_bar.search_options, SearchOptions::NONE)
    });

    // toggling a search option should update the defaults
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search(
                "regex",
                Some(SearchOptions::CASE_SENSITIVE),
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx)
    });
    let mut editor_notifications = cx.notifications(&editor);
    editor_notifications.next().await;
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            display_points_of(editor.all_text_background_highlights(window, cx)),
            &[DisplayPoint::new(DisplayRow(0), 35)..DisplayPoint::new(DisplayRow(0), 40),]
        );
    });

    // defaults should still include whole word
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.search_suggested(None, window, cx);
        assert_eq!(
            search_bar.search_options,
            SearchOptions::CASE_SENSITIVE | SearchOptions::WHOLE_WORD
        )
    });
}
