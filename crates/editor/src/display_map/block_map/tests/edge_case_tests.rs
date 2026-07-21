use super::*;

#[gpui::test]
fn test_remove_intersecting_replace_blocks_edge_case(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "abc\ndef\nghi\njkl\nmno";
    let buffer = cx.update(|cx| MultiBuffer::build_simple(text, cx));
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let (_inlay_map, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (_fold_map, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_tab_map, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let (_wrap_map, wraps_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let mut block_map = BlockMap::new(wraps_snapshot.clone(), 1, 1);

    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    let _block_id = writer.insert(vec![BlockProperties {
        style: BlockStyle::Fixed,
        placement: BlockPlacement::Above(buffer_snapshot.anchor_after(Point::new(1, 0))),
        height: Some(1),
        render: Arc::new(|_| div().into_any()),
        priority: 0,
    }])[0];

    let blocks_snapshot = block_map.read(wraps_snapshot.clone(), Default::default(), None);
    assert_eq!(blocks_snapshot.text(), "abc\n\ndef\nghi\njkl\nmno");

    let mut writer = block_map.write(wraps_snapshot.clone(), Default::default(), None);
    writer.remove_intersecting_replace_blocks(
        [buffer_snapshot
            .anchor_after(Point::new(1, 0))
            .to_offset(&buffer_snapshot)
            ..buffer_snapshot
                .anchor_after(Point::new(1, 0))
                .to_offset(&buffer_snapshot)],
        false,
    );
    let blocks_snapshot = block_map.read(wraps_snapshot, Default::default(), None);
    assert_eq!(blocks_snapshot.text(), "abc\n\ndef\nghi\njkl\nmno");
}

#[gpui::test]
fn test_folded_buffer_with_near_blocks(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "line 1\nline 2\nline 3";
    let buffer = cx.update(|cx| {
        MultiBuffer::build_multi([(text, vec![Point::new(0, 0)..Point::new(2, 6)])], cx)
    });
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let buffer_ids = buffer_snapshot
        .excerpts()
        .map(|excerpt| excerpt.context.start.buffer_id)
        .dedup()
        .collect::<Vec<_>>();
    assert_eq!(buffer_ids.len(), 1);
    let buffer_id = buffer_ids[0];

    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (_, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let (_, wrap_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let mut block_map = BlockMap::new(wrap_snapshot.clone(), 1, 1);

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    writer.insert(vec![BlockProperties {
        style: BlockStyle::Fixed,
        placement: BlockPlacement::Near(buffer_snapshot.anchor_after(Point::new(0, 0))),
        height: Some(1),
        render: Arc::new(|_| div().into_any()),
        priority: 0,
    }]);

    let blocks_snapshot = block_map.read(wrap_snapshot.clone(), Patch::default(), None);
    assert_eq!(blocks_snapshot.text(), "\nline 1\n\nline 2\nline 3");

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    buffer.read_with(cx, |buffer, cx| {
        writer.fold_buffers([buffer_id], buffer, cx);
    });

    let blocks_snapshot = block_map.read(wrap_snapshot, Patch::default(), None);
    assert_eq!(blocks_snapshot.text(), "");
}

#[gpui::test]
fn test_folded_buffer_with_near_blocks_on_last_line(cx: &mut gpui::TestAppContext) {
    cx.update(init_test);

    let text = "line 1\nline 2\nline 3\nline 4";
    let buffer = cx.update(|cx| {
        MultiBuffer::build_multi([(text, vec![Point::new(0, 0)..Point::new(3, 6)])], cx)
    });
    let buffer_snapshot = cx.update(|cx| buffer.read(cx).snapshot(cx));
    let buffer_ids = buffer_snapshot
        .excerpts()
        .map(|excerpt| excerpt.context.start.buffer_id)
        .dedup()
        .collect::<Vec<_>>();
    assert_eq!(buffer_ids.len(), 1);
    let buffer_id = buffer_ids[0];

    let (_, inlay_snapshot) = InlayMap::new(buffer_snapshot.clone());
    let (_, fold_snapshot) = FoldMap::new(inlay_snapshot);
    let (_, tab_snapshot) = TabMap::new(fold_snapshot, 4.try_into().unwrap());
    let (_, wrap_snapshot) =
        cx.update(|cx| WrapMap::new(tab_snapshot, font("Helvetica"), px(14.0), None, cx));
    let mut block_map = BlockMap::new(wrap_snapshot.clone(), 1, 1);

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    writer.insert(vec![BlockProperties {
        style: BlockStyle::Fixed,
        placement: BlockPlacement::Near(buffer_snapshot.anchor_after(Point::new(3, 6))),
        height: Some(1),
        render: Arc::new(|_| div().into_any()),
        priority: 0,
    }]);

    let blocks_snapshot = block_map.read(wrap_snapshot.clone(), Patch::default(), None);
    assert_eq!(blocks_snapshot.text(), "\nline 1\nline 2\nline 3\nline 4\n");

    let mut writer = block_map.write(wrap_snapshot.clone(), Patch::default(), None);
    buffer.read_with(cx, |buffer, cx| {
        writer.fold_buffers([buffer_id], buffer, cx);
    });

    let blocks_snapshot = block_map.read(wrap_snapshot, Patch::default(), None);
    assert_eq!(blocks_snapshot.text(), "");
}
