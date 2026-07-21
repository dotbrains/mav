use super::*;

#[gpui::test]
fn test_excerpt_events(cx: &mut App) {
    let buffer_1 = cx.new(|cx| Buffer::local(sample_text(10, 3, 'a'), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(sample_text(10, 3, 'm'), cx));

    let leader_multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    let follower_multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    let follower_edit_event_count = Arc::new(RwLock::new(0));

    follower_multibuffer.update(cx, |_, cx| {
        let follower_edit_event_count = follower_edit_event_count.clone();
        cx.subscribe(
            &leader_multibuffer,
            move |follower, _, event, cx| match event.clone() {
                Event::BufferRangesUpdated {
                    buffer,
                    path_key,
                    ranges,
                } => {
                    let buffer_snapshot = buffer.read(cx).snapshot();
                    follower.set_merged_excerpt_ranges_for_path(
                        path_key,
                        buffer,
                        &buffer_snapshot,
                        ranges,
                        cx,
                    );
                }
                Event::BuffersRemoved {
                    removed_buffer_ids, ..
                } => {
                    for id in removed_buffer_ids {
                        follower.remove_excerpts_for_buffer(id, cx);
                    }
                }
                Event::Edited { .. } => {
                    *follower_edit_event_count.write() += 1;
                }
                _ => {}
            },
        )
        .detach();
    });

    let buffer_1_snapshot = buffer_1.read(cx).snapshot();
    let buffer_2_snapshot = buffer_2.read(cx).snapshot();
    leader_multibuffer.update(cx, |leader, cx| {
        leader.set_excerpt_ranges_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            &buffer_1_snapshot,
            vec![
                ExcerptRange::new((0..8).to_point(&buffer_1_snapshot)),
                ExcerptRange::new((22..26).to_point(&buffer_1_snapshot)),
            ],
            cx,
        );
        leader.set_excerpt_ranges_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            &buffer_2_snapshot,
            vec![
                ExcerptRange::new((0..5).to_point(&buffer_2_snapshot)),
                ExcerptRange::new((20..25).to_point(&buffer_2_snapshot)),
            ],
            cx,
        );
    });
    assert_eq!(
        leader_multibuffer.read(cx).snapshot(cx).text(),
        follower_multibuffer.read(cx).snapshot(cx).text(),
    );
    assert_eq!(*follower_edit_event_count.read(), 2);

    leader_multibuffer.update(cx, |leader, cx| {
        leader.set_excerpt_ranges_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            &buffer_1_snapshot,
            vec![ExcerptRange::new((0..8).to_point(&buffer_1_snapshot))],
            cx,
        );
        leader.set_excerpt_ranges_for_path(
            PathKey::sorted(1),
            buffer_2,
            &buffer_2_snapshot,
            vec![ExcerptRange::new((0..5).to_point(&buffer_2_snapshot))],
            cx,
        );
    });
    assert_eq!(
        leader_multibuffer.read(cx).snapshot(cx).text(),
        follower_multibuffer.read(cx).snapshot(cx).text(),
    );
    assert_eq!(*follower_edit_event_count.read(), 4);

    leader_multibuffer.update(cx, |leader, cx| {
        leader.clear(cx);
    });
    assert_eq!(
        leader_multibuffer.read(cx).snapshot(cx).text(),
        follower_multibuffer.read(cx).snapshot(cx).text(),
    );
    assert_eq!(*follower_edit_event_count.read(), 5);
}

#[gpui::test]
fn test_expand_excerpts(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local(sample_text(20, 3, 'a'), cx));
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            PathKey::for_buffer(&buffer, cx),
            buffer,
            vec![
                // Note that in this test, this first excerpt
                // does not contain a new line
                Point::new(3, 2)..Point::new(3, 3),
                Point::new(7, 1)..Point::new(7, 3),
                Point::new(15, 0)..Point::new(15, 0),
            ],
            1,
            cx,
        )
    });

    let snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(
        snapshot.text(),
        concat!(
            "ccc\n", //
            "ddd\n", //
            "eee",   //
            "\n",    // End of excerpt
            "ggg\n", //
            "hhh\n", //
            "iii",   //
            "\n",    // End of excerpt
            "ooo\n", //
            "ppp\n", //
            "qqq",   // End of excerpt
        )
    );
    drop(snapshot);

    multibuffer.update(cx, |multibuffer, cx| {
        let multibuffer_snapshot = multibuffer.snapshot(cx);
        let line_zero = multibuffer_snapshot.anchor_before(Point::new(0, 0));
        multibuffer.expand_excerpts(
            multibuffer.snapshot(cx).excerpts().map(|excerpt| {
                multibuffer_snapshot
                    .anchor_in_excerpt(excerpt.context.start)
                    .unwrap()
            }),
            1,
            ExpandExcerptDirection::UpAndDown,
            cx,
        );
        let snapshot = multibuffer.snapshot(cx);
        let line_two = snapshot.anchor_before(Point::new(2, 0));
        assert_eq!(line_two.cmp(&line_zero, &snapshot), cmp::Ordering::Greater);
    });

    let snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(
        snapshot.text(),
        concat!(
            "bbb\n", //
            "ccc\n", //
            "ddd\n", //
            "eee\n", //
            "fff\n", //
            "ggg\n", //
            "hhh\n", //
            "iii\n", //
            "jjj\n", // End of excerpt
            "nnn\n", //
            "ooo\n", //
            "ppp\n", //
            "qqq\n", //
            "rrr",   // End of excerpt
        )
    );
}

