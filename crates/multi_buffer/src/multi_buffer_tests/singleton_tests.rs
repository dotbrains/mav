use super::*;

#[gpui::test]
fn test_empty_singleton(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("", cx));
    let buffer_id = buffer.read(cx).remote_id();
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), "");
    assert_eq!(
        snapshot.row_infos(MultiBufferRow(0)).collect::<Vec<_>>(),
        [RowInfo {
            buffer_id: Some(buffer_id),
            buffer_row: Some(0),
            multibuffer_row: Some(MultiBufferRow(0)),
            diff_status: None,
            expand_info: None,
            wrapped_buffer_row: None,
        }]
    );
}

#[gpui::test]
fn test_singleton(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local(sample_text(6, 6, 'a'), cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), buffer.read(cx).text());

    assert_eq!(
        snapshot
            .row_infos(MultiBufferRow(0))
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        (0..buffer.read(cx).row_count())
            .map(Some)
            .collect::<Vec<_>>()
    );
    assert_consistent_line_numbers(&snapshot);

    buffer.update(cx, |buffer, cx| buffer.edit([(1..3, "XXX\n")], None, cx));
    let snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(snapshot.text(), buffer.read(cx).text());
    assert_eq!(
        snapshot
            .row_infos(MultiBufferRow(0))
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        (0..buffer.read(cx).row_count())
            .map(Some)
            .collect::<Vec<_>>()
    );
    assert_consistent_line_numbers(&snapshot);
}

#[gpui::test]
fn test_buffer_point_to_anchor_at_end_of_singleton_buffer(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("abc", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    let anchor = multibuffer
        .read(cx)
        .buffer_point_to_anchor(&buffer, Point::new(0, 3), cx)
        .unwrap();
    let (anchor, _) = multibuffer
        .read(cx)
        .snapshot(cx)
        .anchor_to_buffer_anchor(anchor)
        .unwrap();

    assert_eq!(
        anchor,
        buffer.read(cx).snapshot().anchor_after(Point::new(0, 3)),
    );
}

#[gpui::test]
fn test_remote(cx: &mut App) {
    let host_buffer = cx.new(|cx| Buffer::local("a", cx));
    let guest_buffer = cx.new(|cx| {
        let state = host_buffer.read(cx).to_proto(cx);
        let ops = cx
            .foreground_executor()
            .block_on(host_buffer.read(cx).serialize_ops(None, cx));
        let mut buffer =
            Buffer::from_proto(ReplicaId::REMOTE_SERVER, Capability::ReadWrite, state, None)
                .unwrap();
        buffer.apply_ops(
            ops.into_iter()
                .map(|op| language::proto::deserialize_operation(op).unwrap()),
            cx,
        );
        buffer
    });
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(guest_buffer.clone(), cx));
    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), "a");

    guest_buffer.update(cx, |buffer, cx| buffer.edit([(1..1, "b")], None, cx));
    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), "ab");

    guest_buffer.update(cx, |buffer, cx| buffer.edit([(2..2, "c")], None, cx));
    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), "abc");
}

