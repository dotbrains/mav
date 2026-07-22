use super::*;

#[gpui::test]
async fn test_search_options_changes(cx: &mut TestAppContext) {
    let (_editor, search_bar, cx) = init_test(cx);
    update_search_settings(
        SearchSettings {
            button: true,
            whole_word: false,
            case_sensitive: false,
            include_ignored: false,
            regex: false,
            center_on_match: false,
        },
        cx,
    );

    let deploy = Deploy {
        focus: true,
        replace_enabled: false,
        selection_search_enabled: true,
    };

    search_bar.update_in(cx, |search_bar, window, cx| {
        assert_eq!(
            search_bar.search_options,
            SearchOptions::NONE,
            "Should have no search options enabled by default"
        );
        search_bar.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx);
        assert_eq!(
            search_bar.search_options,
            SearchOptions::WHOLE_WORD,
            "Should enable the option toggled"
        );
        assert!(
            !search_bar.dismissed,
            "Search bar should be present and visible"
        );
        search_bar.deploy(&deploy, None, window, cx);
        assert_eq!(
            search_bar.search_options,
            SearchOptions::WHOLE_WORD,
            "After (re)deploying, the option should still be enabled"
        );

        search_bar.dismiss(&Dismiss, window, cx);
        search_bar.deploy(&deploy, None, window, cx);
        assert_eq!(
            search_bar.search_options,
            SearchOptions::WHOLE_WORD,
            "After hiding and showing the search bar, search options should be preserved"
        );

        search_bar.toggle_search_option(SearchOptions::REGEX, window, cx);
        search_bar.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx);
        assert_eq!(
            search_bar.search_options,
            SearchOptions::REGEX,
            "Should enable the options toggled"
        );
        assert!(
            !search_bar.dismissed,
            "Search bar should be present and visible"
        );
        search_bar.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx);
    });

    update_search_settings(
        SearchSettings {
            button: true,
            whole_word: false,
            case_sensitive: true,
            include_ignored: false,
            regex: false,
            center_on_match: false,
        },
        cx,
    );
    search_bar.update_in(cx, |search_bar, window, cx| {
            assert_eq!(
                search_bar.search_options,
                SearchOptions::REGEX | SearchOptions::WHOLE_WORD,
                "Should have no search options enabled by default"
            );

            search_bar.deploy(&deploy, None, window, cx);
            assert_eq!(
                search_bar.search_options,
                SearchOptions::REGEX | SearchOptions::WHOLE_WORD,
                "Toggling a non-dismissed search bar with custom options should not change the default options"
            );
            search_bar.dismiss(&Dismiss, window, cx);
            search_bar.deploy(&deploy, None, window, cx);
            assert_eq!(
                search_bar.configured_options,
                SearchOptions::CASE_SENSITIVE,
                "After a settings update and toggling the search bar, configured options should be updated"
            );
            assert_eq!(
                search_bar.search_options,
                SearchOptions::CASE_SENSITIVE,
                "After a settings update and toggling the search bar, configured options should be used"
            );
        });

    update_search_settings(
        SearchSettings {
            button: true,
            whole_word: true,
            case_sensitive: true,
            include_ignored: false,
            regex: false,
            center_on_match: false,
        },
        cx,
    );

    search_bar.update_in(cx, |search_bar, window, cx| {
            search_bar.deploy(&deploy, None, window, cx);
            search_bar.dismiss(&Dismiss, window, cx);
            search_bar.show(window, cx);
            assert_eq!(
                search_bar.search_options,
                SearchOptions::CASE_SENSITIVE | SearchOptions::WHOLE_WORD,
                "Calling deploy on an already deployed search bar should not prevent settings updates from being detected"
            );
        });
}

