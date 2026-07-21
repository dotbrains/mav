use super::*;

#[gpui::test]
async fn test_buffer_folding_sync(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Unified).await;

    let base_text1 = "
        aaa
        bbb
        ccc"
    .unindent();
    let current_text1 = "
        aaa
        bbb
        ccc"
    .unindent();

    let base_text2 = "
        ddd
        eee
        fff"
    .unindent();
    let current_text2 = "
        ddd
        eee
        fff"
    .unindent();

    let (buffer1, diff1) = buffer_with_diff(&base_text1, &current_text1, &mut cx);
    let (buffer2, diff2) = buffer_with_diff(&base_text2, &current_text2, &mut cx);

    let buffer1_id = buffer1.read_with(cx, |buffer, _| buffer.remote_id());
    let buffer2_id = buffer2.read_with(cx, |buffer, _| buffer.remote_id());

    editor.update(cx, |editor, cx| {
        editor.update_excerpts_for_path(
            PathKey::sorted(0),
            buffer1.clone(),
            vec![Point::new(0, 0)..buffer1.read(cx).max_point()],
            0,
            diff1.clone(),
            cx,
        );
        editor.update_excerpts_for_path(
            PathKey::sorted(1),
            buffer2.clone(),
            vec![Point::new(0, 0)..buffer2.read(cx).max_point()],
            1,
            diff2.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    editor.update(cx, |editor, cx| {
        editor.rhs_editor.update(cx, |rhs_editor, cx| {
            rhs_editor.fold_buffer(buffer1_id, cx);
        });
    });

    cx.run_until_parked();

    let rhs_buffer1_folded = editor.read_with(cx, |editor, cx| {
        editor.rhs_editor.read(cx).is_buffer_folded(buffer1_id, cx)
    });
    assert!(
        rhs_buffer1_folded,
        "buffer1 should be folded in rhs before split"
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.split(window, cx);
    });

    cx.run_until_parked();

    let (rhs_editor, lhs_editor) = editor.read_with(cx, |editor, _cx| {
        (
            editor.rhs_editor.clone(),
            editor.lhs.as_ref().unwrap().editor.clone(),
        )
    });

    let rhs_buffer1_folded =
        rhs_editor.read_with(cx, |editor, cx| editor.is_buffer_folded(buffer1_id, cx));
    assert!(
        rhs_buffer1_folded,
        "buffer1 should be folded in rhs after split"
    );

    let base_buffer1_id = diff1.read_with(cx, |diff, cx| diff.base_text(cx).remote_id());
    let lhs_buffer1_folded = lhs_editor.read_with(cx, |editor, cx| {
        editor.is_buffer_folded(base_buffer1_id, cx)
    });
    assert!(
        lhs_buffer1_folded,
        "buffer1 should be folded in lhs after split"
    );

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        § <no file>
        § -----
        ddd
        eee
        fff"
        .unindent(),
        "
        § <no file>
        § -----
        § <no file>
        § -----
        ddd
        eee
        fff"
        .unindent(),
        &mut cx,
    );

    editor.update(cx, |editor, cx| {
        editor.rhs_editor.update(cx, |rhs_editor, cx| {
            rhs_editor.fold_buffer(buffer2_id, cx);
        });
    });

    cx.run_until_parked();

    let rhs_buffer2_folded =
        rhs_editor.read_with(cx, |editor, cx| editor.is_buffer_folded(buffer2_id, cx));
    assert!(rhs_buffer2_folded, "buffer2 should be folded in rhs");

    let base_buffer2_id = diff2.read_with(cx, |diff, cx| diff.base_text(cx).remote_id());
    let lhs_buffer2_folded = lhs_editor.read_with(cx, |editor, cx| {
        editor.is_buffer_folded(base_buffer2_id, cx)
    });
    assert!(lhs_buffer2_folded, "buffer2 should be folded in lhs");

    let rhs_buffer1_still_folded =
        rhs_editor.read_with(cx, |editor, cx| editor.is_buffer_folded(buffer1_id, cx));
    assert!(
        rhs_buffer1_still_folded,
        "buffer1 should still be folded in rhs"
    );

    let lhs_buffer1_still_folded = lhs_editor.read_with(cx, |editor, cx| {
        editor.is_buffer_folded(base_buffer1_id, cx)
    });
    assert!(
        lhs_buffer1_still_folded,
        "buffer1 should still be folded in lhs"
    );

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        § <no file>
        § -----"
            .unindent(),
        "
        § <no file>
        § -----
        § <no file>
        § -----"
            .unindent(),
        &mut cx,
    );
}
