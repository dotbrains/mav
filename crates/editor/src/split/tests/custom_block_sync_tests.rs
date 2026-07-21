use super::*;

#[gpui::test]
async fn test_custom_block_sync_between_split_views(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let base_text = "
        bbb
        ccc
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
        § spacer
        bbb
        ccc"
        .unindent(),
        &mut cx,
    );

    let block_ids = editor.update(cx, |splittable_editor, cx| {
        splittable_editor.rhs_editor.update(cx, |rhs_editor, cx| {
            let snapshot = rhs_editor.buffer().read(cx).snapshot(cx);
            let anchor = snapshot.anchor_before(Point::new(2, 0));
            rhs_editor.insert_blocks(
                [BlockProperties {
                    placement: BlockPlacement::Above(anchor),
                    height: Some(1),
                    style: BlockStyle::Fixed,
                    render: Arc::new(|_| div().into_any()),
                    priority: 0,
                }],
                None,
                cx,
            )
        })
    });

    let rhs_editor = editor.read_with(cx, |editor, _| editor.rhs_editor.clone());
    let lhs_editor = editor.read_with(cx, |editor, _| editor.lhs.as_ref().unwrap().editor.clone());

    cx.update(|_, cx| {
        set_block_content_for_tests(&rhs_editor, block_ids[0], cx, |_| {
            "custom block".to_string()
        });
    });

    let lhs_block_id = lhs_editor.read_with(cx, |lhs_editor, cx| {
        let display_map = lhs_editor.display_map.read(cx);
        let companion = display_map.companion().unwrap().read(cx);
        let mapping =
            companion.custom_block_to_balancing_block(rhs_editor.read(cx).display_map.entity_id());
        *mapping.borrow().get(&block_ids[0]).unwrap()
    });

    cx.update(|_, cx| {
        set_block_content_for_tests(&lhs_editor, lhs_block_id, cx, |_| {
            "custom block".to_string()
        });
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaa
        bbb
        § custom block
        ccc"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        bbb
        § custom block
        ccc"
        .unindent(),
        &mut cx,
    );

    editor.update(cx, |splittable_editor, cx| {
        splittable_editor.rhs_editor.update(cx, |rhs_editor, cx| {
            rhs_editor.remove_blocks(HashSet::from_iter(block_ids), None, cx);
        });
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
        § spacer
        bbb
        ccc"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_custom_block_deletion_and_resplit_sync(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let base_text = "
        bbb
        ccc
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
        § spacer
        bbb
        ccc"
        .unindent(),
        &mut cx,
    );

    let block_ids = editor.update(cx, |splittable_editor, cx| {
        splittable_editor.rhs_editor.update(cx, |rhs_editor, cx| {
            let snapshot = rhs_editor.buffer().read(cx).snapshot(cx);
            let anchor1 = snapshot.anchor_before(Point::new(2, 0));
            let anchor2 = snapshot.anchor_before(Point::new(3, 0));
            rhs_editor.insert_blocks(
                [
                    BlockProperties {
                        placement: BlockPlacement::Above(anchor1),
                        height: Some(1),
                        style: BlockStyle::Fixed,
                        render: Arc::new(|_| div().into_any()),
                        priority: 0,
                    },
                    BlockProperties {
                        placement: BlockPlacement::Above(anchor2),
                        height: Some(1),
                        style: BlockStyle::Fixed,
                        render: Arc::new(|_| div().into_any()),
                        priority: 0,
                    },
                ],
                None,
                cx,
            )
        })
    });

    let rhs_editor = editor.read_with(cx, |editor, _| editor.rhs_editor.clone());
    let lhs_editor = editor.read_with(cx, |editor, _| editor.lhs.as_ref().unwrap().editor.clone());

    cx.update(|_, cx| {
        set_block_content_for_tests(&rhs_editor, block_ids[0], cx, |_| {
            "custom block 1".to_string()
        });
        set_block_content_for_tests(&rhs_editor, block_ids[1], cx, |_| {
            "custom block 2".to_string()
        });
    });

    let (lhs_block_id_1, lhs_block_id_2) = lhs_editor.read_with(cx, |lhs_editor, cx| {
        let display_map = lhs_editor.display_map.read(cx);
        let companion = display_map.companion().unwrap().read(cx);
        let mapping =
            companion.custom_block_to_balancing_block(rhs_editor.read(cx).display_map.entity_id());
        (
            *mapping.borrow().get(&block_ids[0]).unwrap(),
            *mapping.borrow().get(&block_ids[1]).unwrap(),
        )
    });

    cx.update(|_, cx| {
        set_block_content_for_tests(&lhs_editor, lhs_block_id_1, cx, |_| {
            "custom block 1".to_string()
        });
        set_block_content_for_tests(&lhs_editor, lhs_block_id_2, cx, |_| {
            "custom block 2".to_string()
        });
    });

    cx.run_until_parked();

    assert_split_content(
        &editor,
        "
        § <no file>
        § -----
        aaa
        bbb
        § custom block 1
        ccc
        § custom block 2"
            .unindent(),
        "
        § <no file>
        § -----
        § spacer
        bbb
        § custom block 1
        ccc
        § custom block 2"
            .unindent(),
        &mut cx,
    );

    editor.update(cx, |splittable_editor, cx| {
        splittable_editor.rhs_editor.update(cx, |rhs_editor, cx| {
            rhs_editor.remove_blocks(HashSet::from_iter([block_ids[0]]), None, cx);
        });
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
        § custom block 2"
            .unindent(),
        "
        § <no file>
        § -----
        § spacer
        bbb
        ccc
        § custom block 2"
            .unindent(),
        &mut cx,
    );

    editor.update_in(cx, |splittable_editor, window, cx| {
        splittable_editor.unsplit(window, cx);
    });

    cx.run_until_parked();

    editor.update_in(cx, |splittable_editor, window, cx| {
        splittable_editor.split(window, cx);
    });

    cx.run_until_parked();

    let lhs_editor = editor.read_with(cx, |editor, _| editor.lhs.as_ref().unwrap().editor.clone());

    let lhs_block_id_2 = lhs_editor.read_with(cx, |lhs_editor, cx| {
        let display_map = lhs_editor.display_map.read(cx);
        let companion = display_map.companion().unwrap().read(cx);
        let mapping =
            companion.custom_block_to_balancing_block(rhs_editor.read(cx).display_map.entity_id());
        *mapping.borrow().get(&block_ids[1]).unwrap()
    });

    cx.update(|_, cx| {
        set_block_content_for_tests(&lhs_editor, lhs_block_id_2, cx, |_| {
            "custom block 2".to_string()
        });
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
        § custom block 2"
            .unindent(),
        "
        § <no file>
        § -----
        § spacer
        bbb
        ccc
        § custom block 2"
            .unindent(),
        &mut cx,
    );
}
