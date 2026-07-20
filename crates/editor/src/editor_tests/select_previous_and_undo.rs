use super::*;

#[gpui::test]
async fn test_select_all_matches_does_not_scroll(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let large_body_1 = "\nd".repeat(200);
    let large_body_2 = "\ne".repeat(200);

    cx.set_state(&format!(
        "abc\nabc{large_body_1} «ˇa»bc{large_body_2}\nefabc\nabc"
    ));
    let initial_scroll_position = cx.update_editor(|editor, _, cx| {
        let scroll_position = editor.scroll_position(cx);
        assert!(scroll_position.y > 0.0, "Initial selection is between two large bodies and should have the editor scrolled to it");
        scroll_position
    });

    cx.update_editor(|editor, window, cx| editor.select_all_matches(&SelectAllMatches, window, cx))
        .unwrap();
    cx.assert_editor_state(&format!(
        "«ˇa»bc\n«ˇa»bc{large_body_1} «ˇa»bc{large_body_2}\nef«ˇa»bc\n«ˇa»bc"
    ));
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            editor.scroll_position(cx),
            initial_scroll_position,
            "Scroll position should not change after selecting all matches"
        )
    });

    // Simulate typing while the selections are active, as that is where the
    // editor would attempt to actually scroll to the newest selection, which
    // should have been set as the original selection to avoid scrolling to the
    // last match.
    cx.simulate_keystroke("x");
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            editor.scroll_position(cx),
            initial_scroll_position,
            "Scroll position should not change after editing all matches"
        )
    });

    cx.set_state(&format!(
        "abc\nabc{large_body_1} «aˇ»bc{large_body_2}\nefabc\nabc"
    ));
    let initial_scroll_position = cx.update_editor(|editor, _, cx| {
        let scroll_position = editor.scroll_position(cx);
        assert!(scroll_position.y > 0.0, "Initial selection is between two large bodies and should have the editor scrolled to it");
        scroll_position
    });

    cx.update_editor(|editor, window, cx| editor.select_all_matches(&SelectAllMatches, window, cx))
        .unwrap();
    cx.assert_editor_state(&format!(
        "«aˇ»bc\n«aˇ»bc{large_body_1} «aˇ»bc{large_body_2}\nef«aˇ»bc\n«aˇ»bc"
    ));
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            editor.scroll_position(cx),
            initial_scroll_position,
            "Scroll position should not change after selecting all matches"
        )
    });

    cx.simulate_keystroke("x");
    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            editor.scroll_position(cx),
            initial_scroll_position,
            "Scroll position should not change after editing all matches"
        )
    });
}

