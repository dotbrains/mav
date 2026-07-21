use super::*;

#[gpui::test]
async fn test_diff_boundary_anchors(cx: &mut TestAppContext) {
    let base_text = "one\ntwo\nthree\n";
    let text = "one\nthree\n";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    multibuffer.update(cx, |multibuffer, cx| multibuffer.add_diff(diff, cx));

    let (before, after) = multibuffer.update(cx, |multibuffer, cx| {
        let before = multibuffer.snapshot(cx).anchor_before(Point::new(1, 0));
        let after = multibuffer.snapshot(cx).anchor_after(Point::new(1, 0));
        multibuffer.set_all_diff_hunks_expanded(cx);
        (before, after)
    });
    cx.run_until_parked();

    let snapshot = multibuffer.read_with(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    let actual_text = snapshot.text();
    let actual_row_infos = snapshot.row_infos(MultiBufferRow(0)).collect::<Vec<_>>();
    let actual_diff = format_diff(&actual_text, &actual_row_infos, &Default::default(), None);
    pretty_assertions::assert_eq!(
        actual_diff,
        indoc! {
            "  one
             - two
               three
             "
        },
    );

    multibuffer.update(cx, |multibuffer, cx| {
        let snapshot = multibuffer.snapshot(cx);
        assert_eq!(before.to_point(&snapshot), Point::new(1, 0));
        assert_eq!(after.to_point(&snapshot), Point::new(2, 0));
        assert_eq!(
            vec![Point::new(1, 0), Point::new(2, 0),],
            snapshot.summaries_for_anchors::<Point, _>(&[before, after]),
        )
    })
}

#[gpui::test]
async fn test_diff_hunks_in_range(cx: &mut TestAppContext) {
    let base_text = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n";
    let text = "one\nfour\nseven\n";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.add_diff(diff, cx);
        multibuffer.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "  one
             - two
             - three
               four
             - five
             - six
               seven
             - eight
            "
        },
    );

    assert_eq!(
        snapshot
            .diff_hunks_in_range(Point::new(1, 0)..Point::MAX)
            .map(|hunk| hunk.row_range.start.0..hunk.row_range.end.0)
            .collect::<Vec<_>>(),
        vec![1..3, 4..6, 7..8]
    );

    assert_eq!(snapshot.diff_hunk_before(Point::new(1, 1)), None,);
    assert_eq!(
        snapshot.diff_hunk_before(Point::new(7, 0)),
        Some(MultiBufferRow(4))
    );
    assert_eq!(
        snapshot.diff_hunk_before(Point::new(4, 0)),
        Some(MultiBufferRow(1))
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.collapse_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
            one
            four
            seven
            "
        },
    );

    assert_eq!(
        snapshot.diff_hunk_before(Point::new(2, 0)),
        Some(MultiBufferRow(1)),
    );
    assert_eq!(
        snapshot.diff_hunk_before(Point::new(4, 0)),
        Some(MultiBufferRow(2))
    );
}

#[gpui::test]
async fn test_diff_hunks_in_range_query_starting_at_added_row(cx: &mut TestAppContext) {
    let base_text = "one\ntwo\nthree\n";
    let text = "one\nTWO\nthree\n";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.add_diff(diff, cx);
        multibuffer.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "  one
             - two
             + TWO
               three
            "
        },
    );

    assert_eq!(
        snapshot
            .diff_hunks_in_range(Point::new(2, 0)..Point::MAX)
            .map(|hunk| hunk.row_range.start.0..hunk.row_range.end.0)
            .collect::<Vec<_>>(),
        vec![1..3],
        "querying starting at the added row should still return the full hunk including deleted lines"
    );
}

#[gpui::test]
async fn test_inverted_diff_hunks_in_range(cx: &mut TestAppContext) {
    let base_text = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n";
    let text = "ZERO\none\nTHREE\nfour\nseven\nEIGHT\nNINE\n";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    let base_text_buffer = diff.read_with(cx, |diff, _| diff.base_text_buffer().clone());
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(base_text_buffer.clone(), cx));
    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.add_inverted_diff(diff, buffer.clone(), cx);
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "  one
             - two
             - three
               four
             - five
             - six
               seven
             - eight
            "
        },
    );

    assert_eq!(
        snapshot
            .diff_hunks_in_range(Point::new(0, 0)..Point::MAX)
            .map(|hunk| hunk.row_range.start.0..hunk.row_range.end.0)
            .collect::<Vec<_>>(),
        vec![0..0, 1..3, 4..6, 7..8]
    );

    assert_eq!(
        snapshot.diff_hunk_before(Point::new(1, 1)),
        Some(MultiBufferRow(0))
    );
    assert_eq!(
        snapshot.diff_hunk_before(Point::new(7, 0)),
        Some(MultiBufferRow(4))
    );
    assert_eq!(
        snapshot.diff_hunk_before(Point::new(4, 0)),
        Some(MultiBufferRow(1))
    );
}

#[gpui::test]
async fn test_editing_text_in_diff_hunks(cx: &mut TestAppContext) {
    let base_text = "one\ntwo\nfour\nfive\nsix\nseven\n";
    let text = "one\ntwo\nTHREE\nfour\nfive\nseven\n";
    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.add_diff(diff.clone(), cx);
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

    cx.executor().run_until_parked();
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_all_diff_hunks_expanded(cx);
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
              one
              two
            + THREE
              four
              five
            - six
              seven
            "
        },
    );

    // Insert a newline within an insertion hunk
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.edit([(Point::new(2, 0)..Point::new(2, 0), "__\n__")], None, cx);
    });
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
              one
              two
            + __
            + __THREE
              four
              five
            - six
              seven
            "
        },
    );

    // Delete the newline before a deleted hunk.
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.edit([(Point::new(5, 4)..Point::new(6, 0), "")], None, cx);
    });
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
              one
              two
            + __
            + __THREE
              four
              fiveseven
            "
        },
    );

    multibuffer.update(cx, |multibuffer, cx| multibuffer.undo(cx));
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
              one
              two
            + __
            + __THREE
              four
              five
            - six
              seven
            "
        },
    );

    // Cannot (yet) insert at the beginning of a deleted hunk.
    // (because it would put the newline in the wrong place)
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.edit([(Point::new(6, 0)..Point::new(6, 0), "\n")], None, cx);
    });
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
              one
              two
            + __
            + __THREE
              four
              five
            - six
              seven
            "
        },
    );

    // Replace a range that ends in a deleted hunk.
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.edit([(Point::new(5, 2)..Point::new(6, 2), "fty-")], None, cx);
    });
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc! {
            "
              one
              two
            + __
            + __THREE
              four
              fifty-seven
            "
        },
    );
}
