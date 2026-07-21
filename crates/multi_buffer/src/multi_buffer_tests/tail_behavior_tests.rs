use super::*;

#[gpui::test]
fn test_history(cx: &mut App) {
    let test_settings = SettingsStore::test(cx);
    cx.set_global(test_settings);

    let group_interval: Duration = Duration::from_millis(1);
    let buffer_1 = cx.new(|cx| {
        let mut buf = Buffer::local("1234", cx);
        buf.set_group_interval(group_interval);
        buf
    });
    let buffer_2 = cx.new(|cx| {
        let mut buf = Buffer::local("5678", cx);
        buf.set_group_interval(group_interval);
        buf
    });
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    multibuffer.update(cx, |this, cx| {
        this.set_group_interval(group_interval, cx);
    });
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::zero()..buffer_1.read(cx).max_point()],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::zero()..buffer_2.read(cx).max_point()],
            0,
            cx,
        );
    });

    let mut now = Instant::now();

    multibuffer.update(cx, |multibuffer, cx| {
        let transaction_1 = multibuffer.start_transaction_at(now, cx).unwrap();
        multibuffer.edit(
            [
                (Point::new(0, 0)..Point::new(0, 0), "A"),
                (Point::new(1, 0)..Point::new(1, 0), "A"),
            ],
            None,
            cx,
        );
        multibuffer.edit(
            [
                (Point::new(0, 1)..Point::new(0, 1), "B"),
                (Point::new(1, 1)..Point::new(1, 1), "B"),
            ],
            None,
            cx,
        );
        multibuffer.end_transaction_at(now, cx);
        assert_eq!(multibuffer.read(cx).text(), "AB1234\nAB5678");

        // Verify edited ranges for transaction 1
        assert_eq!(
            multibuffer.edited_ranges_for_transaction(transaction_1, cx),
            &[
                MultiBufferOffset(0)..MultiBufferOffset(2),
                MultiBufferOffset(7)..MultiBufferOffset(9),
            ]
        );

        // Edit buffer 1 through the multibuffer
        now += 2 * group_interval;
        multibuffer.start_transaction_at(now, cx);
        multibuffer.edit(
            [(MultiBufferOffset(2)..MultiBufferOffset(2), "C")],
            None,
            cx,
        );
        multibuffer.end_transaction_at(now, cx);
        assert_eq!(multibuffer.read(cx).text(), "ABC1234\nAB5678");

        // Edit buffer 1 independently
        buffer_1.update(cx, |buffer_1, cx| {
            buffer_1.start_transaction_at(now);
            buffer_1.edit([(3..3, "D")], None, cx);
            buffer_1.end_transaction_at(now, cx);

            now += 2 * group_interval;
            buffer_1.start_transaction_at(now);
            buffer_1.edit([(4..4, "E")], None, cx);
            buffer_1.end_transaction_at(now, cx);
        });
        assert_eq!(multibuffer.read(cx).text(), "ABCDE1234\nAB5678");

        // An undo in the multibuffer undoes the multibuffer transaction
        // and also any individual buffer edits that have occurred since
        // that transaction.
        multibuffer.undo(cx);
        assert_eq!(multibuffer.read(cx).text(), "AB1234\nAB5678");

        multibuffer.undo(cx);
        assert_eq!(multibuffer.read(cx).text(), "1234\n5678");

        multibuffer.redo(cx);
        assert_eq!(multibuffer.read(cx).text(), "AB1234\nAB5678");

        multibuffer.redo(cx);
        assert_eq!(multibuffer.read(cx).text(), "ABCDE1234\nAB5678");

        // Undo buffer 2 independently.
        buffer_2.update(cx, |buffer_2, cx| buffer_2.undo(cx));
        assert_eq!(multibuffer.read(cx).text(), "ABCDE1234\n5678");

        // An undo in the multibuffer undoes the components of the
        // the last multibuffer transaction that are not already undone.
        multibuffer.undo(cx);
        assert_eq!(multibuffer.read(cx).text(), "AB1234\n5678");

        multibuffer.undo(cx);
        assert_eq!(multibuffer.read(cx).text(), "1234\n5678");

        multibuffer.redo(cx);
        assert_eq!(multibuffer.read(cx).text(), "AB1234\nAB5678");

        buffer_1.update(cx, |buffer_1, cx| buffer_1.redo(cx));
        assert_eq!(multibuffer.read(cx).text(), "ABCD1234\nAB5678");

        // Redo stack gets cleared after an edit.
        now += 2 * group_interval;
        multibuffer.start_transaction_at(now, cx);
        multibuffer.edit(
            [(MultiBufferOffset(0)..MultiBufferOffset(0), "X")],
            None,
            cx,
        );
        multibuffer.end_transaction_at(now, cx);
        assert_eq!(multibuffer.read(cx).text(), "XABCD1234\nAB5678");
        multibuffer.redo(cx);
        assert_eq!(multibuffer.read(cx).text(), "XABCD1234\nAB5678");
        multibuffer.undo(cx);
        assert_eq!(multibuffer.read(cx).text(), "ABCD1234\nAB5678");
        multibuffer.undo(cx);
        assert_eq!(multibuffer.read(cx).text(), "1234\n5678");

        // Transactions can be grouped manually.
        multibuffer.redo(cx);
        multibuffer.redo(cx);
        assert_eq!(multibuffer.read(cx).text(), "XABCD1234\nAB5678");
        multibuffer.group_until_transaction(transaction_1, cx);
        multibuffer.undo(cx);
        assert_eq!(multibuffer.read(cx).text(), "1234\n5678");
        multibuffer.redo(cx);
        assert_eq!(multibuffer.read(cx).text(), "XABCD1234\nAB5678");
    });
}

