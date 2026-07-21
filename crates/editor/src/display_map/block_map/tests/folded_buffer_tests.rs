use super::*;

#[gpui::test]
fn test_custom_blocks_inside_buffer_folds(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "111\n\n222\n\n333\n\n444\n\n555\n\n666";

    let buffer = cx.update(|cx| {
        let multibuffer = MultiBuffer::build_multi(
            [
                (text, vec![Point::new(0, 0)..Point::new(0, 3)]),
                (
                    text,
                    vec![
                        Point::new(2, 0)..Point::new(2, 3),
                        Point::new(4, 0)..Point::new(4, 3),
                        Point::new(6, 0)..Point::new(6, 3),
                    ],
                ),
                (
                    text,
                    vec![
                        Point::new(8, 0)..Point::new(8, 3),
                        Point::new(10, 0)..Point::new(10, 3),
                    ],
                ),
            ],
            cx,
        );
        assert_eq!(multibuffer.read(cx).snapshot(cx).excerpts().count(), 6);
        multibuffer
    });
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let buffer_ids = buffer_snapshot
        .excerpts()
        .map(|excerpt| excerpt.context.start.buffer_id)
        .dedup()
        .collect::<Vec<_>>();
    assert_eq!(buffer_ids.len(), 3);
    let buffer_id_1 = buffer_ids[0];
    let buffer_id_2 = buffer_ids[1];
    let buffer_id_3 = buffer_ids[2];

    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (_, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let (_, wrap_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let mut block_map = BlockMap::new(wrap_snapshot.clone(), 2, 1);
    let blocks_snapshot = block_map.read(wrap_snapshot.clone(), Patch::default(), None);

    assert_eq!(
        blocks_snapshot.text(),
        "\n\n111\n\n\n222\n\n333\n\n444\n\n\n555\n\n666"
    );
    assert_eq!(
        blocks_snapshot
            .row_infos(BlockRow(0))
            .map(|i| i.buffer_row)
            .collect::<Vec<_>>(),
        vec![
            None,
            None,
            Some(0),
            None,
            None,
            Some(2),
            None,
            Some(4),
            None,
            Some(6),
            None,
            None,
            Some(8),
            None,
            Some(10),
        ]
    );

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    let excerpt_blocks_2 = writer.insert(vec![
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(1, 0))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(2, 0))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Below(buffer_snapshot.anchor_after(Point::new(3, 0))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
    ]);
    let excerpt_blocks_3 = writer.insert(vec![
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(4, 0))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Below(buffer_snapshot.anchor_after(Point::new(5, 0))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
    ]);

    let blocks_snapshot = block_map.read(wrap_snapshot.clone(), Patch::default(), None);
    assert_eq!(
        blocks_snapshot.text(),
        "\n\n111\n\n\n\n222\n\n\n333\n\n444\n\n\n\n\n555\n\n666\n"
    );
    assert_eq!(
        blocks_snapshot
            .row_infos(BlockRow(0))
            .map(|i| i.buffer_row)
            .collect::<Vec<_>>(),
        vec![
            None,
            None,
            Some(0),
            None,
            None,
            None,
            Some(2),
            None,
            None,
            Some(4),
            None,
            Some(6),
            None,
            None,
            None,
            None,
            Some(8),
            None,
            Some(10),
            None,
        ]
    );

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    buffer.read_with(cx, |buffer, cx| {
        writer.fold_buffers([buffer_id_1], buffer, cx);
    });
    let excerpt_blocks_1 = writer.insert(vec![BlockProperties {
        style: BlockStyle::Fixed,
        placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(0, 0))),
        height: Some(1),
        render: Arc::new(|_| div().into_any()),
        priority: 0,
    }]);
    let blocks_snapshot = block_map.read(wrap_snapshot.clone(), Patch::default(), None);
    let blocks = blocks_snapshot
        .blocks_in_range(BlockRow(0)..BlockRow(u32::MAX))
        .collect::<Vec<_>>();
    for (_, block) in &blocks {
        if let BlockId::Custom(custom_block_id) = block.id() {
            assert!(
                !excerpt_blocks_1.contains(&custom_block_id),
                "Should have no blocks from the folded buffer"
            );
            assert!(
                excerpt_blocks_2.contains(&custom_block_id)
                    || excerpt_blocks_3.contains(&custom_block_id),
                "Should have only blocks from unfolded buffers"
            );
        }
    }
    assert_eq!(
        1,
        blocks
            .iter()
            .filter(|(_, block)| matches!(block, Block::FoldedBuffer { .. }))
            .count(),
        "Should have one folded block, producing a header of the second buffer"
    );
    assert_eq!(
        blocks_snapshot.text(),
        "\n\n\n\n\n222\n\n\n333\n\n444\n\n\n\n\n555\n\n666\n"
    );
    assert_eq!(
        blocks_snapshot
            .row_infos(BlockRow(0))
            .map(|i| i.buffer_row)
            .collect::<Vec<_>>(),
        vec![
            None,
            None,
            None,
            None,
            None,
            Some(2),
            None,
            None,
            Some(4),
            None,
            Some(6),
            None,
            None,
            None,
            None,
            Some(8),
            None,
            Some(10),
            None,
        ]
    );

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    buffer.read_with(cx, |buffer, cx| {
        writer.fold_buffers([buffer_id_2], buffer, cx);
    });
    let blocks_snapshot = block_map.read(wrap_snapshot.clone(), Patch::default(), None);
    let blocks = blocks_snapshot
        .blocks_in_range(BlockRow(0)..BlockRow(u32::MAX))
        .collect::<Vec<_>>();
    for (_, block) in &blocks {
        if let BlockId::Custom(custom_block_id) = block.id() {
            assert!(
                !excerpt_blocks_1.contains(&custom_block_id),
                "Should have no blocks from the folded buffer_1"
            );
            assert!(
                !excerpt_blocks_2.contains(&custom_block_id),
                "Should have no blocks from the folded buffer_2"
            );
            assert!(
                excerpt_blocks_3.contains(&custom_block_id),
                "Should have only blocks from unfolded buffers"
            );
        }
    }
    assert_eq!(
        2,
        blocks
            .iter()
            .filter(|(_, block)| matches!(block, Block::FoldedBuffer { .. }))
            .count(),
        "Should have two folded blocks, producing headers"
    );
    assert_eq!(blocks_snapshot.text(), "\n\n\n\n\n\n\n555\n\n666\n");
    assert_eq!(
        blocks_snapshot
            .row_infos(BlockRow(0))
            .map(|i| i.buffer_row)
            .collect::<Vec<_>>(),
        vec![
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(8),
            None,
            Some(10),
            None,
        ]
    );

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    buffer.read_with(cx, |buffer, cx| {
        writer.unfold_buffers([buffer_id_1], buffer, cx);
    });
    let blocks_snapshot = block_map.read(wrap_snapshot.clone(), Patch::default(), None);
    let blocks = blocks_snapshot
        .blocks_in_range(BlockRow(0)..BlockRow(u32::MAX))
        .collect::<Vec<_>>();
    for (_, block) in &blocks {
        if let BlockId::Custom(custom_block_id) = block.id() {
            assert!(
                !excerpt_blocks_2.contains(&custom_block_id),
                "Should have no blocks from the folded buffer_2"
            );
            assert!(
                excerpt_blocks_1.contains(&custom_block_id)
                    || excerpt_blocks_3.contains(&custom_block_id),
                "Should have only blocks from unfolded buffers"
            );
        }
    }
    assert_eq!(
        1,
        blocks
            .iter()
            .filter(|(_, block)| matches!(block, Block::FoldedBuffer { .. }))
            .count(),
        "Should be back to a single folded buffer, producing a header for buffer_2"
    );
    assert_eq!(
        blocks_snapshot.text(),
        "\n\n\n111\n\n\n\n\n\n555\n\n666\n",
        "Should have extra newline for 111 buffer, due to a new block added when it was folded"
    );
    assert_eq!(
        blocks_snapshot
            .row_infos(BlockRow(0))
            .map(|i| i.buffer_row)
            .collect::<Vec<_>>(),
        vec![
            None,
            None,
            None,
            Some(0),
            None,
            None,
            None,
            None,
            None,
            Some(8),
            None,
            Some(10),
            None,
        ]
    );

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    buffer.read_with(cx, |buffer, cx| {
        writer.fold_buffers([buffer_id_3], buffer, cx);
    });
    let blocks_snapshot = block_map.read(wrap_snapshot, Patch::default(), None);
    let blocks = blocks_snapshot
        .blocks_in_range(BlockRow(0)..BlockRow(u32::MAX))
        .collect::<Vec<_>>();
    for (_, block) in &blocks {
        if let BlockId::Custom(custom_block_id) = block.id() {
            assert!(
                excerpt_blocks_1.contains(&custom_block_id),
                "Should have no blocks from the folded buffer_1"
            );
            assert!(
                !excerpt_blocks_2.contains(&custom_block_id),
                "Should have only blocks from unfolded buffers"
            );
            assert!(
                !excerpt_blocks_3.contains(&custom_block_id),
                "Should have only blocks from unfolded buffers"
            );
        }
    }

    assert_eq!(
        blocks_snapshot.text(),
        "\n\n\n111\n\n\n\n",
        "Should have a single, first buffer left after folding"
    );
    assert_eq!(
        blocks_snapshot
            .row_infos(BlockRow(0))
            .map(|i| i.buffer_row)
            .collect::<Vec<_>>(),
        vec![None, None, None, Some(0), None, None, None, None,]
    );
}

