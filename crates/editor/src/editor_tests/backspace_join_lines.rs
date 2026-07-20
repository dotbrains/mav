use super::*;

#[gpui::test]
async fn test_backspace(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // Basic backspace
    cx.set_state(indoc! {"
        onˇe two three
        fou«rˇ» five six
        seven «ˇeight nine
        »ten
    "});
    cx.update_editor(|e, window, cx| e.backspace(&Backspace, window, cx));
    cx.assert_editor_state(indoc! {"
        oˇe two three
        fouˇ five six
        seven ˇten
    "});

    // Test backspace inside and around indents
    cx.set_state(indoc! {"
        zero
            ˇone
                ˇtwo
            ˇ ˇ ˇ  three
        ˇ  ˇ  four
    "});
    cx.update_editor(|e, window, cx| e.backspace(&Backspace, window, cx));
    cx.assert_editor_state(indoc! {"
        zero
        ˇone
            ˇtwo
        ˇ  threeˇ  four
    "});
}

#[gpui::test]
async fn test_delete(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(indoc! {"
        onˇe two three
        fou«rˇ» five six
        seven «ˇeight nine
        »ten
    "});
    cx.update_editor(|e, window, cx| e.delete(&Delete, window, cx));
    cx.assert_editor_state(indoc! {"
        onˇ two three
        fouˇ five six
        seven ˇten
    "});
}

#[gpui::test]
fn test_delete_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("abc\ndef\nghi\n", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(3), 0)..DisplayPoint::new(DisplayRow(3), 0),
            ])
        });
        editor.delete_line(&DeleteLine, window, cx);
        assert_eq!(editor.display_text(cx), "ghi");
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 1),
            ]
        );
    });

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("abc\ndef\nghi\n", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(0), 1)
            ])
        });
        editor.delete_line(&DeleteLine, window, cx);
        assert_eq!(editor.display_text(cx), "ghi\n");
        assert_eq!(
            display_ranges(editor, cx),
            vec![DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 1)]
        );
    });

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("abc\ndef\nghi\n\njkl\nmno", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(2), 1)
            ])
        });
        editor.delete_line(&DeleteLine, window, cx);
        assert_eq!(editor.display_text(cx), "\njkl\nmno");
        assert_eq!(
            display_ranges(editor, cx),
            vec![DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });
}

#[gpui::test]
fn test_join_lines_with_single_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaa\nbbb\nccc\nddd\n\n", cx);
        let mut editor = build_editor(buffer.clone(), window, cx);
        let buffer = buffer.read(cx).as_singleton().unwrap();

        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            &[Point::new(0, 0)..Point::new(0, 0)]
        );

        // When on single line, replace newline at end by space
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb\nccc\nddd\n\n");
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            &[Point::new(0, 3)..Point::new(0, 3)]
        );

        editor.undo(&Undo, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa\nbbb\nccc\nddd\n\n");

        // Select a full line, i.e. start of the first line to the start of the second line
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(1, 0)])
        });
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb\nccc\nddd\n\n");

        editor.undo(&Undo, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa\nbbb\nccc\nddd\n\n");

        // Select two full lines
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 0)..Point::new(2, 0)])
        });
        editor.join_lines(&JoinLines, window, cx);

        // Only the selected lines should be joined, not the third.
        assert_eq!(
            buffer.read(cx).text(),
            "aaa bbb\nccc\nddd\n\n",
            "only the two selected lines (a and b) should be joined"
        );

        // When multiple lines are selected, remove newlines that are spanned by the selection
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 5)..Point::new(2, 2)])
        });
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb ccc ddd\n\n");
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            &[Point::new(0, 11)..Point::new(0, 11)]
        );

        // Undo should be transactional
        editor.undo(&Undo, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb\nccc\nddd\n\n");
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            &[Point::new(0, 5)..Point::new(2, 2)]
        );

        // When joining an empty line don't insert a space
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(2, 1)..Point::new(2, 2)])
        });
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb\nccc\nddd\n");
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            [Point::new(2, 3)..Point::new(2, 3)]
        );

        // We can remove trailing newlines
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb\nccc\nddd");
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            [Point::new(2, 3)..Point::new(2, 3)]
        );

        // We don't blow up on the last line
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb\nccc\nddd");
        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            [Point::new(2, 3)..Point::new(2, 3)]
        );

        // reset to test indentation
        editor.buffer.update(cx, |buffer, cx| {
            buffer.edit(
                [
                    (Point::new(1, 0)..Point::new(1, 2), "  "),
                    (Point::new(2, 0)..Point::new(2, 3), "  \n\td"),
                ],
                None,
                cx,
            )
        });

        // We remove any leading spaces
        assert_eq!(buffer.read(cx).text(), "aaa bbb\n  c\n  \n\td");
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(0, 1)..Point::new(0, 1)])
        });
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb c\n  \n\td");

        // We don't insert a space for a line containing only spaces
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb c\n\td");

        // We ignore any leading tabs
        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb c d");

        editor
    });
}

#[gpui::test]
fn test_join_lines_with_multi_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaa\nbbb\nccc\nddd\n\n", cx);
        let mut editor = build_editor(buffer.clone(), window, cx);
        let buffer = buffer.read(cx).as_singleton().unwrap();

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                Point::new(0, 2)..Point::new(1, 1),
                Point::new(1, 2)..Point::new(1, 2),
                Point::new(3, 1)..Point::new(3, 2),
            ])
        });

        editor.join_lines(&JoinLines, window, cx);
        assert_eq!(buffer.read(cx).text(), "aaa bbb ccc\nddd\n");

        assert_eq!(
            editor
                .selections
                .ranges::<Point>(&editor.display_snapshot(cx)),
            [
                Point::new(0, 7)..Point::new(0, 7),
                Point::new(1, 3)..Point::new(1, 3)
            ]
        );
        editor
    });
}

#[gpui::test]
async fn test_join_lines_with_git_diff_base(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    let diff_base = r#"
        Line 0
        Line 1
        Line 2
        Line 3
        "#
    .unindent();

    cx.set_state(
        &r#"
        ˇLine 0
        Line 1
        Line 2
        Line 3
        "#
        .unindent(),
    );

    cx.set_head_text(&diff_base);
    executor.run_until_parked();

    // Join lines
    cx.update_editor(|editor, window, cx| {
        editor.join_lines(&JoinLines, window, cx);
    });
    executor.run_until_parked();

    cx.assert_editor_state(
        &r#"
        Line 0ˇ Line 1
        Line 2
        Line 3
        "#
        .unindent(),
    );
    // Join again
    cx.update_editor(|editor, window, cx| {
        editor.join_lines(&JoinLines, window, cx);
    });
    executor.run_until_parked();

    cx.assert_editor_state(
        &r#"
        Line 0 Line 1ˇ Line 2
        Line 3
        "#
        .unindent(),
    );
}