#[gpui::test(iterations = 100)]
async fn test_set_anchored_excerpts_for_path(cx: &mut TestAppContext) {
    let buffer_1 = cx.new(|cx| Buffer::local(sample_text(20, 3, 'a'), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(sample_text(15, 4, 'a'), cx));
    let snapshot_1 = buffer_1.update(cx, |buffer, _| buffer.snapshot());
    let snapshot_2 = buffer_2.update(cx, |buffer, _| buffer.snapshot());
    let ranges_1 = vec![
        snapshot_1.anchor_before(Point::new(3, 2))..snapshot_1.anchor_before(Point::new(4, 2)),
        snapshot_1.anchor_before(Point::new(7, 1))..snapshot_1.anchor_before(Point::new(7, 3)),
        snapshot_1.anchor_before(Point::new(15, 0))..snapshot_1.anchor_before(Point::new(15, 0)),
    ];
    let ranges_2 = vec![
        snapshot_2.anchor_before(Point::new(2, 1))..snapshot_2.anchor_before(Point::new(3, 1)),
        snapshot_2.anchor_before(Point::new(10, 0))..snapshot_2.anchor_before(Point::new(10, 2)),
    ];

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    let anchor_ranges_1 = multibuffer
        .update(cx, |multibuffer, cx| {
            multibuffer.set_anchored_excerpts_for_path(
                PathKey::for_buffer(&buffer_1, cx),
                buffer_1.clone(),
                ranges_1,
                2,
                cx,
            )
        })
        .await;
    let snapshot_1 = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    assert_eq!(
        anchor_ranges_1
            .iter()
            .map(|range| range.to_point(&snapshot_1))
            .collect::<Vec<_>>(),
        vec![
            Point::new(2, 2)..Point::new(3, 2),
            Point::new(6, 1)..Point::new(6, 3),
            Point::new(11, 0)..Point::new(11, 0),
        ]
    );
    let anchor_ranges_2 = multibuffer
        .update(cx, |multibuffer, cx| {
            multibuffer.set_anchored_excerpts_for_path(
                PathKey::for_buffer(&buffer_2, cx),
                buffer_2.clone(),
                ranges_2,
                2,
                cx,
            )
        })
        .await;
    let snapshot_2 = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    assert_eq!(
        anchor_ranges_2
            .iter()
            .map(|range| range.to_point(&snapshot_2))
            .collect::<Vec<_>>(),
        vec![
            Point::new(16, 1)..Point::new(17, 1),
            Point::new(22, 0)..Point::new(22, 2)
        ]
    );

    let snapshot = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    assert_eq!(
        snapshot.text(),
        concat!(
            "bbb\n", // buffer_1
            "ccc\n", //
            "ddd\n", // <-- excerpt 1
            "eee\n", // <-- excerpt 1
            "fff\n", //
            "ggg\n", //
            "hhh\n", // <-- excerpt 2
            "iii\n", //
            "jjj\n", //
            //
            "nnn\n", //
            "ooo\n", //
            "ppp\n", // <-- excerpt 3
            "qqq\n", //
            "rrr\n", //
            //
            "aaaa\n", // buffer 2
            "bbbb\n", //
            "cccc\n", // <-- excerpt 4
            "dddd\n", // <-- excerpt 4
            "eeee\n", //
            "ffff\n", //
            //
            "iiii\n", //
            "jjjj\n", //
            "kkkk\n", // <-- excerpt 5
            "llll\n", //
            "mmmm",   //
        )
    );
}
