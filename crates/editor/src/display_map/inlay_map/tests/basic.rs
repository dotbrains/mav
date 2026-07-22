use super::*;

#[gpui::test]
fn test_basic_inlays(cx: &mut App) {
    let buffer = MultiBuffer::build_simple("abcdefghi", cx);
    let buffer_edits = buffer.update(cx, |buffer, _| buffer.subscribe());
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer.read(cx).snapshot(cx));
    assert_eq!(inlay_snapshot.text(), "abcdefghi");
    let mut next_inlay_id = 0;

    let (inlay_snapshot, _) = inlay_map.splice(
        &[],
        vec![Inlay::mock_hint(
            post_inc(&mut next_inlay_id),
            buffer
                .read(cx)
                .snapshot(cx)
                .anchor_after(MultiBufferOffset(3)),
            "|123|",
        )],
    );
    assert_eq!(inlay_snapshot.text(), "abc|123|defghi");
    assert_eq!(
        inlay_snapshot.to_inlay_point(Point::new(0, 0)),
        InlayPoint::new(0, 0)
    );
    assert_eq!(
        inlay_snapshot.to_inlay_point(Point::new(0, 1)),
        InlayPoint::new(0, 1)
    );
    assert_eq!(
        inlay_snapshot.to_inlay_point(Point::new(0, 2)),
        InlayPoint::new(0, 2)
    );
    assert_eq!(
        inlay_snapshot.to_inlay_point(Point::new(0, 3)),
        InlayPoint::new(0, 3)
    );
    assert_eq!(
        inlay_snapshot.to_inlay_point(Point::new(0, 4)),
        InlayPoint::new(0, 9)
    );
    assert_eq!(
        inlay_snapshot.to_inlay_point(Point::new(0, 5)),
        InlayPoint::new(0, 10)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 0), Bias::Left),
        InlayPoint::new(0, 0)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 0), Bias::Right),
        InlayPoint::new(0, 0)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 3), Bias::Left),
        InlayPoint::new(0, 3)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 3), Bias::Right),
        InlayPoint::new(0, 3)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 4), Bias::Left),
        InlayPoint::new(0, 3)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 4), Bias::Right),
        InlayPoint::new(0, 9)
    );

    // Edits before or after the inlay should not affect it.
    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (MultiBufferOffset(2)..MultiBufferOffset(3), "x"),
                (MultiBufferOffset(3)..MultiBufferOffset(3), "y"),
                (MultiBufferOffset(4)..MultiBufferOffset(4), "z"),
            ],
            None,
            cx,
        )
    });
    let (inlay_snapshot, _) = inlay_map.sync(
        buffer.read(cx).snapshot(cx),
        buffer_edits.consume().into_inner(),
    );
    assert_eq!(inlay_snapshot.text(), "abxy|123|dzefghi");

    // An edit surrounding the inlay should invalidate it.
    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [(MultiBufferOffset(4)..MultiBufferOffset(5), "D")],
            None,
            cx,
        )
    });
    let (inlay_snapshot, _) = inlay_map.sync(
        buffer.read(cx).snapshot(cx),
        buffer_edits.consume().into_inner(),
    );
    assert_eq!(inlay_snapshot.text(), "abxyDzefghi");

    let (inlay_snapshot, _) = inlay_map.splice(
        &[],
        vec![
            Inlay::mock_hint(
                post_inc(&mut next_inlay_id),
                buffer
                    .read(cx)
                    .snapshot(cx)
                    .anchor_before(MultiBufferOffset(3)),
                "|123|",
            ),
            Inlay::edit_prediction(
                post_inc(&mut next_inlay_id),
                buffer
                    .read(cx)
                    .snapshot(cx)
                    .anchor_after(MultiBufferOffset(3)),
                "|456|",
            ),
        ],
    );
    assert_eq!(inlay_snapshot.text(), "abx|123||456|yDzefghi");

    // Edits ending where the inlay starts should not move it if it has a left bias.
    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [(MultiBufferOffset(3)..MultiBufferOffset(3), "JKL")],
            None,
            cx,
        )
    });
    let (inlay_snapshot, _) = inlay_map.sync(
        buffer.read(cx).snapshot(cx),
        buffer_edits.consume().into_inner(),
    );
    assert_eq!(inlay_snapshot.text(), "abx|123|JKL|456|yDzefghi");

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 0), Bias::Left),
        InlayPoint::new(0, 0)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 0), Bias::Right),
        InlayPoint::new(0, 0)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 1), Bias::Left),
        InlayPoint::new(0, 1)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 1), Bias::Right),
        InlayPoint::new(0, 1)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 2), Bias::Left),
        InlayPoint::new(0, 2)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 2), Bias::Right),
        InlayPoint::new(0, 2)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 3), Bias::Left),
        InlayPoint::new(0, 2)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 3), Bias::Right),
        InlayPoint::new(0, 8)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 4), Bias::Left),
        InlayPoint::new(0, 2)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 4), Bias::Right),
        InlayPoint::new(0, 8)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 5), Bias::Left),
        InlayPoint::new(0, 2)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 5), Bias::Right),
        InlayPoint::new(0, 8)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 6), Bias::Left),
        InlayPoint::new(0, 2)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 6), Bias::Right),
        InlayPoint::new(0, 8)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 7), Bias::Left),
        InlayPoint::new(0, 2)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 7), Bias::Right),
        InlayPoint::new(0, 8)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 8), Bias::Left),
        InlayPoint::new(0, 8)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 8), Bias::Right),
        InlayPoint::new(0, 8)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 9), Bias::Left),
        InlayPoint::new(0, 9)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 9), Bias::Right),
        InlayPoint::new(0, 9)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 10), Bias::Left),
        InlayPoint::new(0, 10)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 10), Bias::Right),
        InlayPoint::new(0, 10)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 11), Bias::Left),
        InlayPoint::new(0, 11)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 11), Bias::Right),
        InlayPoint::new(0, 11)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 12), Bias::Left),
        InlayPoint::new(0, 11)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 12), Bias::Right),
        InlayPoint::new(0, 17)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 13), Bias::Left),
        InlayPoint::new(0, 11)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 13), Bias::Right),
        InlayPoint::new(0, 17)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 14), Bias::Left),
        InlayPoint::new(0, 11)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 14), Bias::Right),
        InlayPoint::new(0, 17)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 15), Bias::Left),
        InlayPoint::new(0, 11)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 15), Bias::Right),
        InlayPoint::new(0, 17)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 16), Bias::Left),
        InlayPoint::new(0, 11)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 16), Bias::Right),
        InlayPoint::new(0, 17)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 17), Bias::Left),
        InlayPoint::new(0, 17)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 17), Bias::Right),
        InlayPoint::new(0, 17)
    );

    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 18), Bias::Left),
        InlayPoint::new(0, 18)
    );
    assert_eq!(
        inlay_snapshot.clip_point(InlayPoint::new(0, 18), Bias::Right),
        InlayPoint::new(0, 18)
    );

    // The inlays can be manually removed.
    let (inlay_snapshot, _) = inlay_map.splice(
        &inlay_map
            .inlays
            .iter()
            .map(|inlay| inlay.id)
            .collect::<Vec<InlayId>>(),
        Vec::new(),
    );
    assert_eq!(inlay_snapshot.text(), "abxJKLyDzefghi");
}

