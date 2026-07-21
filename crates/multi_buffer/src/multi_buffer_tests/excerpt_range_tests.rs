use super::*;

#[gpui::test]
fn test_set_excerpts_for_buffer_ordering(cx: &mut TestAppContext) {
    let buf1 = cx.new(|cx| {
        Buffer::local(
            indoc! {
            "zero
            one
            two
            two.five
            three
            four
            five
            six
            seven
            eight
            nine
            ten
            eleven
            ",
            },
            cx,
        )
    });
    let path1: PathKey = PathKey::with_sort_prefix(0, rel_path("root").into_arc());

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path1.clone(),
            buf1.clone(),
            vec![
                Point::row_range(1..2),
                Point::row_range(6..7),
                Point::row_range(11..12),
            ],
            1,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {
            "-----
            zero
            one
            two
            two.five
            -----
            four
            five
            six
            seven
            -----
            nine
            ten
            eleven
            "
        },
    );

    buf1.update(cx, |buffer, cx| buffer.edit([(0..5, "")], None, cx));

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path1.clone(),
            buf1.clone(),
            vec![
                Point::row_range(0..3),
                Point::row_range(5..7),
                Point::row_range(10..11),
            ],
            1,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {
            "-----
             one
             two
             two.five
             three
             four
             five
             six
             seven
             eight
             nine
             ten
             eleven
            "
        },
    );
}

#[gpui::test]
fn test_set_excerpts_for_buffer(cx: &mut TestAppContext) {
    let buf1 = cx.new(|cx| {
        Buffer::local(
            indoc! {
            "zero
            one
            two
            three
            four
            five
            six
            seven
            ",
            },
            cx,
        )
    });
    let path1: PathKey = PathKey::with_sort_prefix(0, rel_path("root").into_arc());
    let buf2 = cx.new(|cx| {
        Buffer::local(
            indoc! {
            "000
            111
            222
            333
            444
            555
            666
            777
            888
            999
            "
            },
            cx,
        )
    });
    let path2 = PathKey::with_sort_prefix(1, rel_path("root").into_arc());

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path1.clone(),
            buf1.clone(),
            vec![Point::row_range(0..1)],
            2,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {
        "-----
        zero
        one
        two
        three
        "
        },
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(path1.clone(), buf1.clone(), vec![], 2, cx);
    });

    assert_excerpts_match(&multibuffer, cx, "");

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path1.clone(),
            buf1.clone(),
            vec![Point::row_range(0..1), Point::row_range(7..8)],
            2,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
                zero
                one
                two
                three
                -----
                five
                six
                seven
                "},
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path1.clone(),
            buf1.clone(),
            vec![Point::row_range(0..1), Point::row_range(5..6)],
            2,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
                    zero
                    one
                    two
                    three
                    four
                    five
                    six
                    seven
                    "},
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path2.clone(),
            buf2.clone(),
            vec![Point::row_range(2..3)],
            2,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
                zero
                one
                two
                three
                four
                five
                six
                seven
                -----
                000
                111
                222
                333
                444
                555
                "},
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(path1.clone(), buf1.clone(), vec![], 2, cx);
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path1.clone(),
            buf1.clone(),
            vec![Point::row_range(3..4)],
            2,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
                one
                two
                three
                four
                five
                six
                -----
                000
                111
                222
                333
                444
                555
                "},
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path1.clone(),
            buf1.clone(),
            vec![Point::row_range(3..4)],
            2,
            cx,
        );
    });
}

#[gpui::test]
fn test_update_excerpt_ranges_for_path(cx: &mut TestAppContext) {
    let buffer = cx.new(|cx| {
        Buffer::local(
            indoc! {
            "row 0
            row 1
            row 2
            row 3
            row 4
            row 5
            row 6
            row 7
            row 8
            row 9
            row 10
            row 11
            row 12
            row 13
            row 14
            "},
            cx,
        )
    });
    let path = PathKey::with_sort_prefix(0, rel_path("test.rs").into_arc());

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path.clone(),
            buffer.clone(),
            vec![Point::row_range(2..4), Point::row_range(8..10)],
            0,
            cx,
        );
    });
    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
            row 2
            row 3
            row 4
            -----
            row 8
            row 9
            row 10
            "},
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.update_excerpts_for_path(
            path.clone(),
            buffer.clone(),
            vec![Point::row_range(12..13)],
            0,
            cx,
        );
    });
    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
            row 12
            row 13
            "},
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path.clone(),
            buffer.clone(),
            vec![Point::row_range(2..4)],
            0,
            cx,
        );
    });
    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
            row 2
            row 3
            row 4
            "},
    );
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.update_excerpts_for_path(
            path.clone(),
            buffer.clone(),
            vec![Point::row_range(3..5)],
            0,
            cx,
        );
    });
    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
            row 2
            row 3
            row 4
            row 5
            "},
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path.clone(),
            buffer.clone(),
            vec![
                Point::row_range(0..1),
                Point::row_range(6..8),
                Point::row_range(12..13),
            ],
            0,
            cx,
        );
    });
    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
            row 0
            row 1
            -----
            row 6
            row 7
            row 8
            -----
            row 12
            row 13
            "},
    );
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.update_excerpts_for_path(
            path.clone(),
            buffer.clone(),
            vec![Point::row_range(7..9)],
            0,
            cx,
        );
    });
    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
            row 6
            row 7
            row 8
            row 9
            "},
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path.clone(),
            buffer.clone(),
            vec![Point::row_range(2..3), Point::row_range(6..7)],
            0,
            cx,
        );
    });
    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
            row 2
            row 3
            -----
            row 6
            row 7
            "},
    );
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.update_excerpts_for_path(
            path.clone(),
            buffer.clone(),
            vec![Point::row_range(3..6)],
            0,
            cx,
        );
    });
    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
            row 2
            row 3
            row 4
            row 5
            row 6
            row 7
            "},
    );
}
