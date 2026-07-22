use super::*;

#[gpui::test]
async fn test_search_select_all_matches(cx: &mut TestAppContext) {
    init_globals(cx);
    let buffer_text = r#"
        A regular expression (shortened as regex or regexp;[1] also referred to as
        rational expression[2][3]) is a sequence of characters that specifies a search
        pattern in text. Usually such patterns are used by string-searching algorithms
        for "find" or "find and replace" operations on strings, or for input validation.
        "#
    .unindent();
    let expected_query_matches_count = buffer_text
        .chars()
        .filter(|c| c.eq_ignore_ascii_case(&'a'))
        .count();
    assert!(
        expected_query_matches_count > 1,
        "Should pick a query with multiple results"
    );
    let buffer = cx.new(|cx| Buffer::local(buffer_text, cx));
    let window = cx.add_window(|_, _| gpui::Empty);

    let editor = window.build_entity(cx, |window, cx| {
        Editor::for_buffer(buffer.clone(), None, window, cx)
    });

    let search_bar = window.build_entity(cx, |window, cx| {
        let mut search_bar = BufferSearchBar::new(None, window, cx);
        search_bar.set_active_pane_item(Some(&editor), window, cx);
        search_bar.show(window, cx);
        search_bar
    });

    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.search("a", None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
    let initial_selections = window
            .update(cx, |_, window, cx| {
                search_bar.update(cx, |search_bar, cx| {
                    let handle = search_bar.query_editor.focus_handle(cx);
                    window.focus(&handle, cx);
                    search_bar.activate_current_match(window, cx);
                });
                assert!(
                    !editor.read(cx).is_focused(window),
                    "Initially, the editor should not be focused"
                );
                let initial_selections = editor.update(cx, |editor, cx| {
                    let initial_selections = editor.selections.display_ranges(&editor.display_snapshot(cx));
                    assert_eq!(
                        initial_selections.len(), 1,
                        "Expected to have only one selection before adding carets to all matches, but got: {initial_selections:?}",
                    );
                    initial_selections
                });
                search_bar.update(cx, |search_bar, cx| {
                    assert_eq!(search_bar.active_match_index, Some(0));
                    let handle = search_bar.query_editor.focus_handle(cx);
                    window.focus(&handle, cx);
                    search_bar.select_all_matches(&SelectAllMatches, window, cx);
                });
                assert!(
                    editor.read(cx).is_focused(window),
                    "Should focus editor after successful SelectAllMatches"
                );
                search_bar.update(cx, |search_bar, cx| {
                    let all_selections =
                        editor.update(cx, |editor, cx| editor.selections.display_ranges(&editor.display_snapshot(cx)));
                    assert_eq!(
                        all_selections.len(),
                        expected_query_matches_count,
                        "Should select all `a` characters in the buffer, but got: {all_selections:?}"
                    );
                    assert_eq!(
                        search_bar.active_match_index,
                        Some(0),
                        "Match index should not change after selecting all matches"
                    );
                });

                search_bar.update(cx, |this, cx| this.select_next_match(&SelectNextMatch, window, cx));
                initial_selections
            }).unwrap();

    window
        .update(cx, |_, window, cx| {
            assert!(
                editor.read(cx).is_focused(window),
                "Should still have editor focused after SelectNextMatch"
            );
            search_bar.update(cx, |search_bar, cx| {
                let all_selections = editor.update(cx, |editor, cx| {
                    editor
                        .selections
                        .display_ranges(&editor.display_snapshot(cx))
                });
                assert_eq!(
                    all_selections.len(),
                    1,
                    "On next match, should deselect items and select the next match"
                );
                assert_ne!(
                    all_selections, initial_selections,
                    "Next match should be different from the first selection"
                );
                assert_eq!(
                    search_bar.active_match_index,
                    Some(1),
                    "Match index should be updated to the next one"
                );
                let handle = search_bar.query_editor.focus_handle(cx);
                window.focus(&handle, cx);
                search_bar.select_all_matches(&SelectAllMatches, window, cx);
            });
        })
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            assert!(
                editor.read(cx).is_focused(window),
                "Should focus editor after successful SelectAllMatches"
            );
            search_bar.update(cx, |search_bar, cx| {
                let all_selections = editor.update(cx, |editor, cx| {
                    editor
                        .selections
                        .display_ranges(&editor.display_snapshot(cx))
                });
                assert_eq!(
                    all_selections.len(),
                    expected_query_matches_count,
                    "Should select all `a` characters in the buffer, but got: {all_selections:?}"
                );
                assert_eq!(
                    search_bar.active_match_index,
                    Some(1),
                    "Match index should not change after selecting all matches"
                );
            });
            search_bar.update(cx, |search_bar, cx| {
                search_bar.select_prev_match(&SelectPreviousMatch, window, cx);
            });
        })
        .unwrap();
    let last_match_selections = window
        .update(cx, |_, window, cx| {
            assert!(
                editor.read(cx).is_focused(window),
                "Should still have editor focused after SelectPreviousMatch"
            );

            search_bar.update(cx, |search_bar, cx| {
                let all_selections = editor.update(cx, |editor, cx| {
                    editor
                        .selections
                        .display_ranges(&editor.display_snapshot(cx))
                });
                assert_eq!(
                    all_selections.len(),
                    1,
                    "On previous match, should deselect items and select the previous item"
                );
                assert_eq!(
                    all_selections, initial_selections,
                    "Previous match should be the same as the first selection"
                );
                assert_eq!(
                    search_bar.active_match_index,
                    Some(0),
                    "Match index should be updated to the previous one"
                );
                all_selections
            })
        })
        .unwrap();

    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                let handle = search_bar.query_editor.focus_handle(cx);
                window.focus(&handle, cx);
                search_bar.search("abas_nonexistent_match", None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            search_bar.update(cx, |search_bar, cx| {
                search_bar.select_all_matches(&SelectAllMatches, window, cx);
            });
            assert!(
                editor.update(cx, |this, _cx| !this.is_focused(window)),
                "Should not switch focus to editor if SelectAllMatches does not find any matches"
            );
            search_bar.update(cx, |search_bar, cx| {
                let all_selections = editor.update(cx, |editor, cx| {
                    editor
                        .selections
                        .display_ranges(&editor.display_snapshot(cx))
                });
                assert_eq!(
                    all_selections, last_match_selections,
                    "Should not select anything new if there are no matches"
                );
                assert!(
                    search_bar.active_match_index.is_none(),
                    "For no matches, there should be no active match index"
                );
            });
        })
        .unwrap();
}

