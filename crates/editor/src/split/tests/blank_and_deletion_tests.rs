use super::*;

#[gpui::test]
async fn test_inserting_consecutive_blank_line(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
        aaa
        bbb





        ccc
        ddd
    "
    .unindent();
    let current_text = "
        aaa
        bbb





        CCC
        ddd
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

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 3)..Point::new(1, 3), "\n")], None, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaa
        bbb






        CCC
        ddd"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        § spacer





        ccc
        ddd"
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
        bbb






        CCC
        ddd"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb





        ccc
        § spacer
        ddd"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_reverting_deletion_hunk(cx: &mut gpui::TestAppContext) {
    use git::Restore;
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
        ddd
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
        § spacer
        § spacer
        ddd
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

    let rhs_editor = editor.update(cx, |editor, _cx| editor.rhs_editor.clone());
    cx.update_window_entity(&rhs_editor, |editor, window, cx| {
        editor.change_selections(crate::SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::new(1, 0)..Point::new(1, 0)]);
        });
        editor.git_restore(&Restore, window, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaa
        bbb
        ccc
        ddd
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
        bbb
        ccc
        ddd
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
}

#[gpui::test]
async fn test_deleting_added_lines(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
        aaa
        old1
        old2
        old3
        old4
        zzz
    "
    .unindent();

    let current_text = "
        aaa
        new1
        new2
        new3
        new4
        zzz
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

    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (Point::new(2, 0)..Point::new(3, 0), ""),
                (Point::new(4, 0)..Point::new(5, 0), ""),
            ],
            None,
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
        new1
        new3
        § spacer
        § spacer
        zzz"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        old1
        old2
        old3
        old4
        zzz"
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
        new1
        new3
        § spacer
        § spacer
        zzz"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        old1
        old2
        old3
        old4
        zzz"
        .unindent(),
        &mut cx,
    );
}
