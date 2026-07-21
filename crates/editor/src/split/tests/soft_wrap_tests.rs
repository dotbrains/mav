use super::*;

#[gpui::test]
async fn test_soft_wrap_at_end_of_excerpt(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let text = "aaaa bbbb cccc dddd eeee ffff";

    let (buffer1, diff1) = buffer_with_diff(text, text, &mut cx);
    let (buffer2, diff2) = buffer_with_diff(text, text, &mut cx);

    editor.update(cx, |editor, cx| {
        let end = Point::new(0, text.len() as u32);
        let path1 = PathKey::sorted(0);
        editor.update_excerpts_for_path(
            path1,
            buffer1.clone(),
            vec![Point::new(0, 0)..end],
            0,
            diff1.clone(),
            cx,
        );
        let path2 = PathKey::sorted(1);
        editor.update_excerpts_for_path(
            path2,
            buffer2.clone(),
            vec![Point::new(0, 0)..end],
            0,
            diff2.clone(),
            cx,
        );
    });

    cx.run_until_parked();

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
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff"
            .unindent(),
        "
        § <no file>
        § -----
        aaaa bbbb cccc dddd eeee ffff
        § spacer
        § spacer
        § <no file>
        § -----
        aaaa bbbb cccc dddd eeee ffff
        § spacer
        § spacer"
            .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_soft_wrap_before_modification_hunk(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
        aaaa bbbb cccc dddd eeee ffff
        old line one
        old line two
    "
    .unindent();

    let current_text = "
        aaaa bbbb cccc dddd eeee ffff
        new line
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
        px(200.0),
        px(400.0),
        "
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff
        new line
        § spacer"
            .unindent(),
        "
        § <no file>
        § -----
        aaaa bbbb cccc dddd eeee ffff
        § spacer
        § spacer
        old line one
        old line two"
            .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_soft_wrap_before_deletion_hunk(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
        aaaa bbbb cccc dddd eeee ffff
        deleted line one
        deleted line two
        after
    "
    .unindent();

    let current_text = "
        aaaa bbbb cccc dddd eeee ffff
        after
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
        § spacer
        § spacer
        § spacer
        § spacer
        after"
            .unindent(),
        "
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff
        deleted line\x20
        one
        deleted line\x20
        two
        after"
            .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_soft_wrap_spacer_after_editing_second_line(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let text = "
        aaaa bbbb cccc dddd eeee ffff
        short
    "
    .unindent();

    let (buffer, diff) = buffer_with_diff(&text, &text, &mut cx);

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
        short"
            .unindent(),
        "
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff
        short"
            .unindent(),
        &mut cx,
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 0)..Point::new(1, 5), "modified")], None, cx);
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
        modified"
            .unindent(),
        "
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff
        short"
            .unindent(),
        &mut cx,
    );

    let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.text_snapshot());
    diff.update(cx, |diff, cx| {
        diff.recalculate_diff_sync(&buffer_snapshot, cx);
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
        modified"
            .unindent(),
        "
        § <no file>
        § -----
        aaaa bbbb\x20
        cccc dddd\x20
        eeee ffff
        short"
            .unindent(),
        &mut cx,
    );
}
