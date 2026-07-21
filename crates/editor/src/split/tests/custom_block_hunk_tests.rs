use super::*;

#[gpui::test]
async fn test_custom_block_in_middle_of_added_hunk(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let base_text = "
        ddd
        eee
    "
    .unindent();
    let current_text = "
        aaa
        bbb
        ccc
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
        bbb
        ccc
        ddd
        eee"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer
        ddd
        eee"
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
        ccc
        ddd
        eee"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer
        § custom block
        ddd
        eee"
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
        ccc
        ddd
        eee"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer
        ddd
        eee"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_custom_block_below_in_middle_of_added_hunk(cx: &mut gpui::TestAppContext) {
    use rope::Point;
    use unindent::Unindent as _;

    let (editor, mut cx) = init_test(cx, SoftWrap::None, DiffViewStyle::Split).await;

    let base_text = "
        ddd
        eee
    "
    .unindent();
    let current_text = "
        aaa
        bbb
        ccc
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
        bbb
        ccc
        ddd
        eee"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer
        ddd
        eee"
        .unindent(),
        &mut cx,
    );

    let block_ids = editor.update(cx, |splittable_editor, cx| {
        splittable_editor.rhs_editor.update(cx, |rhs_editor, cx| {
            let snapshot = rhs_editor.buffer().read(cx).snapshot(cx);
            let anchor = snapshot.anchor_after(Point::new(1, 3));
            rhs_editor.insert_blocks(
                [BlockProperties {
                    placement: BlockPlacement::Below(anchor),
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
        ccc
        ddd
        eee"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer
        § custom block
        ddd
        eee"
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
        ccc
        ddd
        eee"
        .unindent(),
        "
        § <no file>
        § -----
        § spacer
        § spacer
        § spacer
        ddd
        eee"
        .unindent(),
        &mut cx,
    );
}

#[gpui::test]
async fn test_custom_block_resize_syncs_balancing_block(cx: &mut gpui::TestAppContext) {
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

    let lhs_block_id = lhs_editor.read_with(cx, |lhs_editor, cx| {
        let display_map = lhs_editor.display_map.read(cx);
        let companion = display_map.companion().unwrap().read(cx);
        let mapping =
            companion.custom_block_to_balancing_block(rhs_editor.read(cx).display_map.entity_id());
        *mapping.borrow().get(&block_ids[0]).unwrap()
    });

    cx.run_until_parked();

    let get_block_height = |editor: &Entity<crate::Editor>,
                            block_id: crate::CustomBlockId,
                            cx: &mut VisualTestContext| {
        editor.update_in(cx, |editor, window, cx| {
            let snapshot = editor.snapshot(window, cx);
            snapshot
                .block_for_id(crate::BlockId::Custom(block_id))
                .map(|block| block.height())
        })
    };

    assert_eq!(
        get_block_height(&rhs_editor, block_ids[0], &mut cx),
        Some(1)
    );
    assert_eq!(
        get_block_height(&lhs_editor, lhs_block_id, &mut cx),
        Some(1)
    );

    editor.update(cx, |splittable_editor, cx| {
        splittable_editor.rhs_editor.update(cx, |rhs_editor, cx| {
            let mut heights = HashMap::default();
            heights.insert(block_ids[0], 3);
            rhs_editor.resize_blocks(heights, None, cx);
        });
    });

    cx.run_until_parked();

    assert_eq!(
        get_block_height(&rhs_editor, block_ids[0], &mut cx),
        Some(3)
    );
    assert_eq!(
        get_block_height(&lhs_editor, lhs_block_id, &mut cx),
        Some(3)
    );

    editor.update(cx, |splittable_editor, cx| {
        splittable_editor.rhs_editor.update(cx, |rhs_editor, cx| {
            let mut heights = HashMap::default();
            heights.insert(block_ids[0], 5);
            rhs_editor.resize_blocks(heights, None, cx);
        });
    });

    cx.run_until_parked();

    assert_eq!(
        get_block_height(&rhs_editor, block_ids[0], &mut cx),
        Some(5)
    );
    assert_eq!(
        get_block_height(&lhs_editor, lhs_block_id, &mut cx),
        Some(5)
    );
}
