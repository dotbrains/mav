use super::*;

#[gpui::test]
fn test_empty_multibuffer(cx: &mut App) {
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));

    let snapshot = multibuffer.read(cx).snapshot(cx);
    assert_eq!(snapshot.text(), "");
    assert_eq!(
        snapshot
            .row_infos(MultiBufferRow(0))
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>(),
        &[Some(0)]
    );
    assert!(
        snapshot
            .row_infos(MultiBufferRow(1))
            .map(|info| info.buffer_row)
            .collect::<Vec<_>>()
            .is_empty(),
    );
}

#[gpui::test]
async fn test_empty_diff_excerpt(cx: &mut TestAppContext) {
    let multibuffer = cx.new(|_| MultiBuffer::new(Capability::ReadWrite));
    let buffer = cx.new(|cx| Buffer::local("", cx));
    let base_text = "a\nb\nc";

    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpt_ranges_for_path(
            PathKey::sorted(0),
            buffer.clone(),
            &buffer.read(cx).snapshot(),
            vec![ExcerptRange::new(Point::zero()..Point::zero())],
            cx,
        );
        multibuffer.set_all_diff_hunks_expanded(cx);
        multibuffer.add_diff(diff.clone(), cx);
    });
    cx.run_until_parked();

    let snapshot = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    assert_eq!(snapshot.text(), "a\nb\nc\n");

    let hunk = snapshot
        .diff_hunks_in_range(Point::new(1, 1)..Point::new(1, 1))
        .next()
        .unwrap();

    assert_eq!(hunk.diff_base_byte_range.start, BufferOffset(0));

    let buf2 = cx.new(|cx| Buffer::local("X", cx));
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buf2,
            [Point::new(0, 0)..Point::new(0, 1)],
            0,
            cx,
        );
    });

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "a\nb\nc")], None, cx);
        diff.update(cx, |diff, cx| {
            diff.recalculate_diff_sync(&buffer.text_snapshot(), cx);
        });
        assert_eq!(buffer.text(), "a\nb\nc")
    });
    cx.run_until_parked();

    let snapshot = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    assert_eq!(snapshot.text(), "a\nb\nc\nX");

    buffer.update(cx, |buffer, cx| {
        buffer.undo(cx);
        diff.update(cx, |diff, cx| {
            diff.recalculate_diff_sync(&buffer.text_snapshot(), cx);
        });
        assert_eq!(buffer.text(), "")
    });
    cx.run_until_parked();

    let snapshot = multibuffer.update(cx, |multibuffer, cx| multibuffer.snapshot(cx));
    assert_eq!(snapshot.text(), "a\nb\nc\n\nX");
}

#[gpui::test]
fn test_singleton_multibuffer_anchors(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("abcd", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
    let old_snapshot = multibuffer.read(cx).snapshot(cx);
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "X")], None, cx);
        buffer.edit([(5..5, "Y")], None, cx);
    });
    let new_snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(old_snapshot.text(), "abcd");
    assert_eq!(new_snapshot.text(), "XabcdY");

    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(0))
            .to_offset(&new_snapshot),
        MultiBufferOffset(0)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(0))
            .to_offset(&new_snapshot),
        MultiBufferOffset(1)
    );
    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(4))
            .to_offset(&new_snapshot),
        MultiBufferOffset(5)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(4))
            .to_offset(&new_snapshot),
        MultiBufferOffset(6)
    );
}

#[gpui::test]
fn test_multibuffer_anchors(cx: &mut App) {
    let buffer_1 = cx.new(|cx| Buffer::local("abcd", cx));
    let buffer_2 = cx.new(|cx| Buffer::local("efghi", cx));
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            buffer_1.clone(),
            [Point::new(0, 0)..Point::new(0, 4)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            buffer_2.clone(),
            [Point::new(0, 0)..Point::new(0, 5)],
            0,
            cx,
        );
        multibuffer
    });
    let old_snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(0))
            .to_offset(&old_snapshot),
        MultiBufferOffset(0)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(0))
            .to_offset(&old_snapshot),
        MultiBufferOffset(0)
    );
    assert_eq!(Anchor::Min.to_offset(&old_snapshot), MultiBufferOffset(0));
    assert_eq!(Anchor::Min.to_offset(&old_snapshot), MultiBufferOffset(0));
    assert_eq!(Anchor::Max.to_offset(&old_snapshot), MultiBufferOffset(10));
    assert_eq!(Anchor::Max.to_offset(&old_snapshot), MultiBufferOffset(10));

    buffer_1.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "W")], None, cx);
        buffer.edit([(5..5, "X")], None, cx);
    });
    buffer_2.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "Y")], None, cx);
        buffer.edit([(6..6, "Z")], None, cx);
    });
    let new_snapshot = multibuffer.read(cx).snapshot(cx);

    assert_eq!(old_snapshot.text(), "abcd\nefghi");
    assert_eq!(new_snapshot.text(), "WabcdX\nYefghiZ");

    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(0))
            .to_offset(&new_snapshot),
        MultiBufferOffset(0)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(0))
            .to_offset(&new_snapshot),
        MultiBufferOffset(1)
    );
    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(1))
            .to_offset(&new_snapshot),
        MultiBufferOffset(2)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(1))
            .to_offset(&new_snapshot),
        MultiBufferOffset(2)
    );
    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(2))
            .to_offset(&new_snapshot),
        MultiBufferOffset(3)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(2))
            .to_offset(&new_snapshot),
        MultiBufferOffset(3)
    );
    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(5))
            .to_offset(&new_snapshot),
        MultiBufferOffset(7)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(5))
            .to_offset(&new_snapshot),
        MultiBufferOffset(8)
    );
    assert_eq!(
        old_snapshot
            .anchor_before(MultiBufferOffset(10))
            .to_offset(&new_snapshot),
        MultiBufferOffset(13)
    );
    assert_eq!(
        old_snapshot
            .anchor_after(MultiBufferOffset(10))
            .to_offset(&new_snapshot),
        MultiBufferOffset(14)
    );
}
