use super::*;

#[gpui::test]
async fn test_no_base_text(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let (buffer1, diff1) = buffer_with_diff("xxx\nyyy", "xxx\nyyy", &mut cx);

    let current_text = "
        aaa
        bbb
        ccc
    "
    .unindent();

    let buffer2 = cx.new(|cx| Buffer::local(current_text.to_string(), cx));
    let diff2 = cx.new(|cx| BufferDiff::new(&buffer2.read(cx).text_snapshot(), None, None, cx));

    editor.update(cx, |editor, cx| {
        let path1 = PathKey::sorted(0);
        editor.update_excerpts_for_path(
            path1,
            buffer1.clone(),
            vec![Point::new(0, 0)..buffer1.read(cx).max_point()],
            0,
            diff1.clone(),
            cx,
        );

        let path2 = PathKey::sorted(1);
        editor.update_excerpts_for_path(
            path2,
            buffer2.clone(),
            vec![Point::new(0, 0)..buffer2.read(cx).max_point()],
            1,
            diff2.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        xxx
        yyy
        § <no file>
        § -----
        aaa
        bbb
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        xxx
        yyy
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer"
            .unindent(),
        &mut cx,
    );

    buffer1.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(0, 3)..Point::new(0, 3), "z")], None, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        xxxz
        yyy
        § <no file>
        § -----
        aaa
        bbb
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        xxx
        yyy
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer"
            .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_deleting_char_in_added_line(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
        aaa
        bbb
        ccc
    "
    .unindent();

    let current_text = "
        NEW1
        NEW2
        ccc
    "
    .unindent();

    let (buffer, diff) = buffer_with_diff(&base_text, &current_text, &mut cx);

    editor.update(cx, |editor, cx| {
        let path = PathKey::sorted(0);
        editor.update_excerpts_for_path(
            path,
            buffer.clone(),
            vec![Point::new(0, 0)..buffer.read(cx).max_point()],
            0,
            diff.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        NEW1
        NEW2
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        ccc"
        .unindent(),
        &mut cx,
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 3)..Point::new(1, 4), "")], None, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        NEW1
        NEW
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        ccc"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_soft_wrap_spacer_before_added_line(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "aaaa bbbb cccc dddd eeee ffff\n";

    let current_text = "
        aaaa bbbb cccc dddd eeee ffff
        added line
    "
    .unindent();

    let (buffer, diff) = buffer_with_diff(&base_text, &current_text, &mut cx);

    editor.update(cx, |editor, cx| {
        let path = PathKey::sorted(0);
        editor.update_excerpts_for_path(
            path,
            buffer.clone(),
            vec![Point::new(0, 0)..buffer.read(cx).max_point()],
            0,
            diff.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    assert_split_content_with_widths(
        &editor,
        px(400.0),
        px(200.0),
        "
        § <no file>
        § -----
        aaaa bbbb cccc dddd eeee ffff
        § spacer
        § spacer
        added line"
            .unindent(),
        "
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff
        § spacer"
            .unindent(),
        &mut cx,
    );

    assert_split_content_with_widths(
        &editor,
        px(200.0),
        px(400.0),
        "
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff
        added line"
            .unindent(),
        "
        § <no file>
        § -----
        aaaa bbbb cccc dddd eeee ffff
        § spacer
        § spacer
        § spacer"
            .unindent(),
        &mut cx,
    );
}

#[gpui::test]
#[ignore]
async fn test_joining_added_line_with_unmodified_line(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
        aaa
        bbb
        ccc
        ddd
        eee
    "
    .unindent();

    let current_text = "
        aaa
        NEW
        eee
    "
    .unindent();

    let (buffer, diff) = buffer_with_diff(&base_text, &current_text, &mut cx);

    editor.update(cx, |editor, cx| {
        let path = PathKey::sorted(0);
        editor.update_excerpts_for_path(
            path,
            buffer.clone(),
            vec![Point::new(0, 0)..buffer.read(cx).max_point()],
            0,
            diff.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaa
        NEW
        § spacer
        § spacer
        eee"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        ccc
        ddd
        eee"
        .unindent(),
        &mut cx,
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 3)..Point::new(2, 0), "")], None, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaa
        § spacer
        § spacer
        § spacer
        NEWeee"
            .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        ccc
        ddd
        eee"
        .unindent(),
        &mut cx,
    );

    let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.text_snapshot());
    diff.update(cx, |diff, cx| {
        diff.recalculate_diff_sync(&buffer_snapshot, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaa
        NEWeee
        § spacer
        § spacer
        § spacer"
            .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        ccc
        ddd
        eee"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_added_file_at_end(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "";
    let current_text = "
        aaaa bbbb cccc dddd eeee ffff
        bbb
        ccc
    "
    .unindent();

    let (buffer, diff) = buffer_with_diff(base_text, &current_text, &mut cx);

    editor.update(cx, |editor, cx| {
        let path = PathKey::sorted(0);
        editor.update_excerpts_for_path(
            path,
            buffer.clone(),
            vec![Point::new(0, 0)..buffer.read(cx).max_point()],
            0,
            diff.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaaa bbbb cccc dddd eeee ffff
        bbb
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer"
            .unindent(),
        &mut cx,
    );

    assert_split_content_with_widths(
        &editor,
        px(200.0),
        px(200.0),
        "
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff
        bbb
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer
        § spacer
        § spacer"
            .unindent(),
        &mut cx,
    );
}