#[gpui::test]
async fn test_undo_format_scrolls_to_last_edit_pos(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_formatting_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        line 1
        line 2
        linˇe 3
        line 4
        line 5
    "});

    // Make an edit
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("X", window, cx);
    });

    // Move cursor to a different position
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(4, 2)..Point::new(4, 2)]);
        });
    });

    cx.assert_editor_state(indoc! {"
        line 1
        line 2
        linXe 3
        line 4
        liˇne 5
    "});

    cx.lsp
        .set_request_handler::<lsp::request::Formatting, _, _>(move |_, _| async move {
            Ok(Some(vec![lsp::TextEdit::new(
                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
                "PREFIX ".to_string(),
            )]))
        });

    cx.update_editor(|editor, window, cx| editor.format(&Default::default(), window, cx))
        .unwrap()
        .await
        .unwrap();

    cx.assert_editor_state(indoc! {"
        PREFIX line 1
        line 2
        linXe 3
        line 4
        liˇne 5
    "});

    // Undo formatting
    cx.update_editor(|editor, window, cx| {
        editor.undo(&Default::default(), window, cx);
    });

    // Verify cursor moved back to position after edit
    cx.assert_editor_state(indoc! {"
        line 1
        line 2
        linXˇe 3
        line 4
        line 5
    "});
}

#[gpui::test]
async fn test_undo_edit_prediction_scrolls_to_edit_pos(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let provider = cx.new(|_| FakeEditPredictionDelegate::default());
    cx.update_editor(|editor, window, cx| {
        editor.set_edit_prediction_provider(Some(provider.clone()), window, cx);
    });

    cx.set_state(indoc! {"
        line 1
        line 2
        linˇe 3
        line 4
        line 5
        line 6
        line 7
        line 8
        line 9
        line 10
    "});

    let snapshot = cx.buffer_snapshot();
    let edit_position = snapshot.anchor_after(Point::new(2, 4));

    cx.update(|_, cx| {
        provider.update(cx, |provider, _| {
            provider.set_edit_prediction(Some(edit_prediction_types::EditPrediction::Local {
                id: None,
                edits: vec![(edit_position..edit_position, "X".into())],
                cursor_position: None,
                edit_preview: None,
            }))
        })
    });

    cx.update_editor(|editor, window, cx| editor.update_visible_edit_prediction(window, cx));
    cx.update_editor(|editor, window, cx| {
        editor.accept_edit_prediction(&crate::AcceptEditPrediction, window, cx)
    });

    cx.assert_editor_state(indoc! {"
        line 1
        line 2
        lineXˇ 3
        line 4
        line 5
        line 6
        line 7
        line 8
        line 9
        line 10
    "});

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(9, 2)..Point::new(9, 2)]);
        });
    });

    cx.assert_editor_state(indoc! {"
        line 1
        line 2
        lineX 3
        line 4
        line 5
        line 6
        line 7
        line 8
        line 9
        liˇne 10
    "});

    cx.update_editor(|editor, window, cx| {
        editor.undo(&Default::default(), window, cx);
    });

    cx.assert_editor_state(indoc! {"
        line 1
        line 2
        lineˇ 3
        line 4
        line 5
        line 6
        line 7
        line 8
        line 9
        line 10
    "});
}

#[gpui::test]
async fn test_select_next_with_multiple_carets(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(
        r#"let foo = 2;
lˇet foo = 2;
let fooˇ = 2;
let foo = 2;
let foo = ˇ2;"#,
    );

    cx.update_editor(|e, window, cx| e.select_next(&SelectNext::default(), window, cx))
        .unwrap();
    cx.assert_editor_state(
        r#"let foo = 2;
«letˇ» foo = 2;
let «fooˇ» = 2;
let foo = 2;
let foo = «2ˇ»;"#,
    );

    // noop for multiple selections with different contents
    cx.update_editor(|e, window, cx| e.select_next(&SelectNext::default(), window, cx))
        .unwrap();
    cx.assert_editor_state(
        r#"let foo = 2;
«letˇ» foo = 2;
let «fooˇ» = 2;
let foo = 2;
let foo = «2ˇ»;"#,
    );

    // Test last selection direction should be preserved
    cx.set_state(
        r#"let foo = 2;
let foo = 2;
let «fooˇ» = 2;
let «ˇfoo» = 2;
let foo = 2;"#,
    );

    cx.update_editor(|e, window, cx| e.select_next(&SelectNext::default(), window, cx))
        .unwrap();
    cx.assert_editor_state(
        r#"let foo = 2;
let foo = 2;
let «fooˇ» = 2;
let «ˇfoo» = 2;
let «ˇfoo» = 2;"#,
    );
}

#[gpui::test]
async fn test_select_previous_multibuffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx =
        EditorTestContext::new_multibuffer(cx, ["aaa\n«bbb\nccc»\nddd", "aaa\n«bbb\nccc»\nddd"]);

    cx.assert_editor_state(indoc! {"
        ˇbbb
        ccc
        bbb
        ccc"});
    cx.dispatch_action(SelectPrevious::default());
    cx.assert_editor_state(indoc! {"
                «bbbˇ»
                ccc
                bbb
                ccc"});
    cx.dispatch_action(SelectPrevious::default());
    cx.assert_editor_state(indoc! {"
                «bbbˇ»
                ccc
                «bbbˇ»
                ccc"});
}

#[gpui::test]
async fn test_select_previous_with_single_caret(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state("abc\nˇabc abc\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("abc\n«abcˇ» abc\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«abcˇ»\n«abcˇ» abc\ndefabc\nabc");

    cx.update_editor(|editor, window, cx| editor.undo_selection(&UndoSelection, window, cx));
    cx.assert_editor_state("abc\n«abcˇ» abc\ndefabc\nabc");

    cx.update_editor(|editor, window, cx| editor.redo_selection(&RedoSelection, window, cx));
    cx.assert_editor_state("«abcˇ»\n«abcˇ» abc\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«abcˇ»\n«abcˇ» abc\ndefabc\n«abcˇ»");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«abcˇ»\n«abcˇ» «abcˇ»\ndefabc\n«abcˇ»");
}

#[gpui::test]
async fn test_select_previous_empty_buffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state("aˇ");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«aˇ»");
    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«aˇ»");
}

#[gpui::test]
async fn test_select_previous_with_multiple_carets(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(
        r#"let foo = 2;
lˇet foo = 2;
let fooˇ = 2;
let foo = 2;
let foo = ˇ2;"#,
    );

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state(
        r#"let foo = 2;
«letˇ» foo = 2;
let «fooˇ» = 2;
let foo = 2;
let foo = «2ˇ»;"#,
    );

    // noop for multiple selections with different contents
    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state(
        r#"let foo = 2;
«letˇ» foo = 2;
let «fooˇ» = 2;
let foo = 2;
let foo = «2ˇ»;"#,
    );
}

#[gpui::test]
async fn test_select_previous_with_single_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    // Enable case sensitive search.
    update_test_editor_settings(&mut cx, &|settings| {
        let mut search_settings = SearchSettingsContent::default();
        search_settings.case_sensitive = Some(true);
        settings.search = Some(search_settings);
    });

    cx.set_state("abc\n«ˇabc» abc\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    // selection direction is preserved
    cx.assert_editor_state("«ˇabc»\n«ˇabc» abc\ndefabc\nabc");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«ˇabc»\n«ˇabc» abc\ndefabc\n«ˇabc»");

    cx.update_editor(|editor, window, cx| editor.undo_selection(&UndoSelection, window, cx));
    cx.assert_editor_state("«ˇabc»\n«ˇabc» abc\ndefabc\nabc");

    cx.update_editor(|editor, window, cx| editor.redo_selection(&RedoSelection, window, cx));
    cx.assert_editor_state("«ˇabc»\n«ˇabc» abc\ndefabc\n«ˇabc»");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«ˇabc»\n«ˇabc» abc\ndef«ˇabc»\n«ˇabc»");

    cx.update_editor(|e, window, cx| e.select_previous(&SelectPrevious::default(), window, cx))
        .unwrap();
    cx.assert_editor_state("«ˇabc»\n«ˇabc» «ˇabc»\ndef«ˇabc»\n«ˇabc»");

    // Test case sensitivity
    cx.set_state("foo\nFOO\nFoo\n«ˇfoo»");
    cx.update_editor(|e, window, cx| {
        e.select_previous(&SelectPrevious::default(), window, cx)
            .unwrap();
        e.select_previous(&SelectPrevious::default(), window, cx)
            .unwrap();
    });
    cx.assert_editor_state("«ˇfoo»\nFOO\nFoo\n«ˇfoo»");

    // Disable case sensitive search.
    update_test_editor_settings(&mut cx, &|settings| {
        let mut search_settings = SearchSettingsContent::default();
        search_settings.case_sensitive = Some(false);
        settings.search = Some(search_settings);
    });

    cx.set_state("foo\nFOO\n«ˇFoo»");
    cx.update_editor(|e, window, cx| {
        e.select_previous(&SelectPrevious::default(), window, cx)
            .unwrap();
        e.select_previous(&SelectPrevious::default(), window, cx)
            .unwrap();
    });
    cx.assert_editor_state("«ˇfoo»\n«ˇFOO»\n«ˇFoo»");
}
