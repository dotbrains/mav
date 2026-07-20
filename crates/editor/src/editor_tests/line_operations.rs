use super::*;

#[gpui::test]
fn test_duplicate_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("abc\ndef\nghi\n", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 2),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
                DisplayPoint::new(DisplayRow(3), 0)..DisplayPoint::new(DisplayRow(3), 0),
            ])
        });
        editor.duplicate_line_down(&DuplicateLineDown, window, cx);
        assert_eq!(editor.display_text(cx), "abc\nabc\ndef\ndef\nghi\n\n");
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(1), 2),
                DisplayPoint::new(DisplayRow(3), 0)..DisplayPoint::new(DisplayRow(3), 0),
                DisplayPoint::new(DisplayRow(6), 0)..DisplayPoint::new(DisplayRow(6), 0),
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
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(2), 1),
            ])
        });
        editor.duplicate_line_down(&DuplicateLineDown, window, cx);
        assert_eq!(editor.display_text(cx), "abc\ndef\nghi\nabc\ndef\nghi\n");
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(3), 1)..DisplayPoint::new(DisplayRow(4), 1),
                DisplayPoint::new(DisplayRow(4), 2)..DisplayPoint::new(DisplayRow(5), 1),
            ]
        );
    });

    // With `duplicate_line_up` the selections move to the duplicated lines,
    // which are inserted above the original lines
    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("abc\ndef\nghi\n", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 2),
                DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0),
                DisplayPoint::new(DisplayRow(3), 0)..DisplayPoint::new(DisplayRow(3), 0),
            ])
        });
        editor.duplicate_line_up(&DuplicateLineUp, window, cx);
        assert_eq!(editor.display_text(cx), "abc\nabc\ndef\ndef\nghi\n\n");
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(0), 0)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(0), 2)..DisplayPoint::new(DisplayRow(0), 2),
                DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(2), 0),
                DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(5), 0),
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
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(2), 1),
            ])
        });
        editor.duplicate_line_up(&DuplicateLineUp, window, cx);
        assert_eq!(editor.display_text(cx), "abc\ndef\nghi\nabc\ndef\nghi\n");
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(2), 1),
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
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(2), 1),
            ])
        });
        editor.duplicate_selection(&DuplicateSelection, window, cx);
        assert_eq!(editor.display_text(cx), "abc\ndbc\ndef\ngf\nghi\n");
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(3), 1),
            ]
        );
    });
}

#[gpui::test]
async fn test_rotate_selections(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;

    // Rotate text selections (horizontal)
    cx.set_state("x=«1ˇ», y=«2ˇ», z=«3ˇ»");
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_forward(&RotateSelectionsForward, window, cx)
    });
    cx.assert_editor_state("x=«3ˇ», y=«1ˇ», z=«2ˇ»");
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_backward(&RotateSelectionsBackward, window, cx)
    });
    cx.assert_editor_state("x=«1ˇ», y=«2ˇ», z=«3ˇ»");

    // Rotate text selections (vertical)
    cx.set_state(indoc! {"
        x=«1ˇ»
        y=«2ˇ»
        z=«3ˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_forward(&RotateSelectionsForward, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        x=«3ˇ»
        y=«1ˇ»
        z=«2ˇ»
    "});
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_backward(&RotateSelectionsBackward, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        x=«1ˇ»
        y=«2ˇ»
        z=«3ˇ»
    "});

    // Rotate text selections (vertical, different lengths)
    cx.set_state(indoc! {"
        x=\"«ˇ»\"
        y=\"«aˇ»\"
        z=\"«aaˇ»\"
    "});
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_forward(&RotateSelectionsForward, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        x=\"«aaˇ»\"
        y=\"«ˇ»\"
        z=\"«aˇ»\"
    "});
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_backward(&RotateSelectionsBackward, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        x=\"«ˇ»\"
        y=\"«aˇ»\"
        z=\"«aaˇ»\"
    "});

    // Rotate whole lines (cursor positions preserved)
    cx.set_state(indoc! {"
        ˇline123
        liˇne23
        line3ˇ
    "});
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_forward(&RotateSelectionsForward, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        line3ˇ
        ˇline123
        liˇne23
    "});
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_backward(&RotateSelectionsBackward, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        ˇline123
        liˇne23
        line3ˇ
    "});

    // Rotate whole lines, multiple cursors per line (positions preserved)
    cx.set_state(indoc! {"
        ˇliˇne123
        ˇline23
        ˇline3
    "});
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_forward(&RotateSelectionsForward, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        ˇline3
        ˇliˇne123
        ˇline23
    "});
    cx.update_editor(|e, window, cx| {
        e.rotate_selections_backward(&RotateSelectionsBackward, window, cx)
    });
    cx.assert_editor_state(indoc! {"
        ˇliˇne123
        ˇline23
        ˇline3
    "});
}

#[gpui::test]
fn test_move_line_up_down(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&sample_text(10, 5, 'a'), cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.fold_creases(
            vec![
                Crease::simple(Point::new(0, 2)..Point::new(1, 2), FoldPlaceholder::test()),
                Crease::simple(Point::new(2, 3)..Point::new(4, 1), FoldPlaceholder::test()),
                Crease::simple(Point::new(7, 0)..Point::new(8, 4), FoldPlaceholder::test()),
            ],
            true,
            window,
            cx,
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(3), 1)..DisplayPoint::new(DisplayRow(3), 1),
                DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(4), 3),
                DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(5), 2),
            ])
        });
        assert_eq!(
            editor.display_text(cx),
            "aa⋯bbb\nccc⋯eeee\nfffff\nggggg\n⋯i\njjjjj"
        );

        editor.move_line_up(&MoveLineUp, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "aa⋯bbb\nccc⋯eeee\nggggg\n⋯i\njjjjj\nfffff"
        );
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(0), 1)..DisplayPoint::new(DisplayRow(0), 1),
                DisplayPoint::new(DisplayRow(2), 1)..DisplayPoint::new(DisplayRow(2), 1),
                DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(3), 3),
                DisplayPoint::new(DisplayRow(4), 0)..DisplayPoint::new(DisplayRow(4), 2)
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.move_line_down(&MoveLineDown, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "ccc⋯eeee\naa⋯bbb\nfffff\nggggg\n⋯i\njjjjj"
        );
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(1), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(3), 1)..DisplayPoint::new(DisplayRow(3), 1),
                DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(4), 3),
                DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(5), 2)
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.move_line_down(&MoveLineDown, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "ccc⋯eeee\nfffff\naa⋯bbb\nggggg\n⋯i\njjjjj"
        );
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(2), 1)..DisplayPoint::new(DisplayRow(2), 1),
                DisplayPoint::new(DisplayRow(3), 1)..DisplayPoint::new(DisplayRow(3), 1),
                DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(4), 3),
                DisplayPoint::new(DisplayRow(5), 0)..DisplayPoint::new(DisplayRow(5), 2)
            ]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.move_line_up(&MoveLineUp, window, cx);
        assert_eq!(
            editor.display_text(cx),
            "ccc⋯eeee\naa⋯bbb\nggggg\n⋯i\njjjjj\nfffff"
        );
        assert_eq!(
            display_ranges(editor, cx),
            vec![
                DisplayPoint::new(DisplayRow(1), 1)..DisplayPoint::new(DisplayRow(1), 1),
                DisplayPoint::new(DisplayRow(2), 1)..DisplayPoint::new(DisplayRow(2), 1),
                DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(3), 3),
                DisplayPoint::new(DisplayRow(4), 0)..DisplayPoint::new(DisplayRow(4), 2)
            ]
        );
    });
}

