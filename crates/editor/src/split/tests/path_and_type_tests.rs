use super::*;

#[gpui::test]
async fn test_split_after_removing_folded_buffer(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Unified).await;

    let base_text_a = "
        aaa
        bbb
        ccc
    "
    .unindent();
    let current_text_a = "
        aaa
        bbb modified
        ccc
    "
    .unindent();

    let base_text_b = "
        xxx
        yyy
        zzz
    "
    .unindent();
    let current_text_b = "
        xxx
        yyy modified
        zzz
    "
    .unindent();

    let (buffer_a, diff_a) = buffer_with_diff(&base_text_a, &current_text_a, &mut cx);
    let (buffer_b, diff_b) = buffer_with_diff(&base_text_b, &current_text_b, &mut cx);

    let path_a = PathKey::sorted(0);
    let path_b = PathKey::sorted(1);

    editor.update(cx, |editor, cx| {
        editor.update_excerpts_for_path(
            path_a.clone(),
            buffer_a.clone(),
            vec![Point::new(0, 0)..buffer_a.read(cx).max_point()],
            0,
            diff_a.clone(),
            cx,
        );
        editor.update_excerpts_for_path(
            path_b.clone(),
            buffer_b.clone(),
            vec![Point::new(0, 0)..buffer_b.read(cx).max_point()],
            0,
            diff_b.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    let buffer_a_id = buffer_a.read_with(cx, |buffer, _| buffer.remote_id());
    editor.update(cx, |editor, cx| {
        editor.rhs_editor().update(cx, |right_editor, cx| {
            right_editor.fold_buffer(buffer_a_id, cx)
        });
    });

    cx.run_until_parked();

    editor.update(cx, |editor, cx| {
        editor.remove_excerpts_for_path(path_a.clone(), cx);
    });
    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| editor.split(window, cx));
    cx.run_until_parked();

    editor.update(cx, |editor, cx| {
        editor.update_excerpts_for_path(
            path_a.clone(),
            buffer_a.clone(),
            vec![Point::new(0, 0)..buffer_a.read(cx).max_point()],
            0,
            diff_a.clone(),
            cx,
        );
        assert!(
            !editor
                .lhs_editor()
                .unwrap()
                .read(cx)
                .is_buffer_folded(buffer_a_id, cx)
        );
        assert!(
            !editor
                .rhs_editor()
                .read(cx)
                .is_buffer_folded(buffer_a_id, cx)
        );
    });
}

#[gpui::test]
async fn test_two_path_keys_for_one_buffer(cx: &mut gpui::TestAppContext) {
    use multi_buffer::PathKey;
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let base_text = "
        aaa
        bbb
        ccc
    "
    .unindent();
    let current_text = "
        aaa
        bbb modified
        ccc
    "
    .unindent();

    let (buffer, diff) = buffer_with_diff(&base_text, &current_text, &mut cx);

    let path_key_1 = PathKey {
        sort_prefix: Some(0),
        path: rel_path("file1.txt").into(),
    };
    let path_key_2 = PathKey {
        sort_prefix: Some(1),
        path: rel_path("file1.txt").into(),
    };

    editor.update(cx, |editor, cx| {
        editor.update_excerpts_for_path(
            path_key_1.clone(),
            buffer.clone(),
            vec![Point::new(0, 0)..Point::new(1, 0)],
            0,
            diff.clone(),
            cx,
        );
        editor.update_excerpts_for_path(
            path_key_2.clone(),
            buffer.clone(),
            vec![Point::new(1, 0)..buffer.read(cx).max_point()],
            1,
            diff.clone(),
            cx,
        );
    });

    cx.run_until_parked();
}

#[gpui::test]
async fn test_spacer_blocks_revert_after_temporary_edit(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::EditorWidth, DiffViewStyle::Split).await;

    let base_text = "
        aaa
        bbb
    "
    .unindent();
    let current_text = "
        aaa
        bbb
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
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        § spacer"
            .unindent(),
        &mut cx,
    );

    let buffer_snapshot = buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(0, 3)..Point::new(0, 3), "\n")], None, cx);
        buffer.text_snapshot()
    });
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
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        § spacer
        bbb
        § spacer"
            .unindent(),
        &mut cx,
    );

    let buffer_snapshot = buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(0, 3)..Point::new(1, 0), "")], None, cx);
        buffer.text_snapshot()
    });
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
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        aaa
        bbb
        § spacer"
            .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_act_as_type(cx: &mut gpui::TestAppContext) {
    let (splittable_editor, cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;
    let editor = splittable_editor.read_with(cx, |editor, cx| {
        editor.act_as_type(TypeId::of::<Editor>(), &splittable_editor, cx)
    });

    assert!(
        editor.is_some(),
        "SplittableEditor should be able to act as Editor"
    );
}
