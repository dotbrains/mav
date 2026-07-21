use super::*;

#[gpui::test]
fn test_set_excerpts_for_buffer_rename(cx: &mut TestAppContext) {
    let buf1 = cx.new(|cx| {
        Buffer::local(
            indoc! {
            "zero
            one
            two
            three
            four
            five
            six
            seven
            ",
            },
            cx,
        )
    });
    let path: PathKey = PathKey::with_sort_prefix(0, rel_path("root").into_arc());
    let buf2 = cx.new(|cx| {
        Buffer::local(
            indoc! {
            "000
            111
            222
            333
            "
            },
            cx,
        )
    });

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path.clone(),
            buf1.clone(),
            vec![Point::row_range(1..1), Point::row_range(4..5)],
            1,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {
        "-----
        zero
        one
        two
        three
        four
        five
        six
        "
        },
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path.clone(),
            buf2.clone(),
            vec![Point::row_range(0..1)],
            2,
            cx,
        );
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {"-----
                000
                111
                222
                333
                "},
    );
}

#[gpui::test]
fn test_set_excerpts_for_path_replaces_previous_buffer(cx: &mut TestAppContext) {
    let buffer_a = cx.new(|cx| {
        Buffer::local(
            indoc! {
            "alpha
            beta
            gamma
            delta
            epsilon
            ",
            },
            cx,
        )
    });
    let buffer_b = cx.new(|cx| {
        Buffer::local(
            indoc! {
            "one
            two
            three
            four
            ",
            },
            cx,
        )
    });
    let path: PathKey = PathKey::with_sort_prefix(0, rel_path("shared/path").into_arc());

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    let removed_buffer_ids: Arc<RwLock<Vec<BufferId>>> = Default::default();
    multibuffer.update(cx, |_, cx| {
        let removed_buffer_ids = removed_buffer_ids.clone();
        cx.subscribe(&multibuffer, move |_, _, event, _| {
            if let Event::BuffersRemoved {
                removed_buffer_ids: ids,
            } = event
            {
                removed_buffer_ids.write().extend(ids.iter().copied());
            }
        })
        .detach();
    });

    let ranges_a = vec![Point::row_range(0..1), Point::row_range(3..4)];
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(path.clone(), buffer_a.clone(), ranges_a.clone(), 0, cx);
    });
    let (anchor_a1, anchor_a2) = multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        let buffer_snapshot = buffer_a.read(cx).snapshot();
        let mut anchors = ranges_a.into_iter().filter_map(|range| {
            let text_range = buffer_snapshot.anchor_range_inside(range);
            let start = snapshot.anchor_in_buffer(text_range.start)?;
            let end = snapshot.anchor_in_buffer(text_range.end)?;
            Some(start..end)
        });
        (
            anchors.next().expect("should have first anchor"),
            anchors.next().expect("should have second anchor"),
        )
    });

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {
        "-----
        alpha
        beta
        -----
        delta
        epsilon
        "
        },
    );

    let buffer_a_id = buffer_a.read_with(cx, |buffer, _| buffer.remote_id());
    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        assert!(
            snapshot
                .excerpts()
                .any(|excerpt| excerpt.context.start.buffer_id == buffer_a_id),
        );
    });

    let ranges_b = vec![Point::row_range(1..2)];
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(path.clone(), buffer_b.clone(), ranges_b.clone(), 1, cx);
    });
    let anchor_b = multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        let buffer_snapshot = buffer_b.read(cx).snapshot();
        ranges_b
            .into_iter()
            .filter_map(|range| {
                let text_range = buffer_snapshot.anchor_range_inside(range);
                let start = snapshot.anchor_in_buffer(text_range.start)?;
                let end = snapshot.anchor_in_buffer(text_range.end)?;
                Some(start..end)
            })
            .next()
            .expect("should have an anchor")
    });

    let buffer_b_id = buffer_b.read_with(cx, |buffer, _| buffer.remote_id());
    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        assert!(
            !snapshot
                .excerpts()
                .any(|excerpt| excerpt.context.start.buffer_id == buffer_a_id),
        );
        assert!(
            snapshot
                .excerpts()
                .any(|excerpt| excerpt.context.start.buffer_id == buffer_b_id),
        );
        assert!(
            multibuffer.buffer(buffer_a_id).is_none(),
            "old buffer should be fully removed from the multibuffer"
        );
        assert!(
            multibuffer.buffer(buffer_b_id).is_some(),
            "new buffer should be present in the multibuffer"
        );
    });
    assert!(
        removed_buffer_ids.read().contains(&buffer_a_id),
        "BuffersRemoved event should have been emitted for the old buffer"
    );

    assert_excerpts_match(
        &multibuffer,
        cx,
        indoc! {
        "-----
        one
        two
        three
        four
        "
        },
    );

    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        anchor_a1.start.cmp(&anchor_b.start, &snapshot);
        anchor_a1.end.cmp(&anchor_b.end, &snapshot);
        anchor_a1.start.cmp(&anchor_a2.start, &snapshot);
        anchor_a1.end.cmp(&anchor_a2.end, &snapshot);
    });
}

#[gpui::test]
fn test_stale_anchor_after_buffer_removal_and_path_reuse(cx: &mut TestAppContext) {
    let buffer_a = cx.new(|cx| Buffer::local("aaa\nbbb\nccc\n", cx));
    let buffer_b = cx.new(|cx| Buffer::local("xxx\nyyy\nzzz\n", cx));
    let buffer_other = cx.new(|cx| Buffer::local("111\n222\n333\n", cx));
    let path = PathKey::with_sort_prefix(0, rel_path("the/path").into_arc());
    let other_path = PathKey::with_sort_prefix(1, rel_path("other/path").into_arc());

    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path.clone(),
            buffer_a.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            other_path.clone(),
            buffer_other.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
    });

    buffer_a.update(cx, |buffer, cx| {
        buffer.edit(
            [(Point::new(1, 0)..Point::new(1, 0), "INSERTED ")],
            None,
            cx,
        );
    });

    let stale_anchor = multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        snapshot.anchor_before(Point::new(1, 5))
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.remove_excerpts(path.clone(), cx);
    });

    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        let offset = stale_anchor.to_offset(&snapshot);
        assert!(
            offset.0 <= snapshot.len().0,
            "stale anchor resolved to offset {offset:?} but multibuffer len is {:?}",
            snapshot.len()
        );
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            path.clone(),
            buffer_b.clone(),
            [Point::new(0, 0)..Point::new(2, 3)],
            0,
            cx,
        );
    });

    multibuffer.read_with(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        let offset = stale_anchor.to_offset(&snapshot);
        assert!(
            offset.0 <= snapshot.len().0,
            "stale anchor resolved to offset {offset:?} but multibuffer len is {:?}",
            snapshot.len()
        );
    });
}
