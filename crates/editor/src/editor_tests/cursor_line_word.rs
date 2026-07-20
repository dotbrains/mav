use super::*;

#[gpui::test]
fn test_beginning_of_line_single_line_editor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor, window, cx| {
        editor.set_text("  indented text", window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 10)..DisplayPoint::new(DisplayRow(0), 10)
            ]);
        });

        editor.move_to_beginning_of_line(
            &MoveToBeginningOfLine {
                stop_at_soft_wraps: true,
                stop_at_indent: true,
            },
            window,
            cx,
        );
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 10)..DisplayPoint::new(DisplayRow(0), 10)
            ]);
        });

        editor.select_to_beginning_of_line(
            &SelectToBeginningOfLine {
                stop_at_soft_wraps: true,
                stop_at_indent: true,
            },
            window,
            cx,
        );
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 10)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });
}

#[gpui::test]
fn test_beginning_end_of_line_ignore_soft_wrap(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let move_to_beg = MoveToBeginningOfLine {
        stop_at_soft_wraps: false,
        stop_at_indent: false,
    };

    let move_to_end = MoveToEndOfLine {
        stop_at_soft_wraps: false,
    };

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("thequickbrownfox\njumpedoverthelazydogs", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.set_wrap_width(Some(140.0.into()), cx);

        // We expect the following lines after wrapping
        // ```
        // thequickbrownfox
        // jumpedoverthelazydo
        // gs
        // ```
        // The final `gs` was soft-wrapped onto a new line.
        assert_eq!(
            "thequickbrownfox\njumpedoverthelaz\nydogs",
            editor.display_text(cx),
        );

        // First, let's assert behavior on the first line, that was not soft-wrapped.
        // Start the cursor at the `k` on the first line
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 7)..DisplayPoint::new(DisplayRow(0), 7)
            ]);
        });

        // Moving to the beginning of the line should put us at the beginning of the line.
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            vec![DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),],
            display_ranges(editor, cx)
        );

        // Moving to the end of the line should put us at the end of the line.
        editor.move_to_end_of_line(&move_to_end, window, cx);
        assert_eq!(
            vec![DisplayPoint::new(DisplayRow(0), 16)..DisplayPoint::new(DisplayRow(0), 16),],
            display_ranges(editor, cx)
        );

        // Now, let's assert behavior on the second line, that ended up being soft-wrapped.
        // Start the cursor at the last line (`y` that was wrapped to a new line)
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(2), 0)
            ]);
        });

        // Moving to the beginning of the line should put us at the start of the second line of
        // display text, i.e., the `j`.
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            vec![DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),],
            display_ranges(editor, cx)
        );

        // Moving to the beginning of the line again should be a no-op.
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            vec![DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),],
            display_ranges(editor, cx)
        );

        // Moving to the end of the line should put us right after the `s` that was soft-wrapped to the
        // next display line.
        editor.move_to_end_of_line(&move_to_end, window, cx);
        assert_eq!(
            vec![DisplayPoint::new(DisplayRow(2), 5)..DisplayPoint::new(DisplayRow(2), 5),],
            display_ranges(editor, cx)
        );

        // Moving to the end of the line again should be a no-op.
        editor.move_to_end_of_line(&move_to_end, window, cx);
        assert_eq!(
            vec![DisplayPoint::new(DisplayRow(2), 5)..DisplayPoint::new(DisplayRow(2), 5),],
            display_ranges(editor, cx)
        );
    });
}

#[gpui::test]
fn test_beginning_of_line_stop_at_indent(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let move_to_beg = MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    let select_to_beg = SelectToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    let delete_to_beg = DeleteToBeginningOfLine {
        stop_at_indent: true,
    };

    let move_to_end = MoveToEndOfLine {
        stop_at_soft_wraps: false,
    };

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("abc\n  def", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 4),
            ]);
        });

        // Moving to the beginning of the line should put the first cursor at the beginning of the line,
        // and the second cursor at the first non-whitespace character in the line.
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(1), 2),
            ]
        );

        // Moving to the beginning of the line again should be a no-op for the first cursor,
        // and should move the second cursor to the beginning of the line.
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
            ]
        );

        // Moving to the beginning of the line again should still be a no-op for the first cursor,
        // and should move the second cursor back to the first non-whitespace character in the line.
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(1), 2),
            ]
        );

        // Selecting to the beginning of the line should select to the beginning of the line for the first cursor,
        // and to the first non-whitespace character in the line for the second cursor.
        editor.move_to_end_of_line(&move_to_end, window, cx);
        editor.move_left(&MoveLeft, window, cx);
        editor.select_to_beginning_of_line(&select_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 2),
            ]
        );

        // Selecting to the beginning of the line again should be a no-op for the first cursor,
        // and should select to the beginning of the line for the second cursor.
        editor.select_to_beginning_of_line(&select_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 0),
            ]
        );

        // Deleting to the beginning of the line should delete to the beginning of the line for the first cursor,
        // and should delete to the first non-whitespace character in the line for the second cursor.
        editor.move_to_end_of_line(&move_to_end, window, cx);
        editor.move_left(&MoveLeft, window, cx);
        editor.delete_to_beginning_of_line(&delete_to_beg, window, cx);
        assert_eq!(editor.text(cx), "c\n  f");
    });
}

