use super::*;

#[gpui::test]
fn test_newline_below_multibuffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer_1 = cx.new(|cx| Buffer::local("aaa\nbbb\nccc", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("ddd\neee\nfff", cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer
    });

    cx.add_window(|window, cx| {
        let mut editor = build_editor(multibuffer, window, cx);

        assert_eq!(
            editor.text(cx),
            indoc! {"
                aaa
                bbb
                ccc
                ddd
                eee
                fff"}
        );

        // Cursor on the last line of the first excerpt.
        // The newline should be inserted within the first excerpt (buffer_1),
        // not in the second excerpt (buffer_2).
        select_ranges(
            &mut editor,
            indoc! {"
                aaa
                bbb
                cˇcc
                ddd
                eee
                fff"},
            window,
            cx,
        );
        editor.newline_below(&NewlineBelow, window, cx);
        assert_text_with_selections(
            &mut editor,
            indoc! {"
                aaa
                bbb
                ccc
                ˇ
                ddd
                eee
                fff"},
            cx,
        );
        buffer_1.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "aaa\nbbb\nccc\n");
        });
        buffer_2.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "ddd\neee\nfff");
        });

        editor
    });
}

#[gpui::test]
fn test_newline_below_multibuffer_middle_of_excerpt(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer_1 = cx.new(|cx| Buffer::local("aaa\nbbb\nccc", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("ddd\neee\nfff", cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer
    });

    cx.add_window(|window, cx| {
        let mut editor = build_editor(multibuffer, window, cx);

        // Cursor in the middle of the first excerpt.
        select_ranges(
            &mut editor,
            indoc! {"
                aˇaa
                bbb
                ccc
                ddd
                eee
                fff"},
            window,
            cx,
        );
        editor.newline_below(&NewlineBelow, window, cx);
        assert_text_with_selections(
            &mut editor,
            indoc! {"
                aaa
                ˇ
                bbb
                ccc
                ddd
                eee
                fff"},
            cx,
        );
        buffer_1.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "aaa\n\nbbb\nccc");
        });
        buffer_2.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "ddd\neee\nfff");
        });

        editor
    });
}

#[gpui::test]
fn test_newline_below_multibuffer_last_line_of_last_excerpt(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer_1 = cx.new(|cx| Buffer::local("aaa\nbbb\nccc", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("ddd\neee\nfff", cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer
    });

    cx.add_window(|window, cx| {
        let mut editor = build_editor(multibuffer, window, cx);

        // Cursor on the last line of the last excerpt.
        select_ranges(
            &mut editor,
            indoc! {"
                aaa
                bbb
                ccc
                ddd
                eee
                fˇff"},
            window,
            cx,
        );
        editor.newline_below(&NewlineBelow, window, cx);
        assert_text_with_selections(
            &mut editor,
            indoc! {"
                aaa
                bbb
                ccc
                ddd
                eee
                fff
                ˇ"},
            cx,
        );
        buffer_1.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "aaa\nbbb\nccc");
        });
        buffer_2.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "ddd\neee\nfff\n");
        });

        editor
    });
}

#[gpui::test]
fn test_newline_below_multibuffer_multiple_cursors(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let buffer_1 = cx.new(|cx| Buffer::local("aaa\nbbb\nccc", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("ddd\neee\nfff", cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer
    });

    cx.add_window(|window, cx| {
        let mut editor = build_editor(multibuffer, window, cx);

        // Cursors on the last line of the first excerpt and the first line
        // of the second excerpt. Each newline should go into its respective buffer.
        select_ranges(
            &mut editor,
            indoc! {"
                aaa
                bbb
                cˇcc
                dˇdd
                eee
                fff"},
            window,
            cx,
        );
        editor.newline_below(&NewlineBelow, window, cx);
        assert_text_with_selections(
            &mut editor,
            indoc! {"
                aaa
                bbb
                ccc
                ˇ
                ddd
                ˇ
                eee
                fff"},
            cx,
        );
        buffer_1.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "aaa\nbbb\nccc\n");
        });
        buffer_2.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "ddd\n\neee\nfff");
        });

        editor
    });
}
