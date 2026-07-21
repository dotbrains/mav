use super::*;

#[gpui::test]
fn test_multibuffer_headers_and_footers(cx: &mut App) {
    init_test(cx);

    let buffer1 = cx.new(|cx| Buffer::local("Buffer 1", cx));
    let buffer2 = cx.new(|cx| Buffer::local("Buffer 2", cx));
    let buffer3 = cx.new(|cx| Buffer::local("Buffer 3", cx));

    let multi_buffer = cx.new(|cx| {
        let mut multi_buffer = MultiBuffer::new(Capability::ReadWrite);
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer1.clone(),
            [Point::zero()..buffer1.read(cx).max_point()],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer2.clone(),
            [Point::zero()..buffer2.read(cx).max_point()],
            0,
            cx,
        );
        multi_buffer.set_excerpts_for_path(
            PathKey::sorted(2),
            buffer3.clone(),
            [Point::zero()..buffer3.read(cx).max_point()],
            0,
            cx,
        );
        multi_buffer
    });
    let excerpt_start_anchors = multi_buffer.read_with(cx, |mb, _| {
        let snapshot = mb.snapshot(cx);
        snapshot
            .excerpts()
            .map(|e| snapshot.anchor_in_excerpt(e.context.start).unwrap())
            .collect::<Vec<_>>()
    });

    let font = test_font();
    let font_size = px(14.);
    let font_id = cx.text_system().resolve_font(&font);
    let mut wrap_width = px(0.);
    for c in "Buff".chars() {
        wrap_width += cx
            .text_system()
            .advance(font_id, font_size, c)
            .unwrap()
            .width;
    }

    let multi_buffer_snapshot = multi_buffer.read(cx).snapshot(cx);
    let (_, inlay_snapshot) = InlayMap::new(multi_buffer_snapshot);
    let (_, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let (_, wraps_snapshot) = WrapMap::new(tab_snapshot, font, font_size, Some(wrap_width), cx);

    let block_map = BlockMap::new(wraps_snapshot.clone(), 1, 1);
    let snapshot = block_map.read(wraps_snapshot, Default::default(), None);

    // Each excerpt has a header above and footer below. Excerpts are also *separated* by a newline.
    assert_eq!(snapshot.text(), "\nBuff\ner 1\n\nBuff\ner 2\n\nBuff\ner 3");

    let blocks: Vec<_> = snapshot
        .blocks_in_range(BlockRow(0)..BlockRow(u32::MAX))
        .map(|(row, block)| (row.0..row.0 + block.height(), block.id()))
        .collect();
    assert_eq!(
        blocks,
        vec![
            (0..1, BlockId::ExcerptBoundary(excerpt_start_anchors[0])), // path, header
            (3..4, BlockId::ExcerptBoundary(excerpt_start_anchors[1])), // path, header
            (6..7, BlockId::ExcerptBoundary(excerpt_start_anchors[2])), // path, header
        ]
    );
}