#[gpui::test]
fn test_beginning_of_line_with_cursor_between_line_start_and_indent(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let move_to_beg = MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("    hello\nworld", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        // test cursor between line_start and indent_start
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 3)..DisplayPoint::new(DisplayRow(0), 3)
            ]);
        });

        // cursor should move to line_start
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );

        // cursor should move to indent_start
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 4)..DisplayPoint::new(DisplayRow(0), 4)]
        );

        // cursor should move to back to line_start
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });
}

#[gpui::test]
fn test_prev_next_word_boundary(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("use std::str::{foo, bar}\n\n  {baz.qux()}", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 11)..DisplayPoint::new(DisplayRow(0), 11),
                DisplayPoint::new(DisplayRow(2), 4)..DisplayPoint::new(DisplayRow(2), 4),
            ])
        });
        editor.move_to_previous_word_start(&MoveToPreviousWordStart, window, cx);
        assert_selection_ranges("use std::ˇstr::{foo, bar}\n\n  {ˇbaz.qux()}", editor, cx);

        editor.move_to_previous_word_start(&MoveToPreviousWordStart, window, cx);
        assert_selection_ranges("use stdˇ::str::{foo, bar}\n\nˇ  {baz.qux()}", editor, cx);

        editor.move_to_previous_word_start(&MoveToPreviousWordStart, window, cx);
        assert_selection_ranges("use ˇstd::str::{foo, bar}\nˇ\n  {baz.qux()}", editor, cx);

        editor.move_to_previous_word_start(&MoveToPreviousWordStart, window, cx);
        assert_selection_ranges("ˇuse std::str::{foo, barˇ}\n\n  {baz.qux()}", editor, cx);

        editor.move_to_previous_word_start(&MoveToPreviousWordStart, window, cx);
        assert_selection_ranges("ˇuse std::str::{foo, ˇbar}\n\n  {baz.qux()}", editor, cx);

        editor.move_to_next_word_end(&MoveToNextWordEnd, window, cx);
        assert_selection_ranges("useˇ std::str::{foo, barˇ}\n\n  {baz.qux()}", editor, cx);

        editor.move_to_next_word_end(&MoveToNextWordEnd, window, cx);
        assert_selection_ranges("use stdˇ::str::{foo, bar}ˇ\n\n  {baz.qux()}", editor, cx);

        editor.move_to_next_word_end(&MoveToNextWordEnd, window, cx);
        assert_selection_ranges("use std::ˇstr::{foo, bar}\nˇ\n  {baz.qux()}", editor, cx);

        editor.move_right(&MoveRight, window, cx);
        editor.select_to_previous_word_start(&SelectToPreviousWordStart, window, cx);
        assert_selection_ranges(
            "use std::«ˇs»tr::{foo, bar}\n«ˇ\n»  {baz.qux()}",
            editor,
            cx,
        );

        editor.select_to_previous_word_start(&SelectToPreviousWordStart, window, cx);
        assert_selection_ranges(
            "use std«ˇ::s»tr::{foo, bar«ˇ}\n\n»  {baz.qux()}",
            editor,
            cx,
        );

        editor.select_to_next_word_end(&SelectToNextWordEnd, window, cx);
        assert_selection_ranges(
            "use std::«ˇs»tr::{foo, bar}«ˇ\n\n»  {baz.qux()}",
            editor,
            cx,
        );
    });
}

#[gpui::test]
fn test_prev_next_word_bounds_with_soft_wrap(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("use one::{\n    two::three::four::five\n};", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.set_wrap_width(Some(140.0.into()), cx);
        assert_eq!(
            editor.display_text(cx),
            "use one::{\n    two::three::\n    four::five\n};"
        );

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(1), 7)..DisplayPoint::new(DisplayRow(1), 7)
            ]);
        });

        editor.move_to_next_word_end(&MoveToNextWordEnd, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(1), 9)..DisplayPoint::new(DisplayRow(1), 9)]
        );

        editor.move_to_next_word_end(&MoveToNextWordEnd, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(1), 14)..DisplayPoint::new(DisplayRow(1), 14)]
        );

        editor.move_to_next_word_end(&MoveToNextWordEnd, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(2), 4)..DisplayPoint::new(DisplayRow(2), 4)]
        );

        editor.move_to_next_word_end(&MoveToNextWordEnd, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(2), 8)..DisplayPoint::new(DisplayRow(2), 8)]
        );

        editor.move_to_previous_word_start(&MoveToPreviousWordStart, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(2), 4)..DisplayPoint::new(DisplayRow(2), 4)]
        );

        editor.move_to_previous_word_start(&MoveToPreviousWordStart, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(1), 14)..DisplayPoint::new(DisplayRow(1), 14)]
        );
    });
}