#[gpui::test]
async fn test_enclosing_indent(cx: &mut TestAppContext) {
    async fn enclosing_indent(
        text: &str,
        buffer_row: u32,
        cx: &mut TestAppContext,
    ) -> Option<(Range<u32>, LineIndent)> {
        let buffer = cx.update(|cx| MultiBuffer::build_simple(text, cx));
        let snapshot = cx.read(|cx| buffer.read(cx).snapshot(cx));
        let (range, indent) = snapshot
            .enclosing_indent(MultiBufferRow(buffer_row))
            .await?;
        Some((range.start.0..range.end.0, indent))
    }

    assert_eq!(
        enclosing_indent(
            indoc!(
                "
                fn b() {
                    if c {
                        let d = 2;
                    }
                }
                "
            ),
            1,
            cx,
        )
        .await,
        Some((
            1..2,
            LineIndent {
                tabs: 0,
                spaces: 4,
                line_blank: false,
            }
        ))
    );

    assert_eq!(
        enclosing_indent(
            indoc!(
                "
                fn b() {
                    if c {
                        let d = 2;
                    }
                }
                "
            ),
            2,
            cx,
        )
        .await,
        Some((
            1..2,
            LineIndent {
                tabs: 0,
                spaces: 4,
                line_blank: false,
            }
        ))
    );

    assert_eq!(
        enclosing_indent(
            indoc!(
                "
                fn b() {
                    if c {
                        let d = 2;

                        let e = 5;
                    }
                }
                "
            ),
            3,
            cx,
        )
        .await,
        Some((
            1..4,
            LineIndent {
                tabs: 0,
                spaces: 4,
                line_blank: false,
            }
        ))
    );
}

