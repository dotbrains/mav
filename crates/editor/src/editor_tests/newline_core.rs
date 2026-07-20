use super::*;

#[gpui::test]
fn test_newline(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaa\n    bbbb\n", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 2),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(1), 2),
                DisplayPoint::new(DisplayRow(1), 6)..DisplayPoint::new(DisplayRow(1), 6),
            ])
        });

        editor.newline(&Newline, window, cx);
        assert_eq!(editor.text(cx), "aa\naa\n  \n    bb\n    bb\n");
    });
}

#[gpui::test]
fn test_newline_trailing_whitespace(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.auto_indent = Some(settings::AutoIndentMode::PreserveIndent);
    });

    let buffer = cx.update(|cx| MultiBuffer::build_simple("    hello\n    world\n", cx));
    let editor = cx.add_window(|window, cx| build_editor(buffer.clone(), window, cx));

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(0), 9)..DisplayPoint::new(DisplayRow(0), 9)
                ])
            });

            editor.newline(&Newline, window, cx);
            assert_eq!(editor.text(cx), "    hello\n    \n    world\n");

            editor.newline(&Newline, window, cx);
            assert_eq!(editor.text(cx), "    hello\n\n    \n    world\n");
        })
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        let start = MultiBufferOffset(0);
        let end = buffer.len(cx);
        buffer.edit([(start..end, "    hello\n    world\n")], None, cx);
    });

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(0), 7)..DisplayPoint::new(DisplayRow(0), 7)
                ])
            });

            editor.newline(&Newline, window, cx);
            assert_eq!(editor.text(cx), "    hel\n    lo\n    world\n");

            editor.newline(&Newline, window, cx);
            assert_eq!(editor.text(cx), "    hel\n\n    lo\n    world\n");
        })
        .unwrap();

    update_test_language_settings(cx, &|settings| {
        settings.defaults.tab_size = NonZeroU32::new(4);
        settings.defaults.hard_tabs = Some(true);
    });

    buffer.update(cx, |buffer, cx| {
        let start = MultiBufferOffset(0);
        let end = buffer.len(cx);
        buffer.edit([(start..end, "\thello\n\tworld\n")], None, cx);
    });

    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(0), 9)..DisplayPoint::new(DisplayRow(0), 9)
                ])
            });

            editor.newline(&Newline, window, cx);
            assert_eq!(editor.text(cx), "\thello\n\t\n\tworld\n");

            editor.newline(&Newline, window, cx);
            assert_eq!(editor.text(cx), "\thello\n\n\t\n\tworld\n");
        })
        .unwrap();
}

