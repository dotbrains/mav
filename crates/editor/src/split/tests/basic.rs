use super::*;

#[gpui::test]
async fn test_expand_excerpt_with_hunk_before_excerpt_start(cx: &mut gpui::TestAppContext) {
    use rope::Point;

    let (editor, cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let base_text = "aaaaaaa rest_of_line\nsecond_line\nthird_line\nfourth_line";
    let current_text = "aaaaaaa rest_of_line\nsecond_line\nMODIFIED\nfourth_line";
    let (buffer, diff) = buffer_with_diff(base_text, current_text, cx);

    let buffer_snapshot = buffer.read_with(cx, |b, _| b.text_snapshot());
    diff.update(cx, |diff, cx| {
        diff.recalculate_diff_sync(&buffer_snapshot, cx);
    });
    cx.run_until_parked();

    let diff_snapshot = diff.read_with(cx, |diff, cx| diff.snapshot(cx));
    let ranges = diff_snapshot
        .hunks(&buffer_snapshot)
        .map(|hunk| hunk.range)
        .collect::<Vec<_>>();

    editor.update(cx, |editor, cx| {
        let path = PathKey::sorted(0);
        editor.update_excerpts_for_path(path, buffer.clone(), ranges, 0, diff.clone(), cx);
    });
    cx.run_until_parked();

    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [(Point::new(0, 7)..Point::new(1, 7), "\nnew_line\n")],
            None,
            cx,
        );
    });

    let excerpts = editor.update(cx, |editor, cx| {
        let snapshot = editor.rhs_multibuffer.read(cx).snapshot(cx);
        snapshot
            .excerpts()
            .map(|excerpt| snapshot.anchor_in_excerpt(excerpt.context.start).unwrap())
            .collect::<Vec<_>>()
    });
    editor.update(cx, |editor, cx| {
        editor.expand_excerpts(
            excerpts.into_iter(),
            2,
            multi_buffer::ExpandExcerptDirection::UpAndDown,
            cx,
        );
    });
}

#[gpui::test]
async fn test_basic_alignment(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
            aaa
            bbb
            ccc
            ddd
            eee
            fff
        "
    .unindent();
    let current_text = "
            aaa
            ddd
            eee
            fff
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
            eee
            fff"
        .unindent(),
        "
            § <no file>
            § -----
            aaa
            bbb
            ccc
            ddd
            eee
            fff"
        .unindent(),
        &mut cx,
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(3, 0)..Point::new(3, 3), "FFF")], None, cx);
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
            eee
            FFF"
        .unindent(),
        "
            § <no file>
            § -----
            aaa
            bbb
            ccc
            ddd
            eee
            fff"
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
            § spacer
            § spacer
            ddd
            eee
            FFF"
        .unindent(),
        "
            § <no file>
            § -----
            aaa
            bbb
            ccc
            ddd
            eee
            fff"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_deleting_unmodified_lines(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text1 = "
            aaa
            bbb
            ccc
            ddd
            eee"
    .unindent();

    let base_text2 = "
            fff
            ggg
            hhh
            iii
            jjj"
    .unindent();

    let (buffer1, diff1) = buffer_with_diff(&base_text1, &base_text1, &mut cx);
    let (buffer2, diff2) = buffer_with_diff(&base_text2, &base_text2, &mut cx);

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

    buffer1.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (Point::new(0, 0)..Point::new(1, 0), ""),
                (Point::new(3, 0)..Point::new(4, 0), ""),
            ],
            None,
            cx,
        );
    });
    buffer2.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (Point::new(0, 0)..Point::new(1, 0), ""),
                (Point::new(3, 0)..Point::new(4, 0), ""),
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
            § spacer
            bbb
            ccc
            § spacer
            eee
            § <no file>
            § -----
            § spacer
            ggg
            hhh
            § spacer
            jjj"
        .unindent(),
        "
            § <no file>
            § -----
            aaa
            bbb
            ccc
            ddd
            eee
            § <no file>
            § -----
            fff
            ggg
            hhh
            iii
            jjj"
        .unindent(),
        &mut cx,
    );

    let buffer1_snapshot = buffer1.read_with(cx, |buffer, _| buffer.text_snapshot());
    diff1.update(cx, |diff, cx| {
        diff.recalculate_diff_sync(&buffer1_snapshot, cx);
    });
    let buffer2_snapshot = buffer2.read_with(cx, |buffer, _| buffer.text_snapshot());
    diff2.update(cx, |diff, cx| {
        diff.recalculate_diff_sync(&buffer2_snapshot, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
            § <no file>
            § -----
            § spacer
            bbb
            ccc
            § spacer
            eee
            § <no file>
            § -----
            § spacer
            ggg
            hhh
            § spacer
            jjj"
        .unindent(),
        "
            § <no file>
            § -----
            aaa
            bbb
            ccc
            ddd
            eee
            § <no file>
            § -----
            fff
            ggg
            hhh
            iii
            jjj"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_deleting_added_line(cx: &mut gpui::TestAppContext) {
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
            NEW1
            NEW2
            ccc
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

    assert_split_content(
        &editor,
        "
            § <no file>
            § -----
            aaa
            NEW1
            NEW2
            ccc
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

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(2, 0)..Point::new(3, 0), "")], None, cx);
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
            § <no file>
            § -----
            aaa
            NEW1
            ccc
            ddd"
        .unindent(),
        "
            § <no file>
            § -----
            aaa
            bbb
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
            NEW1
            ccc
            ddd"
        .unindent(),
        "
            § <no file>
            § -----
            aaa
            bbb
            ccc
            ddd"
        .unindent(),
        &mut cx,
    );
}
