use super::*;

#[gpui::test]
async fn test_delete_to_bracket(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(
        Language::new(
            LanguageConfig {
                brackets: BracketPairConfig {
                    pairs: vec![
                        BracketPair {
                            start: "\"".to_string(),
                            end: "\"".to_string(),
                            close: true,
                            surround: true,
                            newline: false,
                        },
                        BracketPair {
                            start: "(".to_string(),
                            end: ")".to_string(),
                            close: true,
                            surround: true,
                            newline: true,
                        },
                    ],
                    ..BracketPairConfig::default()
                },
                ..LanguageConfig::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_brackets_query(
            r#"
                ("(" @open ")" @close)
                ("\"" @open "\"" @close)
            "#,
        )
        .unwrap(),
    );

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    cx.set_state(r#"macro!("// ˇCOMMENT");"#);
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    // Deletion stops before brackets if asked to not ignore them.
    cx.assert_editor_state(r#"macro!("ˇCOMMENT");"#);
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    // Deletion has to remove a single bracket and then stop again.
    cx.assert_editor_state(r#"macro!(ˇCOMMENT");"#);

    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state(r#"macro!ˇCOMMENT");"#);

    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state(r#"ˇCOMMENT");"#);

    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state(r#"ˇCOMMENT");"#);

    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    // Brackets on the right are not paired anymore, hence deletion does not stop at them
    cx.assert_editor_state(r#"ˇ");"#);

    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state(r#"ˇ"#);

    cx.update_editor(|editor, window, cx| {
        editor.delete_to_next_word_end(
            &DeleteToNextWordEnd {
                ignore_newlines: true,
                ignore_brackets: false,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state(r#"ˇ"#);

    cx.set_state(r#"macro!("// ˇCOMMENT");"#);
    cx.update_editor(|editor, window, cx| {
        editor.delete_to_previous_word_start(
            &DeleteToPreviousWordStart {
                ignore_newlines: true,
                ignore_brackets: true,
            },
            window,
            cx,
        );
    });
    cx.assert_editor_state(r#"macroˇCOMMENT");"#);
}

#[gpui::test]
fn test_delete_to_previous_word_start_or_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("one\n2\nthree\n4", cx);
        build_editor(buffer, window, cx)
    });
    let del_to_prev_word_start = DeleteToPreviousWordStart {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let del_to_prev_word_start_ignore_newlines = DeleteToPreviousWordStart {
        ignore_newlines: true,
        ignore_brackets: false,
    };

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 1)..DisplayPoint::new(DisplayRow(3), 1)
            ])
        });
        editor.delete_to_previous_word_start(&del_to_prev_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "one\n2\nthree\n");
        editor.delete_to_previous_word_start(&del_to_prev_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "one\n2\nthree");
        editor.delete_to_previous_word_start(&del_to_prev_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "one\n2\n");
        editor.delete_to_previous_word_start(&del_to_prev_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "one\n2");
        editor.delete_to_previous_word_start(&del_to_prev_word_start_ignore_newlines, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "one\n");
        editor.delete_to_previous_word_start(&del_to_prev_word_start_ignore_newlines, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "");
    });
}

#[gpui::test]
fn test_delete_to_previous_subword_start_or_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("fooBar\n\nbazQux", cx);
        build_editor(buffer, window, cx)
    });
    let del_to_prev_sub_word_start = DeleteToPreviousSubwordStart {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let del_to_prev_sub_word_start_ignore_newlines = DeleteToPreviousSubwordStart {
        ignore_newlines: true,
        ignore_brackets: false,
    };

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(2), 6)..DisplayPoint::new(DisplayRow(2), 6)
            ])
        });
        editor.delete_to_previous_subword_start(&del_to_prev_sub_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "fooBar\n\nbaz");
        editor.delete_to_previous_subword_start(&del_to_prev_sub_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "fooBar\n\n");
        editor.delete_to_previous_subword_start(&del_to_prev_sub_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "fooBar\n");
        editor.delete_to_previous_subword_start(&del_to_prev_sub_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "fooBar");
        editor.delete_to_previous_subword_start(&del_to_prev_sub_word_start, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "foo");
        editor.delete_to_previous_subword_start(
            &del_to_prev_sub_word_start_ignore_newlines,
            window,
            cx,
        );
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "");
    });
}

#[gpui::test]
fn test_delete_to_next_word_end_or_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("\none\n   two\nthree\n   four", cx);
        build_editor(buffer, window, cx)
    });
    let del_to_next_word_end = DeleteToNextWordEnd {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let del_to_next_word_end_ignore_newlines = DeleteToNextWordEnd {
        ignore_newlines: true,
        ignore_brackets: false,
    };

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)
            ])
        });
        editor.delete_to_next_word_end(&del_to_next_word_end, window, cx);
        assert_eq!(
            editor.buffer.read(cx).read(cx).text(),
            "one\n   two\nthree\n   four"
        );
        editor.delete_to_next_word_end(&del_to_next_word_end, window, cx);
        assert_eq!(
            editor.buffer.read(cx).read(cx).text(),
            "\n   two\nthree\n   four"
        );
        editor.delete_to_next_word_end(&del_to_next_word_end, window, cx);
        assert_eq!(
            editor.buffer.read(cx).read(cx).text(),
            "two\nthree\n   four"
        );
        editor.delete_to_next_word_end(&del_to_next_word_end, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "\nthree\n   four");
        editor.delete_to_next_word_end(&del_to_next_word_end_ignore_newlines, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "\n   four");
        editor.delete_to_next_word_end(&del_to_next_word_end_ignore_newlines, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "four");
        editor.delete_to_next_word_end(&del_to_next_word_end_ignore_newlines, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "");
    });
}

#[gpui::test]
fn test_delete_to_next_subword_end_or_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("\nfooBar\n   bazQux", cx);
        build_editor(buffer, window, cx)
    });
    let del_to_next_subword_end = DeleteToNextSubwordEnd {
        ignore_newlines: false,
        ignore_brackets: false,
    };
    let del_to_next_subword_end_ignore_newlines = DeleteToNextSubwordEnd {
        ignore_newlines: true,
        ignore_brackets: false,
    };

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)
            ])
        });
        // Delete "\n" (empty line)
        editor.delete_to_next_subword_end(&del_to_next_subword_end, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "fooBar\n   bazQux");
        // Delete "foo" (subword boundary)
        editor.delete_to_next_subword_end(&del_to_next_subword_end, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "Bar\n   bazQux");
        // Delete "Bar"
        editor.delete_to_next_subword_end(&del_to_next_subword_end, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "\n   bazQux");
        // Delete "\n   " (newline + leading whitespace)
        editor.delete_to_next_subword_end(&del_to_next_subword_end, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "bazQux");
        // Delete "baz" (subword boundary)
        editor.delete_to_next_subword_end(&del_to_next_subword_end, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "Qux");
        // With ignore_newlines, delete "Qux"
        editor.delete_to_next_subword_end(&del_to_next_subword_end_ignore_newlines, window, cx);
        assert_eq!(editor.buffer.read(cx).read(cx).text(), "");
    });
}
