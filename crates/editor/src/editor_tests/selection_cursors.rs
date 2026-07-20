use super::*;

#[gpui::test]
fn test_selection_with_mouse(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaaaa\nbbbbbb\ncccccc\nddddddd\n", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(2), 2), false, 1, window, cx);
    });
    assert_eq!(
        editor
            .update(cx, |editor, _, cx| display_ranges(editor, cx))
            .unwrap(),
        [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(2), 2)]
    );

    _ = editor.update(cx, |editor, window, cx| {
        editor.update_selection(
            DisplayPoint::new(DisplayRow(3), 3),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
    });

    assert_eq!(
        editor
            .update(cx, |editor, _, cx| display_ranges(editor, cx))
            .unwrap(),
        [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(3), 3)]
    );

    _ = editor.update(cx, |editor, window, cx| {
        editor.update_selection(
            DisplayPoint::new(DisplayRow(1), 1),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
    });

    assert_eq!(
        editor
            .update(cx, |editor, _, cx| display_ranges(editor, cx))
            .unwrap(),
        [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(1), 1)]
    );

    _ = editor.update(cx, |editor, window, cx| {
        editor.end_selection(window, cx);
        editor.update_selection(
            DisplayPoint::new(DisplayRow(3), 3),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
    });

    assert_eq!(
        editor
            .update(cx, |editor, _, cx| display_ranges(editor, cx))
            .unwrap(),
        [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(1), 1)]
    );

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(3), 3), true, 1, window, cx);
        editor.update_selection(
            DisplayPoint::new(DisplayRow(0), 0),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
    });

    assert_eq!(
        editor
            .update(cx, |editor, _, cx| display_ranges(editor, cx))
            .unwrap(),
        [
            DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(1), 1),
            DisplayPoint::new(DisplayRow(3), 3)..DisplayPoint::new(DisplayRow(0), 0)
        ]
    );

    _ = editor.update(cx, |editor, window, cx| {
        editor.end_selection(window, cx);
    });

    assert_eq!(
        editor
            .update(cx, |editor, _, cx| display_ranges(editor, cx))
            .unwrap(),
        [DisplayPoint::new(DisplayRow(3), 3)..DisplayPoint::new(DisplayRow(0), 0)]
    );
}

#[gpui::test]
fn test_multiple_cursor_removal(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaaaa\nbbbbbb\ncccccc\nddddddd\n", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(2), 1), false, 1, window, cx);
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.end_selection(window, cx);
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(3), 2), true, 1, window, cx);
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.end_selection(window, cx);
    });

    assert_eq!(
        editor
            .update(cx, |editor, _, cx| display_ranges(editor, cx))
            .unwrap(),
        [
            DisplayPoint::new(DisplayRow(2), 1)..DisplayPoint::new(DisplayRow(2), 1),
            DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(3), 2)
        ]
    );

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(2), 1), true, 1, window, cx);
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.end_selection(window, cx);
    });

    assert_eq!(
        editor
            .update(cx, |editor, _, cx| display_ranges(editor, cx))
            .unwrap(),
        [DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(3), 2)]
    );
}

#[gpui::test]
fn test_canceling_pending_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaaaa\nbbbbbb\ncccccc\ndddddd\n", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(2), 2), false, 1, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(2), 2)]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.update_selection(
            DisplayPoint::new(DisplayRow(3), 3),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(3), 3)]
        );
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.cancel(&Cancel, window, cx);
        editor.update_selection(
            DisplayPoint::new(DisplayRow(1), 1),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(3), 3)]
        );
    });
}

#[gpui::test]
fn test_movement_actions_with_pending_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaaaaa\nbbbbbb\ncccccc\ndddddd\n", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(2), 2), false, 1, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(2), 2)]
        );

        editor.move_down(&Default::default(), window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(3), 2)..DisplayPoint::new(DisplayRow(3), 2)]
        );

        editor.begin_selection(DisplayPoint::new(DisplayRow(2), 2), false, 1, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(2), 2)..DisplayPoint::new(DisplayRow(2), 2)]
        );

        editor.move_up(&Default::default(), window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(1), 2)..DisplayPoint::new(DisplayRow(1), 2)]
        );
    });
}

#[gpui::test]
fn test_extending_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple("aaa bbb ccc ddd eee", cx);
        build_editor(buffer, window, cx)
    });

    _ = editor.update(cx, |editor, window, cx| {
        editor.begin_selection(DisplayPoint::new(DisplayRow(0), 5), false, 1, window, cx);
        editor.end_selection(window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 5)..DisplayPoint::new(DisplayRow(0), 5)]
        );

        editor.extend_selection(DisplayPoint::new(DisplayRow(0), 10), 1, window, cx);
        editor.end_selection(window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 5)..DisplayPoint::new(DisplayRow(0), 10)]
        );

        editor.extend_selection(DisplayPoint::new(DisplayRow(0), 10), 1, window, cx);
        editor.end_selection(window, cx);
        editor.extend_selection(DisplayPoint::new(DisplayRow(0), 10), 2, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 5)..DisplayPoint::new(DisplayRow(0), 11)]
        );

        editor.update_selection(
            DisplayPoint::new(DisplayRow(0), 1),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
        editor.end_selection(window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 5)..DisplayPoint::new(DisplayRow(0), 0)]
        );

        editor.begin_selection(DisplayPoint::new(DisplayRow(0), 5), true, 1, window, cx);
        editor.end_selection(window, cx);
        editor.begin_selection(DisplayPoint::new(DisplayRow(0), 5), true, 2, window, cx);
        editor.end_selection(window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 4)..DisplayPoint::new(DisplayRow(0), 7)]
        );

        editor.extend_selection(DisplayPoint::new(DisplayRow(0), 10), 1, window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 4)..DisplayPoint::new(DisplayRow(0), 11)]
        );

        editor.update_selection(
            DisplayPoint::new(DisplayRow(0), 6),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 4)..DisplayPoint::new(DisplayRow(0), 7)]
        );

        editor.update_selection(
            DisplayPoint::new(DisplayRow(0), 1),
            0,
            gpui::Point::<f32>::default(),
            window,
            cx,
        );
        editor.end_selection(window, cx);
        assert_eq!(
            display_ranges(editor, cx),
            [DisplayPoint::new(DisplayRow(0), 7)..DisplayPoint::new(DisplayRow(0), 0)]
        );
    });
}
