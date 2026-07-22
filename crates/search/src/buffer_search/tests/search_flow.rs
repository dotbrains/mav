use super::*;

#[gpui::test]
async fn test_search_simple(cx: &mut TestAppContext) {
    let (editor, search_bar, cx) = init_test(cx);
    let display_points_of = |background_highlights: Vec<(Range<DisplayPoint>, Hsla)>| {
        background_highlights
            .into_iter()
            .map(|(range, _)| range)
            .collect::<Vec<_>>()
    };
    // Search for a string that appears with different casing.
    // By default, search is case-insensitive.
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("us", None, true, window, cx)
        })
        .await
        .unwrap();
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            display_points_of(editor.all_text_background_highlights(window, cx)),
            &[
                DisplayPoint::new(DisplayRow(2), 17)..DisplayPoint::new(DisplayRow(2), 19),
                DisplayPoint::new(DisplayRow(2), 43)..DisplayPoint::new(DisplayRow(2), 45),
            ]
        );
    });

    // Switch to a case sensitive search.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.toggle_search_option(SearchOptions::CASE_SENSITIVE, window, cx);
    });
    let mut editor_notifications = cx.notifications(&editor);
    editor_notifications.next().await;
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            display_points_of(editor.all_text_background_highlights(window, cx)),
            &[DisplayPoint::new(DisplayRow(2), 43)..DisplayPoint::new(DisplayRow(2), 45),]
        );
    });

    // Search for a string that appears both as a whole word and
    // within other words. By default, all results are found.
    search_bar
        .update_in(cx, |search_bar, window, cx| {
            search_bar.search("or", None, true, window, cx)
        })
        .await
        .unwrap();
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            display_points_of(editor.all_text_background_highlights(window, cx)),
            &[
                DisplayPoint::new(DisplayRow(0), 24)..DisplayPoint::new(DisplayRow(0), 26),
                DisplayPoint::new(DisplayRow(0), 41)..DisplayPoint::new(DisplayRow(0), 43),
                DisplayPoint::new(DisplayRow(2), 71)..DisplayPoint::new(DisplayRow(2), 73),
                DisplayPoint::new(DisplayRow(3), 1)..DisplayPoint::new(DisplayRow(3), 3),
                DisplayPoint::new(DisplayRow(3), 11)..DisplayPoint::new(DisplayRow(3), 13),
                DisplayPoint::new(DisplayRow(3), 56)..DisplayPoint::new(DisplayRow(3), 58),
                DisplayPoint::new(DisplayRow(3), 60)..DisplayPoint::new(DisplayRow(3), 62),
            ]
        );
    });

    // Switch to a whole word search.
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.toggle_search_option(SearchOptions::WHOLE_WORD, window, cx);
    });
    let mut editor_notifications = cx.notifications(&editor);
    editor_notifications.next().await;
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            display_points_of(editor.all_text_background_highlights(window, cx)),
            &[
                DisplayPoint::new(DisplayRow(0), 41)..DisplayPoint::new(DisplayRow(0), 43),
                DisplayPoint::new(DisplayRow(3), 11)..DisplayPoint::new(DisplayRow(3), 13),
                DisplayPoint::new(DisplayRow(3), 56)..DisplayPoint::new(DisplayRow(3), 58),
            ]
        );
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)
            ])
        });
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        assert_eq!(search_bar.active_match_index, Some(0));
        search_bar.select_next_match(&SelectNextMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(0), 41)..DisplayPoint::new(DisplayRow(0), 43)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(0));
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_next_match(&SelectNextMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(3), 11)..DisplayPoint::new(DisplayRow(3), 13)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(1));
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_next_match(&SelectNextMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(3), 56)..DisplayPoint::new(DisplayRow(3), 58)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(2));
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_next_match(&SelectNextMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(0), 41)..DisplayPoint::new(DisplayRow(0), 43)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(0));
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_prev_match(&SelectPreviousMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(3), 56)..DisplayPoint::new(DisplayRow(3), 58)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(2));
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_prev_match(&SelectPreviousMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(3), 11)..DisplayPoint::new(DisplayRow(3), 13)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(1));
    });

    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_prev_match(&SelectPreviousMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(0), 41)..DisplayPoint::new(DisplayRow(0), 43)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(0));
    });

    // Park the cursor in between matches and ensure that going to the previous match
    // selects the closest match to the left of the cursor.
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0)
            ])
        });
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_prev_match(&SelectPreviousMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(0), 41)..DisplayPoint::new(DisplayRow(0), 43)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(0));
    });

    // Park the cursor in between matches and ensure that going to the next match
    // selects the closest match to the right of the cursor.
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0)
            ])
        });
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_next_match(&SelectNextMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(3), 11)..DisplayPoint::new(DisplayRow(3), 13)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(1));
    });

    // Park the cursor after the last match and ensure that going to the previous match
    // selects the last match.
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 60)..DisplayPoint::new(DisplayRow(3), 60)
            ])
        });
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_prev_match(&SelectPreviousMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(3), 56)..DisplayPoint::new(DisplayRow(3), 58)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(2));
    });

    // Park the cursor after the last match and ensure that going to the next match
    // wraps around and selects the first match.
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 60)..DisplayPoint::new(DisplayRow(3), 60)
            ])
        });
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        search_bar.select_next_match(&SelectNextMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(0), 41)..DisplayPoint::new(DisplayRow(0), 43)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(0));
    });

    // Park the cursor before the first match and ensure that going to the previous match
    // wraps around and selects the last match.
    editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)
            ])
        });
    });
    search_bar.update_in(cx, |search_bar, window, cx| {
        assert_eq!(search_bar.active_match_index, Some(0));
        search_bar.select_prev_match(&SelectPreviousMatch, window, cx);
        assert_eq!(
            editor.update(cx, |editor, cx| editor
                .selections
                .display_ranges(&editor.display_snapshot(cx))),
            [DisplayPoint::new(DisplayRow(3), 56)..DisplayPoint::new(DisplayRow(3), 58)]
        );
    });
    search_bar.read_with(cx, |search_bar, _| {
        assert_eq!(search_bar.active_match_index, Some(2));
    });
}
