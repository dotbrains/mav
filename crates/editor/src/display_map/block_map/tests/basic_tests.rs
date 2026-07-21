use super::*;

#[gpui::test]
fn test_offset_for_row() {
    assert_eq!(offset_for_row("", RowDelta(0)), (RowDelta(0), 0));
    assert_eq!(offset_for_row("", RowDelta(1)), (RowDelta(0), 0));
    assert_eq!(offset_for_row("abcd", RowDelta(0)), (RowDelta(0), 0));
    assert_eq!(offset_for_row("abcd", RowDelta(1)), (RowDelta(0), 4));
    assert_eq!(offset_for_row("\n", RowDelta(0)), (RowDelta(0), 0));
    assert_eq!(offset_for_row("\n", RowDelta(1)), (RowDelta(1), 1));
    assert_eq!(
        offset_for_row("abc\ndef\nghi", RowDelta(0)),
        (RowDelta(0), 0)
    );
    assert_eq!(
        offset_for_row("abc\ndef\nghi", RowDelta(1)),
        (RowDelta(1), 4)
    );
    assert_eq!(
        offset_for_row("abc\ndef\nghi", RowDelta(2)),
        (RowDelta(2), 8)
    );
    assert_eq!(
        offset_for_row("abc\ndef\nghi", RowDelta(3)),
        (RowDelta(2), 11)
    );
}

#[gpui::test]
fn test_basic_blocks(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "aaa\nbbb\nccc\nddd";

    let buffer = cx.update(|cx| MultiBuffer::build_simple(text, cx));
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let subscription = buffer.update(cx, |buffer, _| buffer.subscribe());
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (mut fold_map, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (mut tab_map, tab_snapshot) = TabMap::new(fold_snapshot, 1.try_into().unwrap());
    let (wrap_map, wraps_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let mut block_map = BlockMap::new(wraps_snapshot.clone(), 1, 1);

    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    let block_ids = writer.insert(vec![
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(1, 0))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(1, 2))),
            height: Some(2),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Below(buffer_snapshot.anchor_after(Point::new(3, 3))),
            height: Some(3),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
    ]);

    let snapshot = block_map.read(wraps_snapshot, Default::default(), None);
    assert_eq!(snapshot.text(), "aaa\n\n\n\nbbb\nccc\nddd\n\n\n");

    let blocks = snapshot
        .blocks_in_range(BlockRow(0)..BlockRow(8))
        .map(|(start_row, block)| {
            let block = block.as_custom().unwrap();
            (start_row.0..start_row.0 + block.height.unwrap(), block.id)
        })
        .collect::<Vec<_>>();

    // When multiple blocks are on the same line, the newer blocks appear first.
    assert_eq!(
        blocks,
        &[
            (1..2, block_ids[0]),
            (2..4, block_ids[1]),
            (7..10, block_ids[2]),
        ]
    );

    assert_eq!(
        snapshot.to_block_point(WrapPoint::new(WrapRow(0), 3)),
        BlockPoint::new(BlockRow(0), 3)
    );
    assert_eq!(
        snapshot.to_block_point(WrapPoint::new(WrapRow(1), 0)),
        BlockPoint::new(BlockRow(4), 0)
    );
    assert_eq!(
        snapshot.to_block_point(WrapPoint::new(WrapRow(3), 3)),
        BlockPoint::new(BlockRow(6), 3)
    );

    assert_eq!(
        snapshot.to_wrap_point(BlockPoint::new(BlockRow(0), 3), Bias::Left),
        WrapPoint::new(WrapRow(0), 3)
    );
    assert_eq!(
        snapshot.to_wrap_point(BlockPoint::new(BlockRow(1), 0), Bias::Left),
        WrapPoint::new(WrapRow(1), 0)
    );
    assert_eq!(
        snapshot.to_wrap_point(BlockPoint::new(BlockRow(3), 0), Bias::Left),
        WrapPoint::new(WrapRow(1), 0)
    );
    assert_eq!(
        snapshot.to_wrap_point(BlockPoint::new(BlockRow(7), 0), Bias::Left),
        WrapPoint::new(WrapRow(3), 3)
    );

    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(1), 0), Bias::Left),
        BlockPoint::new(BlockRow(0), 3)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(1), 0), Bias::Right),
        BlockPoint::new(BlockRow(4), 0)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(1), 1), Bias::Left),
        BlockPoint::new(BlockRow(0), 3)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(1), 1), Bias::Right),
        BlockPoint::new(BlockRow(4), 0)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(4), 0), Bias::Left),
        BlockPoint::new(BlockRow(4), 0)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(4), 0), Bias::Right),
        BlockPoint::new(BlockRow(4), 0)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(6), 3), Bias::Left),
        BlockPoint::new(BlockRow(6), 3)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(6), 3), Bias::Right),
        BlockPoint::new(BlockRow(6), 3)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(7), 0), Bias::Left),
        BlockPoint::new(BlockRow(6), 3)
    );
    assert_eq!(
        snapshot.clip_point(BlockPoint::new(BlockRow(7), 0), Bias::Right),
        BlockPoint::new(BlockRow(6), 3)
    );

    assert_eq!(
        snapshot
            .row_infos(BlockRow(0))
            .map(|row_info| row_info.buffer_row)
            .collect::<Vec<_>>(),
        &[
            Some(0),
            None,
            None,
            None,
            Some(1),
            Some(2),
            Some(3),
            None,
            None,
            None
        ]
    );

    // Insert a line break, separating two block decorations into separate lines.
    let buffer_snapshot = buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 1)..Point::new(1, 1), "!!!\n")], None, cx);
        buffer.snapshot(cx)
    });

    let (inlay_snapshot, inlay_edits) =
        inlay_map.sync(buffer_snapshot, subscription.consume().into_inner());
    let (fold_snapshot, fold_edits) = fold_map.read(inlay_snapshot, inlay_edits);
    let (tab_snapshot, tab_edits) = tab_map.sync(fold_snapshot, fold_edits, 4.try_into().unwrap());
    let (wraps_snapshot, wrap_edits) = wrap_map.update(cx, |wrap_map, cx| {
        wrap_map.sync(tab_snapshot, tab_edits, cx)
    });
    let snapshot = block_map.read(wraps_snapshot, wrap_edits, None);
    assert_eq!(snapshot.text(), "aaa\n\nb!!!\n\n\nbb\nccc\nddd\n\n\n");
}