#[gpui::test]
fn test_inlay_buffer_rows(cx: &mut App) {
    let buffer = MultiBuffer::build_simple("abc\ndef\nghi", cx);
    let (mut inlay_map, inlay_snapshot) = InlayMap::new(buffer.read(cx).snapshot(cx));
    assert_eq!(inlay_snapshot.text(), "abc\ndef\nghi");
    let mut next_inlay_id = 0;

    let (inlay_snapshot, _) = inlay_map.splice(
        &[],
        vec![
            Inlay::mock_hint(
                post_inc(&mut next_inlay_id),
                buffer
                    .read(cx)
                    .snapshot(cx)
                    .anchor_before(MultiBufferOffset(0)),
                "|123|\n",
            ),
            Inlay::mock_hint(
                post_inc(&mut next_inlay_id),
                buffer
                    .read(cx)
                    .snapshot(cx)
                    .anchor_before(MultiBufferOffset(4)),
                "|456|",
            ),
            Inlay::edit_prediction(
                post_inc(&mut next_inlay_id),
                buffer
                    .read(cx)
                    .snapshot(cx)
                    .anchor_before(MultiBufferOffset(7)),
                "\n|567|\n",
            ),
        ],
    );
    assert_eq!(inlay_snapshot.text(), "|123|\nabc\n|456|def\n|567|\n\nghi");
    assert_eq!(
        inlay_snapshot
            .row_infos(0)
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        vec![Some(0), None, Some(1), None, None, Some(2)]
    );
}