#[gpui::test]
fn test_replace_with_heights(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "aaa\nbbb\nccc\nddd";

    let buffer = cx.update(|cx| MultiBuffer::build_simple(text, cx));
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let _subscription = buffer.update(cx, |buffer, _| buffer.subscribe());
    let (_inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (_fold_map, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_tab_map, tab_snapshot) = TabMap::new(fold_snapshot, 1.try_into().unwrap());
    let (_wrap_map, wraps_snapshot) =
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

    {
        let snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
        assert_eq!(snapshot.text(), "aaa\n\n\n\nbbb\nccc\nddd\n\n\n");

        let mut block_map_writer =
            block_map.write(wraps_snapshot.clone(), Default::default(), None);

        let mut new_heights = HashMap::default();
        new_heights.insert(block_ids[0], 2);
        block_map_writer.resize(new_heights);
        let snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
        assert_eq!(snapshot.text(), "aaa\n\n\n\n\nbbb\nccc\nddd\n\n\n");
    }

    {
        let mut block_map_writer =
            block_map.write(wraps_snapshot.clone(), Default::default(), None);

        let mut new_heights = HashMap::default();
        new_heights.insert(block_ids[0], 1);
        block_map_writer.resize(new_heights);

        let snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
        assert_eq!(snapshot.text(), "aaa\n\n\n\nbbb\nccc\nddd\n\n\n");
    }

    {
        let mut block_map_writer =
            block_map.write(wraps_snapshot.clone(), Default::default(), None);

        let mut new_heights = HashMap::default();
        new_heights.insert(block_ids[0], 0);
        block_map_writer.resize(new_heights);

        let snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
        assert_eq!(snapshot.text(), "aaa\n\n\nbbb\nccc\nddd\n\n\n");
    }

    {
        let mut block_map_writer =
            block_map.write(wraps_snapshot.clone(), Default::default(), None);

        let mut new_heights = HashMap::default();
        new_heights.insert(block_ids[0], 3);
        block_map_writer.resize(new_heights);

        let snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
        assert_eq!(snapshot.text(), "aaa\n\n\n\n\n\nbbb\nccc\nddd\n\n\n");
    }

    {
        let mut block_map_writer =
            block_map.write(wraps_snapshot.clone(), Default::default(), None);

        let mut new_heights = HashMap::default();
        new_heights.insert(block_ids[0], 3);
        block_map_writer.resize(new_heights);

        let snapshot = block_map.read(wraps_snapshot, Default::default(), None);
        // Same height as before, should remain the same
        assert_eq!(snapshot.text(), "aaa\n\n\n\n\n\nbbb\nccc\nddd\n\n\n");
    }
}

#[gpui::test]
fn test_blocks_on_wrapped_lines(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "one two three\nfour five six\nseven eight";

    let buffer = cx.update(|cx| MultiBuffer::build_simple(text, cx));
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (_, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let (_, wraps_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), Some(px(90.)), cx));
    let mut block_map = BlockMap::new(wraps_snapshot.clone(), 1, 1);

    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    writer.insert(vec![
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(1, 12))),
            render: Arc::new(|_| div().into_any()),
            height: Some(1),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Below(buffer_snapshot.anchor_after(Point::new(1, 1))),
            render: Arc::new(|_| div().into_any()),
            height: Some(1),
            priority: 0,
        },
    ]);

    // Blocks with an 'above' disposition go above their corresponding buffer line.
    // Blocks with a 'below' disposition go below their corresponding buffer line.
    let snapshot = block_map.read(wraps_snapshot, Default::default(), None);
    assert_eq!(
        snapshot.text(),
        "one two \nthree\n\nfour five \nsix\n\nseven \neight"
    );
}

#[gpui::test]
fn test_insert_and_remove_block_anchored_past_soft_wrap(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "one two three\nfour\nfive";

    let buffer = cx.update(|cx| MultiBuffer::build_simple(text, cx));
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (_, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let (_, wraps_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), Some(px(90.)), cx));
    let mut block_map = BlockMap::new(wraps_snapshot.clone(), 1, 1);

    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    let block_id = writer.insert(vec![BlockProperties {
        style: BlockStyle::Fixed,
        placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(0, 12))),
        render: Arc::new(|_| div().into_any()),
        height: Some(2),
        priority: 0,
    }])[0];

    let snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
    assert_eq!(snapshot.text(), "\n\none two \nthree\nfour\nfive");

    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    writer.remove(HashSet::from_iter([block_id]));

    let snapshot = block_map.read(wraps_snapshot, Default::default(), None);
    assert_eq!(snapshot.text(), "one two \nthree\nfour\nfive");
}

