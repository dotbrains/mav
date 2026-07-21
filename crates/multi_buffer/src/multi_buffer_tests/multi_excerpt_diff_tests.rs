use super::*;

#[gpui::test]
async fn test_diff_hunks_with_multiple_excerpts(cx: &mut TestAppContext) {
    let base_text_1 = indoc!(
        "
        one
        two
            three
        four
        five
        six
        "
    );
    let text_1 = indoc!(
        "
        ZERO
        one
        TWO
            three
        six
        "
    );
    let base_text_2 = indoc!(
        "
        seven
          eight
        nine
        ten
        eleven
        twelve
        "
    );
    let text_2 = indoc!(
        "
          eight
        nine
        eleven
        THIRTEEN
        FOURTEEN
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
    assert_eq!(
        snapshot.text(),
        indoc!(
            "
            ZERO
            one
            TWO
                three
            six

              eight
            nine
            eleven
            THIRTEEN
            FOURTEEN
            "
        ),
    );

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.expand_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
            + ZERO
              one
            - two
            + TWO
                  three
            - four
            - five
              six

            - seven
                eight
              nine
            - ten
              eleven
            - twelve
            + THIRTEEN
            + FOURTEEN
            "
        ),
    );

    let id_1 = buffer_1.read_with(cx, |buffer, _| buffer.remote_id());
    let id_2 = buffer_2.read_with(cx, |buffer, _| buffer.remote_id());
    let base_id_1 = diff_1.read_with(cx, |diff, cx| diff.base_text(cx).remote_id());
    let base_id_2 = diff_2.read_with(cx, |diff, cx| diff.base_text(cx).remote_id());

    let buffer_lines = (0..=snapshot.max_row().0)
        .map(|row| {
            let (buffer, range) = snapshot.buffer_line_for_row(MultiBufferRow(row))?;
            Some((
                buffer.remote_id(),
                buffer.text_for_range(range).collect::<String>(),
            ))
        })
        .collect::<Vec<_>>();
    pretty_assertions::assert_eq!(
        buffer_lines,
        [
            Some((id_1, "ZERO".into())),
            Some((id_1, "one".into())),
            Some((base_id_1, "two".into())),
            Some((id_1, "TWO".into())),
            Some((id_1, "    three".into())),
            Some((base_id_1, "four".into())),
            Some((base_id_1, "five".into())),
            Some((id_1, "six".into())),
            Some((id_1, "".into())),
            Some((base_id_2, "seven".into())),
            Some((id_2, "  eight".into())),
            Some((id_2, "nine".into())),
            Some((base_id_2, "ten".into())),
            Some((id_2, "eleven".into())),
            Some((base_id_2, "twelve".into())),
            Some((id_2, "THIRTEEN".into())),
            Some((id_2, "FOURTEEN".into())),
            Some((id_2, "".into())),
        ]
    );

    let buffer_ids_by_range = [
        (Point::new(0, 0)..Point::new(0, 0), &[id_1] as &[_]),
        (Point::new(0, 0)..Point::new(2, 0), &[id_1]),
        (Point::new(2, 0)..Point::new(2, 0), &[id_1]),
        (Point::new(3, 0)..Point::new(3, 0), &[id_1]),
        (Point::new(8, 0)..Point::new(9, 0), &[id_1]),
        (Point::new(8, 0)..Point::new(10, 0), &[id_1, id_2]),
        (Point::new(9, 0)..Point::new(9, 0), &[id_2]),
    ];
    for (range, buffer_ids) in buffer_ids_by_range {
        assert_eq!(
            snapshot
                .buffer_ids_for_range(range.clone())
                .collect::<Vec<_>>(),
            buffer_ids,
            "buffer_ids_for_range({range:?}"
        );
    }

    assert_position_translation(&snapshot);
    assert_line_indents(&snapshot);

    assert_eq!(
        snapshot
            .diff_hunks_in_range(MultiBufferOffset(0)..snapshot.len())
            .map(|hunk| hunk.row_range.start.0..hunk.row_range.end.0)
            .collect::<Vec<_>>(),
        &[0..1, 2..4, 5..7, 9..10, 12..13, 14..17]
    );

    buffer_2.update(cx, |buffer, cx| {
        buffer.edit_via_marked_text(
            indoc!(
                "
                  eight
                «»eleven
                THIRTEEN
                FOURTEEN
                "
            ),
            None,
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
            + ZERO
              one
            - two
            + TWO
                  three
            - four
            - five
              six

            - seven
                eight
              eleven
            - twelve
            + THIRTEEN
            + FOURTEEN
            "
        ),
    );

    assert_line_indents(&snapshot);
}
