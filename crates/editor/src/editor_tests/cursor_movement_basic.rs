use super::*;

#[gpui::test]
fn test_move_cursor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer = cx.update(|cx| MultiBuffer::build_simple(&sample_text(6, 6, 'a'), cx));
    let editor = cx.add_window(|window, cx| build_editor(buffer.clone(), window, cx));

    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            vec![
                (Point::new(1, 0)..Point::new(1, 0), "\t"),
                (Point::new(1, 1)..Point::new(1, 1), "\t"),
            ],
            None,
            cx,
        );
    });
    _ = editor.update(cx, |editor, window, cx| {
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0)]
        );

        editor.move_right(&MoveRight, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 4)]
        );

        editor.move_left(&MoveLeft, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0)]
        );

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );

        editor.move_to_end(&MoveToEnd, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(5), 6)..DisplayPoint::new(DisplayRow(5), 6)]
        );

        editor.move_to_beginning(&MoveToBeginning, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0)]
        );

        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 2)
            ]);
        });
        editor.select_to_beginning(&SelectToBeginning, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 0)]
        );

        editor.select_to_end(&SelectToEnd, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(5), 6)]
        );
    });
}

#[gpui::test]
fn test_move_cursor_multibyte(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("🟥🟧🟨🟩🟦🟪\nabcde\nαβγδε", cx);
        build_editor(buffer, window, cx)
    });

    assert_eq!('🟥'.len_utf8(), 4);
    assert_eq!('α'.len_utf8(), 2);

    _ = editor.update(cx, |editor, window, cx| {
        editor.fold_creases(
            vec![
                Crease::simple(Point::new(0, 8)..Point::new(0, 16), FoldPlaceholder::test()),
                Crease::simple(Point::new(1, 2)..Point::new(1, 4), FoldPlaceholder::test()),
                Crease::simple(Point::new(2, 4)..Point::new(2, 8), FoldPlaceholder::test()),
            ],
            true,
            window,
            cx,
        );
        assert_eq!(editor.display_text(cx), "🟥🟧⋯🟦🟪\nab⋯e\nαβ⋯ε");

        editor.move_right(&MoveRight, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(0, "🟥".len())]);
        editor.move_right(&MoveRight, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(0, "🟥🟧".len())]);
        editor.move_right(&MoveRight, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(0, "🟥🟧⋯".len())]);

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(1, "ab⋯e".len())]);
        editor.move_left(&MoveLeft, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(1, "ab⋯".len())]);
        editor.move_left(&MoveLeft, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(1, "ab".len())]);
        editor.move_left(&MoveLeft, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(1, "a".len())]);

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(2, "α".len())]);
        editor.move_right(&MoveRight, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(2, "αβ".len())]);
        editor.move_right(&MoveRight, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(2, "αβ⋯".len())]);
        editor.move_right(&MoveRight, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(2, "αβ⋯ε".len())]);

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(1, "ab⋯e".len())]);
        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(2, "αβ⋯ε".len())]);
        editor.move_up(&MoveUp, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(1, "ab⋯e".len())]);

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(0, "🟥🟧".len())]);
        editor.move_left(&MoveLeft, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(0, "🟥".len())]);
        editor.move_left(&MoveLeft, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(0, "".len())]);
    });
}

#[gpui::test]
fn test_move_cursor_different_line_lengths(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("ⓐⓑⓒⓓⓔ\nabcd\nαβγ\nabcd\nⓐⓑⓒⓓⓔ\n", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([empty_range(0, "ⓐⓑⓒⓓⓔ".len())]);
        });

        // moving above start of document should move selection to start of document,
        // but the next move down should still be at the original goal_x
        editor.move_up(&MoveUp, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(0, "".len())]);

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(1, "abcd".len())]);

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(2, "αβγ".len())]);

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(3, "abcd".len())]);

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(4, "ⓐⓑⓒⓓⓔ".len())]);

        // moving past end of document should not change goal_x
        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(5, "".len())]);

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(5, "".len())]);

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(4, "ⓐⓑⓒⓓⓔ".len())]);

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(3, "abcd".len())]);

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(display_ranges(editor, cx), &[empty_range(2, "αβγ".len())]);
    });
}

#[gpui::test]
fn test_beginning_end_of_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let move_to_beg = MoveToBeginningOfLine {
        stop_at_soft_wraps: true,
        stop_at_indent: true,
    };

    let delete_to_beg = DeleteToBeginningOfLine {
        stop_at_indent: false,
    };

    let move_to_end = MoveToEndOfLine {
        stop_at_soft_wraps: true,
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
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(1), 2),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.move_to_beginning_of_line(&move_to_beg, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(1), 2),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.move_to_end_of_line(&move_to_end, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 3)..DisplayPoint::new(DisplayRow(0), 3),
                DisplayPoint::new(DisplayRow(1), 5)..DisplayPoint::new(DisplayRow(1), 5),
            ]
        );
    });

    // Moving to the end of line again is a no-op.
    _ = editor.update(cx, |editor, window, cx| {
        editor.move_to_end_of_line(&move_to_end, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 3)..DisplayPoint::new(DisplayRow(0), 3),
                DisplayPoint::new(DisplayRow(1), 5)..DisplayPoint::new(DisplayRow(1), 5),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.move_left(&MoveLeft, window, cx);
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
            &[
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 2),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
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
            &[
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 0),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
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
            &[
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 2),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.select_to_end_of_line(
            &SelectToEndOfLine {
                stop_at_soft_wraps: true,
            },
            window,
            cx,
        );
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 3),
                DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 5),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.delete_to_end_of_line(&DeleteToEndOfLine, window, cx);
        assert_eq!(editor.display_text(cx), "ab\n  de");
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 2),
                DisplayPoint::new(DisplayRow(1), 4)..DisplayPoint::new(DisplayRow(1), 4),
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.delete_to_beginning_of_line(&delete_to_beg, window, cx);
        assert_eq!(editor.display_text(cx), "\n");
        assert_eq!(
            display_ranges(editor, cx),
            &[
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 0),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
            ]
        );
    });
}
