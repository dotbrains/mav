use super::*;

/// Tests `excerpt_containing` and `excerpts_for_range` (functions mapping multi-buffer text-coordinates to excerpts)
#[gpui::test]
fn test_excerpts_containment_functions(cx: &mut App) {
    // Multibuffer content for these tests:
    //    0123
    // 0: aa0
    // 1: aa1
    //    -----
    // 2: bb0
    // 3: bb1
    //    -----MultiBufferOffset(0)..
    // 4: cc0

    let buffer_1 = cx.new(|cx| Buffer::local("aa0\naa1", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("bb0\nbb1", cx));
    let buffer_3 = cx.new(|cx| Buffer::local("cc0", cx));

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    let (excerpt_1_info, excerpt_2_info, excerpt_3_info) =
        multibuffer.update(cx, |multibuffer, cx| {
            multibuffer.set_excerpts_for_path(
                PathKey::sorted(0),
                buffer_1.clone(),
                [Point::new(0, 0)..Point::new(1, 3)],
                0,
                cx,
            );

            multibuffer.set_excerpts_for_path(
                PathKey::sorted(1),
                buffer_2.clone(),
                [Point::new(0, 0)..Point::new(1, 3)],
                0,
                cx,
            );

            multibuffer.set_excerpts_for_path(
                PathKey::sorted(2),
                buffer_3.clone(),
                [Point::new(0, 0)..Point::new(0, 3)],
                0,
                cx,
            );

            let snapshot = multibuffer.snapshot(cx);
            let mut excerpts = snapshot.excerpts();
            (
                excerpts.next().unwrap(),
                excerpts.next().unwrap(),
                excerpts.next().unwrap(),
            )
        });

    let snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(snapshot.text(), "aa0\naa1\nbb0\nbb1\ncc0");

    //// Test `excerpts_for_range`

    let p00 = snapshot.point_to_offset(Point::new(0, 0));
    let p10 = snapshot.point_to_offset(Point::new(1, 0));
    let p20 = snapshot.point_to_offset(Point::new(2, 0));
    let p23 = snapshot.point_to_offset(Point::new(2, 3));
    let p13 = snapshot.point_to_offset(Point::new(1, 3));
    let p40 = snapshot.point_to_offset(Point::new(4, 0));
    let p43 = snapshot.point_to_offset(Point::new(4, 3));

    let excerpts: Vec<_> = snapshot.excerpts_for_range(p00..p00).collect();
    assert_eq!(excerpts.len(), 1);
    assert_eq!(excerpts[0].range, excerpt_1_info);

    // Cursor at very end of excerpt 3
    let excerpts: Vec<_> = snapshot.excerpts_for_range(p43..p43).collect();
    assert_eq!(excerpts.len(), 1);
    assert_eq!(excerpts[0].range, excerpt_3_info);

    let excerpts: Vec<_> = snapshot.excerpts_for_range(p00..p23).collect();
    assert_eq!(excerpts.len(), 2);
    assert_eq!(excerpts[0].range, excerpt_1_info);
    assert_eq!(excerpts[1].range, excerpt_2_info);

    // This range represent an selection with end-point just inside excerpt_2
    // Today we only expand the first excerpt, but another interpretation that
    // we could consider is expanding both here
    let excerpts: Vec<_> = snapshot.excerpts_for_range(p10..p20).collect();
    assert_eq!(excerpts.len(), 1);
    assert_eq!(excerpts[0].range, excerpt_1_info);

    //// Test that `excerpts_for_range` and `excerpt_containing` agree for all single offsets (cursor positions)
    for offset in 0..=snapshot.len().0 {
        let offset = MultiBufferOffset(offset);
        let excerpts_for_range: Vec<_> = snapshot.excerpts_for_range(offset..offset).collect();
        assert_eq!(
            excerpts_for_range.len(),
            1,
            "Expected exactly one excerpt for offset {offset}",
        );

        let (_, excerpt_containing) =
            snapshot
                .excerpt_containing(offset..offset)
                .unwrap_or_else(|| {
                    panic!("Expected excerpt_containing to find excerpt for offset {offset}")
                });

        assert_eq!(
            excerpts_for_range[0].range, excerpt_containing,
            "excerpts_for_range and excerpt_containing should agree for offset {offset}",
        );
    }

    //// Test `excerpt_containing` behavior with ranges:

    // Ranges intersecting a single-excerpt
    let (_, containing) = snapshot.excerpt_containing(p00..p13).unwrap();
    assert_eq!(containing, excerpt_1_info);

    // Ranges intersecting multiple excerpts (should return None)
    let containing = snapshot.excerpt_containing(p20..p40);
    assert!(
        containing.is_none(),
        "excerpt_containing should return None for ranges spanning multiple excerpts"
    );
}

#[gpui::test]
fn test_range_to_buffer_ranges(cx: &mut App) {
    let buffer_1 = cx.new(|cx| Buffer::local("aaa\nbbb", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("ccc", cx));

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(1, 3)],
            0,
            cx,
        );

        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(0, 3)],
            0,
            cx,
        );
    });

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), "aaa\nbbb\nccc");

    let excerpt_2_start = Point::new(2, 0);

    let ranges_half_open = snapshot.range_to_buffer_ranges(Point::zero()..excerpt_2_start);
    assert_eq!(
        ranges_half_open.len(),
        1,
        "Half-open range ending at excerpt start should EXCLUDE that excerpt"
    );
    assert_eq!(ranges_half_open[0].1, BufferOffset(0)..BufferOffset(7));
    assert_eq!(
        ranges_half_open[0].0.remote_id(),
        buffer_1.read(cx).remote_id()
    );

    let buffer_empty = cx.new(|cx| Buffer::local("", cx));
    let multibuffer_trailing_empty = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    let (_te_excerpt_1_info, _te_excerpt_2_info) =
        multibuffer_trailing_empty.update(cx, |multibuffer, cx| {
            multibuffer.set_excerpts_for_path(
                PathKey::sorted(0),
                buffer_1.clone(),
                [Point::new(0, 0)..Point::new(1, 3)],
                0,
                cx,
            );

            multibuffer.set_excerpts_for_path(
                PathKey::sorted(1),
                buffer_empty.clone(),
                [Point::new(0, 0)..Point::new(0, 0)],
                0,
                cx,
            );

            let snapshot = multibuffer.snapshot(cx);
            let mut infos = snapshot.excerpts();
            (infos.next().unwrap(), infos.next().unwrap())
        });

    let snapshot_trailing = multibuffer_trailing_empty.read(cx).snapshot(cx);
    assert_eq!(snapshot_trailing.text(), "aaa\nbbb\n");

    let max_point = snapshot_trailing.max_point();

    let ranges_half_open_max = snapshot_trailing.range_to_buffer_ranges(Point::zero()..max_point);
    assert_eq!(
        ranges_half_open_max.len(),
        2,
        "Should include trailing empty excerpts"
    );
    assert_eq!(ranges_half_open_max[1].1, BufferOffset(0)..BufferOffset(0));
}