#[gpui::test]
fn test_blocks_hidden_in_folds(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "line0\nline1\nline2\nline3\nline4";

    let buffer = cx.update(|cx| MultiBuffer::build_simple(text, cx));
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let tab_size = 1.try_into().unwrap();
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (mut fold_map, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (mut tab_map, tab_snapshot) = TabMap::new(fold_snapshot, tab_size);
    let (wrap_map, wraps_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let mut block_map = BlockMap::new(wraps_snapshot.clone(), 1, 1);

    let above = |row| BlockProperties {
        style: BlockStyle::Fixed,
        placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(row, 0))),
        height: Some(1),
        render: Arc::new(|_| div().into_any()),
        priority: 0,
    };
    let below = |row| BlockProperties {
        placement: BlockPlacement::Below(buffer_snapshot.anchor_after(Point::new(row, 0))),
        ..above(row)
    };

    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    let block_ids = writer.insert(vec![above(1), above(2), below(2), above(4)]);
    let (block_a, block_b, block_c, block_d) =
        (block_ids[0], block_ids[1], block_ids[2], block_ids[3]);

    let present_blocks =
        |block_map: &mut BlockMap, wraps_snapshot: WrapSnapshot, wrap_edits: WrapPatch| {
            let snapshot = block_map.read(wraps_snapshot, wrap_edits, None);
            let max_row = snapshot.max_point().row;
            snapshot
                .blocks_in_range(BlockRow(0)..BlockRow(max_row + 1))
                .filter_map(|(_, block)| Some(block.as_custom()?.id))
                .collect::<HashSet<_>>()
        };

    assert_eq!(
        present_blocks(&mut block_map, wraps_snapshot, Default::default()),
        HashSet::from_iter([block_a, block_b, block_c, block_d]),
        "every block is present before folding",
    );

    // Fold lines 2 and 3 entirely, leaving line1 (the fold start) visible.
    let (inlay_snapshot, inlay_edits) = inlay_map.sync(buffer_snapshot.clone(), vec![]);
    let (mut fold_writer, _, _) = fold_map.write(inlay_snapshot, inlay_edits);
    let (fold_snapshot, fold_edits) = fold_writer.fold(vec![(
        Point::new(1, 5)..Point::new(3, 5),
        FoldPlaceholder::test(),
    )]);
    let (tab_snapshot, tab_edits) = tab_map.sync(fold_snapshot, fold_edits, tab_size);
    let (wraps_snapshot, wrap_edits) = wrap_map.update(cx, |wrap_map, cx| {
        wrap_map.sync(tab_snapshot, tab_edits, cx)
    });

    assert_eq!(
        present_blocks(&mut block_map, wraps_snapshot, wrap_edits),
        HashSet::from_iter([block_a, block_d]),
        "blocks B and C anchored to folded lines are dropped, A (fold-start line) and D (past the fold) stay",
    );
}
