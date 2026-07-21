use super::*;

#[gpui::test]
async fn test_map_excerpt_ranges(cx: &mut TestAppContext) {
    let base_text = indoc!(
        "
        {
          (aaa)
          (bbb)
          (ccc)
        }
        xxx
        yyy
        zzz
        [
          (ddd)
          (EEE)
        ]
        "
    );
    let text = indoc!(
        "
        {
          (aaa)
          (CCC)
        }
        xxx
        yyy
        zzz
        [
          (ddd)
          (EEE)
        ]
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
                Point::new(0, 0)..Point::new(3, 1),
                Point::new(7, 0)..Point::new(10, 1),
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
    pretty_assertions::assert_eq!(
        actual_diff,
        indoc!(
            "
              {
                (aaa)
            -   (bbb)
            -   (ccc)
            +   (CCC)
              } [\u{2193}]
              [ [\u{2191}]
                (ddd)
                (EEE)
              ] [\u{2193}]"
        )
    );

    assert_eq!(
        snapshot.map_excerpt_ranges(
            snapshot.point_to_offset(Point::new(1, 3))..snapshot.point_to_offset(Point::new(1, 3)),
            |buffer, excerpt_range, input_range| {
                assert_eq!(
                    buffer.offset_to_point(input_range.start.0)
                        ..buffer.offset_to_point(input_range.end.0),
                    Point::new(1, 3)..Point::new(1, 3),
                );
                assert_eq!(
                    buffer.offset_to_point(excerpt_range.context.start.0)
                        ..buffer.offset_to_point(excerpt_range.context.end.0),
                    Point::new(0, 0)..Point::new(3, 1),
                );
                vec![
                    (input_range.start..BufferOffset(input_range.start.0 + 3), ()),
                    (excerpt_range.context, ()),
                    (
                        BufferOffset(text::ToOffset::to_offset(&Point::new(2, 2), buffer))
                            ..BufferOffset(text::ToOffset::to_offset(&Point::new(2, 7), buffer)),
                        (),
                    ),
                    (
                        BufferOffset(text::ToOffset::to_offset(&Point::new(0, 0), buffer))
                            ..BufferOffset(text::ToOffset::to_offset(&Point::new(2, 0), buffer)),
                        (),
                    ),
                ]
            },
        ),
        Some(vec![
            (
                snapshot.point_to_offset(Point::new(1, 3))
                    ..snapshot.point_to_offset(Point::new(1, 6)),
                (),
            ),
            (
                snapshot.point_to_offset(Point::zero())..snapshot.point_to_offset(Point::new(5, 1)),
                ()
            ),
            (
                snapshot.point_to_offset(Point::new(4, 2))
                    ..snapshot.point_to_offset(Point::new(4, 7)),
                (),
            ),
            (
                snapshot.point_to_offset(Point::zero())..snapshot.point_to_offset(Point::new(4, 0)),
                ()
            ),
        ]),
    );

    assert_eq!(
        snapshot.map_excerpt_ranges(
            snapshot.point_to_offset(Point::new(5, 0))..snapshot.point_to_offset(Point::new(7, 0)),
            |_, _, range| vec![(range, ())],
        ),
        None,
    );

    assert_eq!(
        snapshot.map_excerpt_ranges(
            snapshot.point_to_offset(Point::new(7, 3))..snapshot.point_to_offset(Point::new(7, 6)),
            |buffer, excerpt_range, input_range| {
                assert_eq!(
                    buffer.offset_to_point(input_range.start.0)
                        ..buffer.offset_to_point(input_range.end.0),
                    Point::new(8, 3)..Point::new(8, 6),
                );
                assert_eq!(
                    buffer.offset_to_point(excerpt_range.context.start.0)
                        ..buffer.offset_to_point(excerpt_range.context.end.0),
                    Point::new(7, 0)..Point::new(10, 1),
                );
                vec![(input_range, ())]
            },
        ),
        Some(vec![(
            snapshot.point_to_offset(Point::new(7, 3))..snapshot.point_to_offset(Point::new(7, 6)),
            (),
        )]),
    );
}