#[gpui::test]
fn test_excerpt_boundaries_and_clipping(cx: &mut App) {
    let buffer_1 = cx.new(|cx| Buffer::local(sample_text(7, 6, 'a'), cx));
    let buffer_2 = cx.new(|cx| Buffer::local(sample_text(7, 6, 'g'), cx));
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    let events = Arc::new(RwLock::new(Vec::<Event>::new()));
    multibuffer.update(cx, |_, cx| {
        let events = events.clone();
        cx.subscribe(&multibuffer, move |_, _, event, _| {
            if let Event::Edited { .. } = event {
                events.write().push(event.clone())
            }
        })
        .detach();
    });

    let subscription = multibuffer.update(cx, |multibuffer, cx| {
        let subscription = multibuffer.subscribe();
        multibuffer.set_excerpt_ranges_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            &buffer_1.read(cx).snapshot(),
            vec![ExcerptRange::new(Point::new(1, 2)..Point::new(2, 5))],
            cx,
        );
        assert_eq!(
            subscription.consume().into_inner(),
            [Edit {
                old: MultiBufferOffset(0)..MultiBufferOffset(0),
                new: MultiBufferOffset(0)..MultiBufferOffset(10)
            }]
        );

        multibuffer.set_excerpt_ranges_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            &buffer_1.read(cx).snapshot(),
            vec![
                ExcerptRange::new(Point::new(1, 2)..Point::new(2, 5)),
                ExcerptRange::new(Point::new(5, 3)..Point::new(6, 4)),
            ],
            cx,
        );
        multibuffer.set_excerpt_ranges_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            &buffer_2.read(cx).snapshot(),
            vec![ExcerptRange::new(Point::new(3, 1)..Point::new(3, 3))],
            cx,
        );
        assert_eq!(
            subscription.consume().into_inner(),
            [Edit {
                old: MultiBufferOffset(10)..MultiBufferOffset(10),
                new: MultiBufferOffset(10)..MultiBufferOffset(22)
            }]
        );

        subscription
    });

    // Adding excerpts emits an edited event.
    assert_eq!(
        events.read().as_slice(),
        &[
            Event::Edited {
                edited_buffer: None,
                source: language::BufferEditSource::User,
            },
            Event::Edited {
                edited_buffer: None,
                source: language::BufferEditSource::User,
            },
            Event::Edited {
                edited_buffer: None,
                source: language::BufferEditSource::User,
            }
        ]
    );

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(
        snapshot.text(),
        indoc!(
            "
            bbbb
            ccccc
            fff
            gggg
            jj"
        ),
    );
    assert_eq!(
        snapshot
            .row_infos(MultiBufferRow(0))
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        [Some(1), Some(2), Some(5), Some(6), Some(3)]
    );
    assert_eq!(
        snapshot
            .row_infos(MultiBufferRow(2))
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        [Some(5), Some(6), Some(3)]
    );
    assert_eq!(
        snapshot
            .row_infos(MultiBufferRow(4))
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        [Some(3)]
    );
    assert!(
        snapshot
            .row_infos(MultiBufferRow(5))
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>()
            .is_empty()
    );

    assert_eq!(
        boundaries_in_range(Point::new(0, 0)..Point::new(4, 2), &snapshot),
        &[
            (MultiBufferRow(0), "bbbb\nccccc".to_string(), true),
            (MultiBufferRow(2), "fff\ngggg".to_string(), false),
            (MultiBufferRow(4), "jj".to_string(), true),
        ]
    );
    assert_eq!(
        boundaries_in_range(Point::new(0, 0)..Point::new(2, 0), &snapshot),
        &[(MultiBufferRow(0), "bbbb\nccccc".to_string(), true)]
    );
    assert_eq!(
        boundaries_in_range(Point::new(1, 0)..Point::new(1, 5), &snapshot),
        &[]
    );
    assert_eq!(
        boundaries_in_range(Point::new(1, 0)..Point::new(2, 0), &snapshot),
        &[]
    );
    assert_eq!(
        boundaries_in_range(Point::new(1, 0)..Point::new(4, 0), &snapshot),
        &[(MultiBufferRow(2), "fff\ngggg".to_string(), false)]
    );
    assert_eq!(
        boundaries_in_range(Point::new(1, 0)..Point::new(4, 0), &snapshot),
        &[(MultiBufferRow(2), "fff\ngggg".to_string(), false)]
    );
    assert_eq!(
        boundaries_in_range(Point::new(2, 0)..Point::new(3, 0), &snapshot),
        &[(MultiBufferRow(2), "fff\ngggg".to_string(), false)]
    );
    assert_eq!(
        boundaries_in_range(Point::new(4, 0)..Point::new(4, 2), &snapshot),
        &[(MultiBufferRow(4), "jj".to_string(), true)]
    );
    assert_eq!(
        boundaries_in_range(Point::new(4, 2)..Point::new(4, 2), &snapshot),
        &[]
    );

    buffer_1.update(cx, |buffer, cx| {
        let text = "\n";
        buffer.edit(
            [
                (Point::new(0, 0)..Point::new(0, 0), text),
                (Point::new(2, 1)..Point::new(2, 3), text),
            ],
            None,
            cx,
        );
    });

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(
        snapshot.text(),
        concat!(
            "bbbb\n", // Preserve newlines
            "c\n",    //
            "cc\n",   //
            "fff\n",  //
            "gggg\n", //
            "jj"      //
        )
    );

    assert_eq!(
        subscription.consume().into_inner(),
        [Edit {
            old: MultiBufferOffset(6)..MultiBufferOffset(8),
            new: MultiBufferOffset(6)..MultiBufferOffset(7)
        }]
    );

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(
        snapshot.clip_point(Point::new(0, 5), Bias::Left),
        Point::new(0, 4)
    );
    assert_eq!(
        snapshot.clip_point(Point::new(0, 5), Bias::Right),
        Point::new(0, 4)
    );
    assert_eq!(
        snapshot.clip_point(Point::new(5, 1), Bias::Right),
        Point::new(5, 1)
    );
    assert_eq!(
        snapshot.clip_point(Point::new(5, 2), Bias::Right),
        Point::new(5, 2)
    );
    assert_eq!(
        snapshot.clip_point(Point::new(5, 3), Bias::Right),
        Point::new(5, 2)
    );

    let snapshot = multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.remove_excerpts(PathKey::sorted(1), cx);
        multibuffer.snapshot(cx)
    });

    assert_eq!(
        snapshot.text(),
        concat!(
            "bbbb\n", // Preserve newlines
            "c\n",    //
            "cc\n",   //
            "fff\n",  //
            "gggg",   //
        )
    );

    fn boundaries_in_range(
        range: Range<Point>,
        snapshot: &MultiBufferSnapshot,
    ) -> Vec<(MultiBufferRow, String, bool)> {
        snapshot
            .excerpt_boundaries_in_range(range)
            .map(|boundary| {
                let starts_new_buffer = boundary.starts_new_buffer();
                (
                    boundary.row,
                    boundary
                        .next
                        .buffer(snapshot)
                        .text_for_range(boundary.next.range.context)
                        .collect::<String>(),
                    starts_new_buffer,
                )
            })
            .collect::<Vec<_>>()
    }
}