#[gpui::test]
async fn test_summaries_for_anchors(cx: &mut TestAppContext) {
    let base_text_1 = indoc!(
        "
        bar
        "
    );
    let text_1 = indoc!(
        "
        BAR
        "
    );
    let base_text_2 = indoc!(
        "
        foo
        "
    );
    let text_2 = indoc!(
        "
        FOO
        "
    );

    let buffer_1 = cx.new(|cx| Buffer::local(text_1, cx));
    let buffer_2 = cx.new(|cx| Buffer::local(text_2, cx));
    let diff_1 = cx.new(|cx| {
        BufferDiff::new_with_base_text(base_text_1, &buffer_1.read(cx).text_snapshot(), cx)
    });
    let diff_2 = cx.new(|cx| {
        BufferDiff::new_with_base_text(base_text_2, &buffer_2.read(cx).text_snapshot(), cx)
    });
    cx.run_until_parked();

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_all_diff_hunks_expanded(cx);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::zero()..buffer_1.read(cx).max_point()],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::zero()..buffer_2.read(cx).max_point()],
            0,
            cx,
        );
        multibuffer.add_diff(diff_1.clone(), cx);
        multibuffer.add_diff(diff_2.clone(), cx);
        multibuffer
    });

    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
            - bar
            + BAR

            - foo
            + FOO
            "
        ),
    );

    let anchor_1 = multibuffer.read_with(cx, |multibuffer, cx| {
        multibuffer
            .snapshot(cx)
            .anchor_in_excerpt(text::Anchor::min_for_buffer(buffer_1.read(cx).remote_id()))
            .unwrap()
    });
    let point_1 = snapshot.summaries_for_anchors::<Point, _>([&anchor_1])[0];
    assert_eq!(point_1, Point::new(0, 0));

    let anchor_2 = multibuffer.read_with(cx, |multibuffer, cx| {
        multibuffer
            .snapshot(cx)
            .anchor_in_excerpt(text::Anchor::min_for_buffer(buffer_2.read(cx).remote_id()))
            .unwrap()
    });
    let point_2 = snapshot.summaries_for_anchors::<Point, _>([&anchor_2])[0];
    assert_eq!(point_2, Point::new(3, 0));
}

#[gpui::test]
async fn test_trailing_deletion_without_newline(cx: &mut TestAppContext) {
    let base_text_1 = "one\ntwo".to_owned();
    let text_1 = "one\n".to_owned();

    let buffer_1 = cx.new(|cx| Buffer::local(text_1, cx));
    let diff_1 = cx.new(|cx| {
        BufferDiff::new_with_base_text(&base_text_1, &buffer_1.read(cx).text_snapshot(), cx)
    });
    cx.run_until_parked();

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::zero()..buffer_1.read(cx).max_point()],
            0,
            cx,
        );
        multibuffer.add_diff(diff_1.clone(), cx);
        multibuffer.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
        multibuffer
    });

    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
              one
            - two
            "
        ),
    );

    assert_eq!(snapshot.max_point(), Point::new(2, 0));
    assert_eq!(snapshot.len().0, 8);

    assert_eq!(
        snapshot
            .dimensions_from_points::<Point>([Point::new(2, 0)])
            .collect::<Vec<_>>(),
        vec![Point::new(2, 0)]
    );

    let (_, translated_offset) = snapshot.point_to_buffer_offset(Point::new(2, 0)).unwrap();
    assert_eq!(translated_offset.0, "one\n".len());
    let (_, translated_point) = snapshot.point_to_buffer_point(Point::new(2, 0)).unwrap();
    assert_eq!(translated_point, Point::new(1, 0));

    // The same, for an excerpt that's not at the end of the multibuffer.

    let text_2 = "foo\n".to_owned();
    let buffer_2 = cx.new(|cx| Buffer::local(&text_2, cx));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpt_ranges_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            &buffer_2.read(cx).snapshot(),
            vec![ExcerptRange::new(Point::new(0, 0)..Point::new(1, 0))],
            cx,
        );
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
              one
            - two

              foo
            "
        ),
    );

    assert_eq!(
        snapshot
            .dimensions_from_points::<Point>([Point::new(2, 0)])
            .collect::<Vec<_>>(),
        vec![Point::new(2, 0)]
    );

    let buffer_1_id = buffer_1.read_with(cx, |buffer_1, _| buffer_1.remote_id());
    let (buffer, translated_offset) = snapshot.point_to_buffer_offset(Point::new(2, 0)).unwrap();
    assert_eq!(buffer.remote_id(), buffer_1_id);
    assert_eq!(translated_offset.0, "one\n".len());
    let (buffer, translated_point) = snapshot.point_to_buffer_point(Point::new(2, 0)).unwrap();
    assert_eq!(buffer.remote_id(), buffer_1_id);
    assert_eq!(translated_point, Point::new(1, 0));
}
