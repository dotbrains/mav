use super::*;

#[gpui::test]
async fn test_adding_line_to_addition_hunk(cx: &mut gpui::TestAppContext) {
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
        aaa
        bbb
        xxx
        yyy
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
        aaa
        bbb
        xxx
        yyy
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        § spacer
        § spacer
        ccc"
        .unindent(),
        &mut cx,
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(3, 3)..Point::new(3, 3), "\nzzz")], None, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaa
        bbb
        xxx
        yyy
        zzz
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        § spacer
        § spacer
        § spacer
        ccc"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_scrolling(cx: &mut gpui::TestAppContext) {
    use crate::test::editor_content_with_blocks_and_size;
    use gpui::size;
    use rope::Point;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let long_line = "x".repeat(200);
    let mut lines: Vec<String> = (0..50).map(|i| format!("line {i}")).collect();
    lines[25] = long_line;
    let content = lines.join("\n");

    let (buffer, diff) = buffer_with_diff(&content, &content, &mut cx);

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

    let (rhs_editor, lhs_editor) = editor.update(cx, |editor, _cx| {
        let lhs = editor.lhs.as_ref().expect("should have lhs editor");
        (editor.rhs_editor.clone(), lhs.editor.clone())
    });

    rhs_editor.update_in(cx, |e, window, cx| {
        e.set_scroll_position(gpui::Point::new(0., 10.), window, cx);
    });

    let rhs_pos =
        rhs_editor.update_in(cx, |e, window, cx| e.snapshot(window, cx).scroll_position());
    let lhs_pos =
        lhs_editor.update_in(cx, |e, window, cx| e.snapshot(window, cx).scroll_position());
    assert_eq!(rhs_pos.y, 10., "RHS should be scrolled to row 10");
    assert_eq!(
        lhs_pos.y, rhs_pos.y,
        "LHS should have same scroll position as RHS after set_scroll_position"
    );

    let draw_size = size(px(300.), px(300.));

    rhs_editor.update_in(cx, |e, window, cx| {
        e.change_selections(Some(crate::Autoscroll::fit()).into(), window, cx, |s| {
            s.select_ranges([Point::new(25, 150)..Point::new(25, 150)]);
        });
    });

    let _ = editor_content_with_blocks_and_size(&rhs_editor, draw_size, &mut cx);
    cx.run_until_parked();
    let _ = editor_content_with_blocks_and_size(&lhs_editor, draw_size, &mut cx);
    cx.run_until_parked();

    let rhs_pos =
        rhs_editor.update_in(cx, |e, window, cx| e.snapshot(window, cx).scroll_position());
    let lhs_pos =
        lhs_editor.update_in(cx, |e, window, cx| e.snapshot(window, cx).scroll_position());

    assert!(
        rhs_pos.y > 0.,
        "RHS should have scrolled vertically to show cursor at row 25"
    );
    assert!(
        rhs_pos.x > 0.,
        "RHS should have scrolled horizontally to show cursor at column 150"
    );
    assert_eq!(
        lhs_pos.y, rhs_pos.y,
        "LHS should have same vertical scroll position as RHS after autoscroll"
    );
    assert_eq!(
        lhs_pos.x, rhs_pos.x,
        "LHS should have same horizontal scroll position as RHS after autoscroll"
    )
}

#[gpui::test]
async fn test_edit_line_before_soft_wrapped_line_preceding_hunk(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
        first line
        aaaa bbbb cccc dddd eeee ffff
        original
    "
    .unindent();

    let current_text = "
        first line
        aaaa bbbb cccc dddd eeee ffff
        modified
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
                first line
                aaaa bbbb cccc dddd eeee ffff
                § spacer
                § spacer
                modified"
            .unindent(),
        "
                § <no file>
                § -----
                first line
                aaaa bbbb\x20
                cccc dddd\x20
                eeee ffff
                original"
            .unindent(),
        &mut cx,
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [(Point::new(0, 0)..Point::new(0, 10), "edited first")],
            None,
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
                edited first
                aaaa bbbb cccc dddd eeee ffff
                § spacer
                § spacer
                modified"
            .unindent(),
        "
                § <no file>
                § -----
                first line
                aaaa bbbb\x20
                cccc dddd\x20
                eeee ffff
                original"
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
                edited first
                aaaa bbbb cccc dddd eeee ffff
                § spacer
                § spacer
                modified"
            .unindent(),
        "
                § <no file>
                § -----
                first line
                aaaa bbbb\x20
                cccc dddd\x20
                eeee ffff
                original"
            .unindent(),
        &mut cx,
    );
}