#[gpui::test]
async fn test_newline_yaml(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let yaml_language = languages::language("yaml", tree_sitter_yaml::LANGUAGE.into());
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(yaml_language), cx));

    // Object (between 2 fields)
    cx.set_state(indoc! {"
    test:ˇ
    hello: bye"});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
    test:
        ˇ
    hello: bye"});

    // Object (first and single line)
    cx.set_state(indoc! {"
    test:ˇ"});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
    test:
        ˇ"});

    // Array with objects (after first element)
    cx.set_state(indoc! {"
    test:
        - foo: barˇ"});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
    test:
        - foo: bar
        ˇ"});

    // Array with objects and comment
    cx.set_state(indoc! {"
    test:
        - foo: bar
        - bar: # testˇ"});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
    test:
        - foo: bar
        - bar: # test
            ˇ"});

    // Array with objects (after second element)
    cx.set_state(indoc! {"
    test:
        - foo: bar
        - bar: fooˇ"});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
    test:
        - foo: bar
        - bar: foo
        ˇ"});

    // Array with strings (after first element)
    cx.set_state(indoc! {"
    test:
        - fooˇ"});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
    test:
        - foo
        ˇ"});
}

#[gpui::test]
fn test_newline_with_old_selections(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(
            "
                a
                b(
                    X
                )
                c(
                    X
                )
            "
            .unindent()
            .as_str(),
            cx,
        );
        let mut editor = build_editor(buffer, window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                Point::new(2, 4)..Point::new(2, 5),
                Point::new(5, 4)..Point::new(5, 5),
            ])
        });
        editor
    });

    _ = editor.update(cx, |editor, window, cx| {
        // Edit the buffer directly, deleting ranges surrounding the editor's selections
        editor.buffer.update(cx, |buffer, cx| {
            buffer.edit(
                [
                    (Point::new(1, 2)..Point::new(3, 0), ""),
                    (Point::new(4, 2)..Point::new(6, 0), ""),
                ],
                None,
                cx,
            );
            assert_eq!(
                buffer.read(cx).text(),
                "
                    a
                    b()
                    c()
                "
                .unindent()
            );
        });
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            &[
                Point::new(1, 2)..Point::new(1, 2),
                Point::new(2, 2)..Point::new(2, 2),
            ],
        );

        editor.newline(&Newline, window, cx);
        assert_eq!(
            editor.text(cx),
            "
                a
                b(
                )
                c(
                )
            "
            .unindent()
        );

        // The selections are moved after the inserted newlines
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            &[
                Point::new(2, 0)..Point::new(2, 0),
                Point::new(4, 0)..Point::new(4, 0),
            ],
        );
    });
}

#[gpui::test]
async fn test_newline_above(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });

    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query(r#"(_ "(" ")" @end) @indent"#)
        .unwrap(),
    );

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
    cx.set_state(indoc! {"
        const a: ˇA = (
            (ˇ
                «const_functionˇ»(ˇ),
                so«mˇ»et«hˇ»ing_ˇelse,ˇ
            )ˇ
        ˇ);ˇ
    "});

    cx.update_editor(|e, window, cx| e.newline_above(&NewlineAbove, window, cx));
    cx.assert_editor_state(indoc! {"
        ˇ
        const a: A = (
            ˇ
            (
                ˇ
                ˇ
                const_function(),
                ˇ
                ˇ
                ˇ
                ˇ
                something_else,
                ˇ
            )
            ˇ
            ˇ
        );
    "});
}

#[gpui::test]
async fn test_newline_below(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.tab_size = NonZeroU32::new(4)
    });

    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_indents_query(r#"(_ "(" ")" @end) @indent"#)
        .unwrap(),
    );

    let mut cx = EditorTestContext::new(cx).await;
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));
    cx.set_state(indoc! {"
        const a: ˇA = (
            (ˇ
                «const_functionˇ»(ˇ),
                so«mˇ»et«hˇ»ing_ˇelse,ˇ
            )ˇ
        ˇ);ˇ
    "});

    cx.update_editor(|e, window, cx| e.newline_below(&NewlineBelow, window, cx));
    cx.assert_editor_state(indoc! {"
        const a: A = (
            ˇ
            (
                ˇ
                const_function(),
                ˇ
                ˇ
                something_else,
                ˇ
                ˇ
                ˇ
                ˇ
            )
            ˇ
        );
        ˇ
        ˇ
    "});
}

#[gpui::test]
fn test_newline_respects_read_only(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaa\nbbbb\n", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.set_read_only(true);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 2)
            ])
        });

        editor.newline(&Newline, window, cx);
        assert_eq!(
            editor.text(cx),
            "aaaa\nbbbb\n",
            "newline should not modify a read-only editor"
        );

        editor.newline_above(&NewlineAbove, window, cx);
        assert_eq!(
            editor.text(cx),
            "aaaa\nbbbb\n",
            "newline_above should not modify a read-only editor"
        );

        editor.newline_below(&NewlineBelow, window, cx);
        assert_eq!(
            editor.text(cx),
            "aaaa\nbbbb\n",
            "newline_below should not modify a read-only editor"
        );
    });
}

#[gpui::test]
async fn test_newline_below_with_cursor_on_deleted_hunk(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state("aaa\nbbb\ncˇcc");
    cx.set_head_text("aaa\nXXX\nbbb\nccc");
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.expand_all_diff_hunks(&Default::default(), window, cx);
    });
    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
                DisplayPoint::new(DisplayRow(3), 3)..DisplayPoint::new(DisplayRow(3), 3),
            ]);
        });
    });

    cx.update_editor(|editor, window, cx| {
        editor.newline_below(&NewlineBelow, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(cx.buffer(|buffer, _| buffer.text()), "aaa\nbbb\nccc\n");

    let cursors = cx.update_editor(|editor, window, cx| {
        let display_snapshot = editor.snapshot(window, cx).display_snapshot;
        editor
            .selections
            .all_display(&display_snapshot)
            .iter()
            .map(|selection| selection.head())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        cursors,
        vec![
            DisplayPoint::new(DisplayRow(1), 0),
            DisplayPoint::new(DisplayRow(4), 0),
        ],
    );
}