#[gpui::test]
async fn test_select_occurrence_case_sensitivity(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_test(cx);
    let mut editor_cx = EditorTestContext::for_editor_in(editor, cx).await;

    // Start with case sensitive search settings.
    let mut search_settings = SearchSettings::default();
    search_settings.case_sensitive = true;
    update_search_settings(search_settings, cx);
    search_bar.update(cx, |search_bar, cx| {
        let mut search_options = search_bar.search_options;
        search_options.insert(SearchOptions::CASE_SENSITIVE);
        search_bar.set_search_options(search_options, cx);
    });

    editor_cx.set_state("«ˇfoo»\nFOO\nFoo\nfoo");
    editor_cx.update_editor(|e, window, cx| {
        e.select_next(&Default::default(), window, cx).unwrap();
    });
    editor_cx.assert_editor_state("«ˇfoo»\nFOO\nFoo\n«ˇfoo»");

    // Update the search bar's case sensitivite toggle, so we can later
    // confirm that `select_next` will now be case-insensitive.
    editor_cx.set_state("«ˇfoo»\nFOO\nFoo\nfoo");
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.toggle_case_sensitive(&Default::default(), window, cx);
    });
    editor_cx.update_editor(|e, window, cx| {
        e.select_next(&Default::default(), window, cx).unwrap();
    });
    editor_cx.assert_editor_state("«ˇfoo»\n«ˇFOO»\nFoo\nfoo");

    // Confirm that, after dismissing the search bar, only the editor's
    // search settings actually affect the behavior of `select_next`.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.dismiss(&Default::default(), window, cx);
    });
    editor_cx.set_state("«ˇfoo»\nFOO\nFoo\nfoo");
    editor_cx.update_editor(|e, window, cx| {
        e.select_next(&Default::default(), window, cx).unwrap();
    });
    editor_cx.assert_editor_state("«ˇfoo»\nFOO\nFoo\n«ˇfoo»");

    // Update the editor's search settings, disabling case sensitivity, to
    // check that the value is respected.
    let mut search_settings = SearchSettings::default();
    search_settings.case_sensitive = false;
    update_search_settings(search_settings, cx);
    editor_cx.set_state("«ˇfoo»\nFOO\nFoo\nfoo");
    editor_cx.update_editor(|e, window, cx| {
        e.select_next(&Default::default(), window, cx).unwrap();
    });
    editor_cx.assert_editor_state("«ˇfoo»\n«ˇFOO»\nFoo\nfoo");
}

#[gpui::test]
async fn test_regex_search_does_not_highlight_non_matching_occurrences(cx: &mut TestAppContext) {
    init_globals(cx);
    let buffer = cx.new(|cx| {
        Buffer::local(
            "something is at the top\nsomething is behind something\nsomething is at the bottom\n",
            cx,
        )
    });
    let cx = cx.add_empty_window();
    let editor =
        cx.new_window_entity(|window, cx| Editor::for_buffer(buffer.clone(), None, window, cx));
    let search_bar = cx.new_window_entity(|window, cx| {
        let mut search_bar = BufferSearchBar::new(None, window, cx);
        search_bar.set_active_pane_item(Some(&editor), window, cx);
        search_bar.show(window, cx);
        search_bar
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.toggle_search_option(SearchOptions::REGEX, window, cx);
    });

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("^something", None, true, window, cx)
        })
        .await
        .unwrap();

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_next_match(&SelectNextMatch, window, cx);
    });

    // Advance past the debounce so the selection occurrence highlight would
    // have fired if it were not suppressed by the active buffer search.
    cx.executor()
        .advance_clock(SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT + Duration::from_millis(1));
    cx.run_until_parked();

    editor.update(cx, |editor, cx| {
        assert!(
            !editor.has_background_highlights(HighlightKey::SelectedTextHighlight),
            "selection occurrence highlights must be suppressed during buffer search"
        );
        assert_eq!(
            editor.search_background_highlights(cx).len(),
            3,
            "expected exactly 3 search highlights (one per line start)"
        );
    });

    // Manually select "something" — this should restore occurrence highlights
    // because it clears the search-navigation flag.
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(0, 9)])
        });
    });

    cx.executor()
        .advance_clock(SELECTION_HIGHLIGHT_DEBOUNCE_TIMEOUT + Duration::from_millis(1));
    cx.run_until_parked();

    editor.update(cx, |editor, _cx| {
        assert!(
            editor.has_background_highlights(HighlightKey::SelectedTextHighlight),
            "selection occurrence highlights must be restored after a manual selection"
        );
    });
}

#[gpui::test]
async fn test_replace_with_non_ascii_characters(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_test(cx);

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("￥100 ￥200 ￥100", window, cx)
    });

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("￥", None, true, window, cx)
        })
        .await
        .unwrap();

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.replacement_editor.update(cx, |editor, cx| {
            editor.set_text("\\n", window, cx);
        });
        search_bar.replace_all(&ReplaceAll, window, cx)
    });

    assert_eq!(
        editor.read_with(cx, |this, cx| this.text(cx)),
        "\\n100 \\n200 \\n100"
    );
}