#[perf]
#[gpui::test]
async fn test_search_query_with_match_whole_word(cx: &mut TestAppContext) {
    init_globals(cx);
    let buffer_text = r#"
        self.buffer.update(cx, |buffer, cx| {
            buffer.edit(
                edits,
                Some(AutoindentMode::Block {
                    original_indent_columns,
                }),
                cx,
            )
        });

        this.buffer.update(cx, |buffer, cx| {
            buffer.edit([(end_of_line..start_of_next_line, replace)], None, cx)
        });
        "#
    .unindent();
    let buffer = cx.new(|cx| Buffer::local(buffer_text, cx));
    let cx = cx.add_empty_window();

    let editor =
        cx.new_window_entity(|window, cx| Editor::for_buffer(buffer.clone(), None, window, cx));

    let search_bar = cx.new_window_entity(|window, cx| {
        let mut search_bar = BufferSearchBar::new(None, window, cx);
        search_bar.set_active_pane_item(Some(&editor), window, cx);
        search_bar.show(window, cx);
        search_bar
    });

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search(
                "edit\\(",
                Some(SearchOptions::WHOLE_WORD | SearchOptions::REGEX),
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_all_matches(&SelectAllMatches, window, cx);
    });
    search_bar.update(cx, |_, cx| {
        let all_selections = editor.update(cx, |editor, cx| {
            editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))
        });
        assert_eq!(
            all_selections.len(),
            2,
            "Should select all `edit(` in the buffer, but got: {all_selections:?}"
        );
    });

    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search(
                "edit(",
                Some(SearchOptions::WHOLE_WORD | SearchOptions::CASE_SENSITIVE),
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_all_matches(&SelectAllMatches, window, cx);
    });
    search_bar.update(cx, |_, cx| {
        let all_selections = editor.update(cx, |editor, cx| {
            editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))
        });
        assert_eq!(
            all_selections.len(),
            2,
            "Should select all `edit(` in the buffer, but got: {all_selections:?}"
        );
    });
}