#[gpui::test]
fn test_basic_buffer_fold(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "111";

    let buffer = cx.update(|cx| {
        MultiBuffer::build_multi([(text, vec![Point::new(0, 0)..Point::new(0, 3)])], cx)
    });
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let buffer_ids = buffer_snapshot
        .excerpts()
        .map(|excerpt| excerpt.context.start.buffer_id)
        .dedup()
        .collect::<Vec<_>>();
    assert_eq!(buffer_ids.len(), 1);
    let buffer_id = buffer_ids[0];

    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot);
    let (_, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let (_, wrap_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let mut block_map = BlockMap::new(wrap_snapshot.clone(), 2, 1);
    let blocks_snapshot = block_map.read(wrap_snapshot.clone(), Patch::default(), None);

    assert_eq!(blocks_snapshot.text(), "\n\n111");

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    buffer.read_with(cx, |buffer, cx| {
        writer.fold_buffers([buffer_id], buffer, cx);
    });
    let blocks_snapshot = block_map.read(wrap_snapshot, Patch::default(), None);
    let blocks = blocks_snapshot
        .blocks_in_range(BlockRow(0)..BlockRow(u32::MAX))
        .collect::<Vec<_>>();
    assert_eq!(
        1,
        blocks
            .iter()
            .filter(|(_, block)| { matches!(block, Block::FoldedBuffer { .. }) })
            .count(),
        "Should have one folded block, producing a header of the second buffer"
    );
    assert_eq!(blocks_snapshot.text(), "\n");
    assert_eq!(
        blocks_snapshot
            .row_infos(BlockRow(0))
            .map(|i| i.buffer_row)
            .collect::<Vec<_>>(),
        vec![None, None],
        "When fully folded, should be no buffer rows"
    );
}