#[gpui::test]
fn test_range_to_buffer_ranges_zero_length_at_excerpt_boundary(cx: &mut App) {
    let buffer_1 = cx.new(|cx| Buffer::local("aaa\nbbb", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("ccc\nddd", cx));

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(1, 3)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(1, 3)],
            0,
            cx,
        );
    });

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), "aaa\nbbb\nccc\nddd");

    // This point is right at the start of the very first excerpt, so if we get
    // a buffer range, we should get `0..0`
    let excerpt_2_start = Point::new(2, 0);
    let expected_ranges = vec![BufferOffset(0)..BufferOffset(0)];
    let ranges = snapshot
        .range_to_buffer_ranges(excerpt_2_start..excerpt_2_start)
        .into_iter()
        .map(|tup| tup.1)
        .collect_vec();

    assert_eq!(
        ranges, expected_ranges,
        "Zero-length range at excerpt boundary should return the excerpt at that point"
    );
}

#[gpui::test]
async fn test_buffer_range_to_excerpt_ranges(cx: &mut TestAppContext) {
    let base_text = indoc!(
        "
        aaa
        bbb
        ccc
        ddd
        eee
        ppp
        qqq
        rrr
        fff
        ggg
        hhh
        "
    );
    let text = indoc!(
        "
        aaa
        BBB
        ddd
        eee
        ppp
        qqq
        rrr
        FFF
        ggg
        hhh
        "
    );

    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    cx.run_until_parked();

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            [
                Point::new(0, 0)..Point::new(3, 3),
                Point::new(7, 0)..Point::new(9, 3),
            ],
            0,
            cx,
        );
        multibuffer.add_diff(diff.clone(), cx);
        multibuffer
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
    });
    cx.run_until_parked();

    let snapshot = multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));

    let actual_diff = format_diff(
        &snapshot.text(),
        &snapshot.row_infos(MultiBufferRow(0)).collect::<Vec<_>>(),
        &Default::default(),
        None,
    );
    let expected_diff = indoc!(
        "
          aaa
        - bbb
        - ccc
        + BBB
          ddd
          eee [\u{2193}]
        - fff [\u{2191}]
        + FFF
          ggg
          hhh [\u{2193}]"
    );
    pretty_assertions::assert_eq!(actual_diff, expected_diff);

    let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());

    let query_spanning_deleted_hunk = buffer_snapshot.anchor_after(Point::new(0, 0))
        ..buffer_snapshot.anchor_before(Point::new(1, 3));
    assert_eq!(
        snapshot
            .buffer_range_to_excerpt_ranges(query_spanning_deleted_hunk)
            .map(|range| range.to_point(&snapshot))
            .collect::<Vec<_>>(),
        vec![
            Point::new(0, 0)..Point::new(1, 0),
            Point::new(3, 0)..Point::new(3, 3),
        ],
    );

    let query_within_contiguous_main_buffer = buffer_snapshot.anchor_after(Point::new(1, 0))
        ..buffer_snapshot.anchor_before(Point::new(2, 3));
    assert_eq!(
        snapshot
            .buffer_range_to_excerpt_ranges(query_within_contiguous_main_buffer)
            .map(|range| range.to_point(&snapshot))
            .collect::<Vec<_>>(),
        vec![Point::new(3, 0)..Point::new(4, 3)],
    );

    let query_spanning_both_excerpts = buffer_snapshot.anchor_after(Point::new(2, 0))
        ..buffer_snapshot.anchor_before(Point::new(8, 3));
    assert_eq!(
        snapshot
            .buffer_range_to_excerpt_ranges(query_spanning_both_excerpts)
            .map(|range| range.to_point(&snapshot))
            .collect::<Vec<_>>(),
        vec![
            Point::new(4, 0)..Point::new(5, 3),
            Point::new(7, 0)..Point::new(8, 3),
        ],
    );
}
