use super::*;

#[gpui::test]
async fn test_edit_spanning_excerpt_boundaries_then_resplit(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let base_text = "
        aaa
        bbb
        ccc
        ddd
        eee
        fff
        ggg
        hhh
        iii
        jjj
        kkk
        lll
    "
    .unindent();
    let current_text = base_text.clone();

    let (buffer, diff) = buffer_with_diff(&base_text, &current_text, &mut cx);

    editor.update(cx, |editor, cx| {
        let path = PathKey::sorted(0);
        editor.update_excerpts_for_path(
            path,
            buffer.clone(),
            vec![
                Point::new(0, 0)..Point::new(3, 3),
                Point::new(5, 0)..Point::new(8, 3),
                Point::new(10, 0)..Point::new(11, 3),
            ],
            0,
            diff.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 0)..Point::new(10, 0), "")], None, cx);
    });

    cx.run_until_parked();

    editor.update_in(cx, |splittable_editor, window, cx| {
        splittable_editor.unsplit(window, cx);
    });

    cx.run_until_parked();

    editor.update_in(cx, |splittable_editor, window, cx| {
        splittable_editor.split(window, cx);
    });

    cx.run_until_parked();
}

#[gpui::test]
async fn test_range_folds_removed_on_split(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Unified).await;

    let base_text = "
        aaa
        bbb
        ccc
        ddd
        eee"
    .unindent();
    let current_text = "
        aaa
        bbb
        ccc
        ddd
        eee"
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

    editor.update_in(cx, |editor, window, cx| {
        editor.rhs_editor.update(cx, |rhs_editor, cx| {
            rhs_editor.fold_creases(
                vec![Crease::simple(
                    Point::new(1, 0)..Point::new(3, 0),
                    FoldPlaceholder::test(),
                )],
                false,
                window,
                cx,
            );
        });
    });

    cx.run_until_parked();

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

    let rhs_has_folds_after_split = rhs_editor.update(cx, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        snapshot
            .folds_in_range(MultiBufferOffset(0)..snapshot.buffer_snapshot().len())
            .next()
            .is_some()
    });
    assert!(
        !rhs_has_folds_after_split,
        "rhs should not have range folds after split"
    );

    let lhs_has_folds = lhs_editor.update(cx, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        snapshot
            .folds_in_range(MultiBufferOffset(0)..snapshot.buffer_snapshot().len())
            .next()
            .is_some()
    });
    assert!(!lhs_has_folds, "lhs should not have any range folds");
}

#[gpui::test]
async fn test_multiline_inlays_create_spacers(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let base_text = "
        aaa
        bbb
        ccc
        ddd
    "
    .unindent();
    let current_text = base_text.clone();

    let (buffer, diff) = buffer_with_diff(&base_text, &current_text, &mut cx);

    editor.update(cx, |editor, cx| {
        let path = PathKey::sorted(0);
        editor.update_excerpts_for_path(
            path,
            buffer.clone(),
            vec![Point::new(0, 0)..Point::new(3, 3)],
            0,
            diff.clone(),
            cx,
        );
    });

    cx.run_until_parked();

    let rhs_editor = editor.read_with(cx, |e, _| e.rhs_editor.clone());
    rhs_editor.update(cx, |rhs_editor, cx| {
        let snapshot = rhs_editor.buffer().read(cx).snapshot(cx);
        rhs_editor.splice_inlays(
            &[],
            vec![
                Inlay::edit_prediction(
                    0,
                    snapshot.anchor_after(Point::new(0, 3)),
                    "\nINLAY_WITHIN",
                ),
                Inlay::edit_prediction(
                    1,
                    snapshot.anchor_after(Point::new(1, 3)),
                    "\nINLAY_MID_1\nINLAY_MID_2",
                ),
                Inlay::edit_prediction(
                    2,
                    snapshot.anchor_after(Point::new(3, 3)),
                    "\nINLAY_END_1\nINLAY_END_2",
                ),
            ],
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
        INLAY_WITHIN
        bbb
        INLAY_MID_1
        INLAY_MID_2
        ccc
        ddd
        INLAY_END_1
        INLAY_END_2"
            .unindent(),
        "
        § <no file>
        § -----
        aaa
        § spacer
        bbb
        § spacer
        § spacer
        ccc
        ddd
        § spacer
        § spacer"
            .unindent(),
        &mut cx,
    );
}