#[gpui::test]
fn test_move_line_up_selection_at_end_of_fold(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("\n\n\n\n\n\naaaa\nbbbb\ncccc", cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        editor.fold_creases(
            vec![Crease::simple(
                Point::new(6, 4)..Point::new(7, 4),
                FoldPlaceholder::test(),
            )],
            true,
            window,
            cx,
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(7, 4)..Point::new(7, 4)])
        });
        assert_eq!(editor.display_text(cx), "\n\n\n\n\n\naaaa⋯\ncccc");
        editor.move_line_up(&MoveLineUp, window, cx);
        let buffer_text = editor.buffer.read(cx).snapshot(cx).text();
        assert_eq!(buffer_text, "\n\n\n\n\naaaa\nbbbb\n\ncccc");
    });
}

#[gpui::test]
fn test_move_line_up_down_with_blocks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(&sample_text(10, 5, 'a'), cx);
        build_editor(buffer, window, cx)
    });
    _ = editor.update(cx, |editor, window, cx| {
        let snapshot = editor.buffer.read(cx).snapshot(cx);
        editor.insert_blocks(
            [BlockProperties {
                style: BlockStyle::Fixed,
                placement: BlockPlacement::Below(snapshot.anchor_after(Point::new(2, 0))),
                height: Some(1),
                render: Arc::new(|_| div().into_any()),
                priority: 0,
            }],
            Some(Autoscroll::fit()),
            cx,
        );
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(2, 0)..Point::new(2, 0)])
        });
        editor.move_line_down(&MoveLineDown, window, cx);
    });
}

#[gpui::test]
async fn test_selections_and_replace_blocks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(
        &"
            ˇzero
            one
            two
            three
            four
            five
        "
        .unindent(),
    );

    // Create a four-line block that replaces three lines of text.
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let snapshot = &snapshot.buffer_snapshot();
        let placement = BlockPlacement::Replace(
            snapshot.anchor_after(Point::new(1, 0))..=snapshot.anchor_after(Point::new(3, 0)),
        );
        editor.insert_blocks(
            [BlockProperties {
                placement,
                height: Some(4),
                style: BlockStyle::Sticky,
                render: Arc::new(|_| gpui::div().into_any_element()),
                priority: 0,
            }],
            None,
            cx,
        );
    });

    // Move down so that the cursor touches the block.
    cx.update_editor(|editor, window, cx| {
        editor.move_down(&Default::default(), window, cx);
    });
    cx.assert_editor_state(
        &"
            zero
            «one
            two
            threeˇ»
            four
            five
        "
        .unindent(),
    );

    // Move down past the block.
    cx.update_editor(|editor, window, cx| {
        editor.move_down(&Default::default(), window, cx);
    });
    cx.assert_editor_state(
        &"
            zero
            one
            two
            three
            ˇfour
            five
        "
        .unindent(),
    );
}