#[gpui::test]
fn test_replace_lines(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "line1\nline2\nline3\nline4\nline5";

    let buffer = cx.update(|cx| MultiBuffer::build_simple(text, cx));
    let buffer_subscription = buffer.update(cx, |buffer, _cx| buffer.subscribe());
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (mut fold_map, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let tab_size = 1.try_into().unwrap();
    let (mut tab_map, tab_snapshot) = TabMap::new(fold_snapshot, tab_size);
    let (wrap_map, wraps_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let mut block_map = BlockMap::new(wraps_snapshot.clone(), 1, 1);

    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    let replace_block_id = writer.insert(vec![BlockProperties {
        style: BlockStyle::Fixed,
        placement: BlockPlacement::Replace(
            buffer_snapshot.anchor_after(Point::new(1, 3))
                ..=buffer_snapshot.anchor_before(Point::new(3, 1)),
        ),
        height: Some(4),
        render: Arc::new(|_| div().into_any()),
        priority: 0,
    }])[0];

    let blocks_snapshot = block_map.read(wraps_snapshot, Default::default(), None);
    assert_eq!(blocks_snapshot.text(), "line1\n\n\n\n\nline5");

    let buffer_snapshot = buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(2, 0)..Point::new(3, 0), "")], None, cx);
        buffer.snapshot(cx)
    });
    let (inlay_snapshot, inlay_edits) =
        inlay_map.sync(buffer_snapshot, buffer_subscription.consume().into_inner());
    let (fold_snapshot, fold_edits) = fold_map.read(inlay_snapshot, inlay_edits);
    let (tab_snapshot, tab_edits) = tab_map.sync(fold_snapshot, fold_edits, tab_size);
    let (wraps_snapshot, wrap_edits) = wrap_map.update(cx, |wrap_map, cx| {
        wrap_map.sync(tab_snapshot, tab_edits, cx)
    });
    let blocks_snapshot = block_map.read(wraps_snapshot, wrap_edits, None);
    assert_eq!(blocks_snapshot.text(), "line1\n\n\n\n\nline5");

    let buffer_snapshot = buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [(
                Point::new(1, 5)..Point::new(1, 5),
                "\nline 2.1\nline2.2\nline 2.3\nline 2.4",
            )],
            None,
            cx,
        );
        buffer.snapshot(cx)
    });
    let (inlay_snapshot, inlay_edits) = inlay_map.sync(
        buffer_snapshot.clone(),
        buffer_subscription.consume().into_inner(),
    );
    let (fold_snapshot, fold_edits) = fold_map.read(inlay_snapshot, inlay_edits);
    let (tab_snapshot, tab_edits) = tab_map.sync(fold_snapshot, fold_edits, tab_size);
    let (wraps_snapshot, wrap_edits) = wrap_map.update(cx, |wrap_map, cx| {
        wrap_map.sync(tab_snapshot, tab_edits, cx)
    });
    let blocks_snapshot = block_map.read(wraps_snapshot.clone(), wrap_edits, None);
    assert_eq!(blocks_snapshot.text(), "line1\n\n\n\n\nline5");

    // Blocks inserted right above the start or right below the end of the replaced region are hidden.
    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    writer.insert(vec![
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(0, 3))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(1, 3))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Below(buffer_snapshot.anchor_after(Point::new(6, 2))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
    ]);
    let blocks_snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
    assert_eq!(blocks_snapshot.text(), "\nline1\n\n\n\n\nline5");

    // Ensure blocks inserted *inside* replaced region are hidden.
    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    writer.insert(vec![
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Below(buffer_snapshot.anchor_after(Point::new(1, 3))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(2, 1))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
        BlockProperties {
            style: BlockStyle::Fixed,
            placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(6, 1))),
            height: Some(1),
            render: Arc::new(|_| div().into_any()),
            priority: 0,
        },
    ]);
    let blocks_snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
    assert_eq!(blocks_snapshot.text(), "\nline1\n\n\n\n\nline5");

    // Removing the replace block shows all the hidden blocks again.
    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    writer.remove(HashSet::from_iter([replace_block_id]));
    let blocks_snapshot = block_map.read(wraps_snapshot, Default::default(), None);
    assert_eq!(
        blocks_snapshot.text(),
        "\nline1\n\nline2\n\n\nline 2.1\nline2.2\nline 2.3\nline 2.4\n\nline4\n\nline5"
    );
}
