use super::*;
use gpui::{App, div};
use multi_buffer::MultiBuffer;

#[gpui::test]
fn test_insert_and_remove_creases(cx: &mut App) {
    let text = "line1\nline2\nline3\nline4\nline5";
    let buffer = MultiBuffer::build_simple(text, cx);
    let snapshot = buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx));
    let mut crease_map = CreaseMap::new(&buffer.read(cx).read(cx));

    let creases = [
        Crease::inline(
            snapshot.anchor_before(Point::new(1, 0))..snapshot.anchor_after(Point::new(1, 5)),
            FoldPlaceholder::test(),
            |_row, _folded, _toggle, _window, _cx| div(),
            |_row, _folded, _window, _cx| div(),
        ),
        Crease::inline(
            snapshot.anchor_before(Point::new(3, 0))..snapshot.anchor_after(Point::new(3, 5)),
            FoldPlaceholder::test(),
            |_row, _folded, _toggle, _window, _cx| div(),
            |_row, _folded, _window, _cx| div(),
        ),
    ];
    let crease_ids = crease_map.insert(creases, &snapshot);
    assert_eq!(crease_ids.len(), 2);

    let crease_snapshot = crease_map.snapshot();
    assert!(
        crease_snapshot
            .query_row(MultiBufferRow(1), &snapshot)
            .is_some()
    );
    assert!(
        crease_snapshot
            .query_row(MultiBufferRow(3), &snapshot)
            .is_some()
    );

    crease_map.remove(crease_ids, &snapshot);

    let crease_snapshot = crease_map.snapshot();
    assert!(
        crease_snapshot
            .query_row(MultiBufferRow(1), &snapshot)
            .is_none()
    );
    assert!(
        crease_snapshot
            .query_row(MultiBufferRow(3), &snapshot)
            .is_none()
    );
}

#[gpui::test]
#[ztracing::instrument(skip_all)]
fn test_creases_in_range(cx: &mut App) {
    let text = "line1\nline2\nline3\nline4\nline5\nline6\nline7";
    let buffer = MultiBuffer::build_simple(text, cx);
    let snapshot = buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx));
    let mut crease_map = CreaseMap::new(&snapshot);

    let creases = [
        Crease::inline(
            snapshot.anchor_before(Point::new(1, 0))..snapshot.anchor_after(Point::new(1, 5)),
            FoldPlaceholder::test(),
            |_row, _folded, _toggle, _window, _cx| div(),
            |_row, _folded, _window, _cx| div(),
        ),
        Crease::inline(
            snapshot.anchor_before(Point::new(3, 0))..snapshot.anchor_after(Point::new(3, 5)),
            FoldPlaceholder::test(),
            |_row, _folded, _toggle, _window, _cx| div(),
            |_row, _folded, _window, _cx| div(),
        ),
        Crease::inline(
            snapshot.anchor_before(Point::new(5, 0))..snapshot.anchor_after(Point::new(5, 5)),
            FoldPlaceholder::test(),
            |_row, _folded, _toggle, _window, _cx| div(),
            |_row, _folded, _window, _cx| div(),
        ),
    ];
    crease_map.insert(creases, &snapshot);

    let crease_snapshot = crease_map.snapshot();

    let range = MultiBufferRow(0)..MultiBufferRow(7);
    let creases: Vec<_> = crease_snapshot.creases_in_range(range, &snapshot).collect();
    assert_eq!(creases.len(), 3);

    let range = MultiBufferRow(2)..MultiBufferRow(5);
    let creases: Vec<_> = crease_snapshot.creases_in_range(range, &snapshot).collect();
    assert_eq!(creases.len(), 1);
    assert_eq!(creases[0].range().start.to_point(&snapshot).row, 3);

    let range = MultiBufferRow(0)..MultiBufferRow(2);
    let creases: Vec<_> = crease_snapshot.creases_in_range(range, &snapshot).collect();
    assert_eq!(creases.len(), 1);
    assert_eq!(creases[0].range().start.to_point(&snapshot).row, 1);

    let range = MultiBufferRow(6)..MultiBufferRow(7);
    let creases: Vec<_> = crease_snapshot.creases_in_range(range, &snapshot).collect();
    assert_eq!(creases.len(), 0);
}
