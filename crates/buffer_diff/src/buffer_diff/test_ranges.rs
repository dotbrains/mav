use super::*;
use std::{fmt::Write as _, sync::mpsc};

use gpui::TestAppContext;
use pretty_assertions::{assert_eq, assert_ne};
use rand::{Rng as _, rngs::StdRng};
use text::{Buffer, BufferId, ReplicaId, Rope};
use unindent::Unindent as _;

#[gpui::test]
async fn test_changed_ranges(cx: &mut gpui::TestAppContext) {
    let base_text = "
            one
            two
            three
            four
            five
            six
        "
    .unindent();
    let buffer_text = "
            one
            TWO
            three
            four
            FIVE
            six
        "
    .unindent();
    let buffer = cx.new(|cx| language::Buffer::local(buffer_text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(&base_text, &buffer.read(cx).text_snapshot(), cx));
    cx.run_until_parked();
    let (tx, rx) = mpsc::channel();
    let subscription =
        cx.update(|cx| cx.subscribe(&diff, move |_, event, _| tx.send(event.clone()).unwrap()));

    let snapshot = buffer.update(cx, |buffer, cx| {
        buffer.set_text(
            "
                ONE
                TWO
                THREE
                FOUR
                FIVE
                SIX
            "
            .unindent(),
            cx,
        );
        buffer.text_snapshot()
    });
    let base_text_snapshot = diff.read_with(cx, |diff, cx| diff.base_text(cx));
    let update = diff
        .update(cx, |diff, cx| {
            diff.update_diff(
                snapshot.clone(),
                &base_text_snapshot,
                Some(Arc::from(base_text_snapshot.text())),
                cx,
            )
        })
        .await;
    diff.update(cx, |diff, cx| diff.set_snapshot(update, cx));
    cx.run_until_parked();
    drop(subscription);
    let events = rx.into_iter().collect::<Vec<_>>();
    match events.as_slice() {
        [
            BufferDiffEvent::DiffChanged(DiffChanged {
                changed_range: _,
                base_text_changed_range,
                extended_range: _,
                base_text_changed: _,
            }),
        ] => {
            // TODO(cole) this seems like it should pass but currently fails (see compare_hunks)
            // assert_eq!(
            //     *changed_range,
            //     Some(Anchor::min_max_range_for_buffer(
            //         buffer.read_with(cx, |buffer, _| buffer.remote_id())
            //     ))
            // );
            assert_eq!(*base_text_changed_range, Some(0..base_text.len()));
        }
        _ => panic!("unexpected events: {:?}", events),
    }
}

#[gpui::test]
async fn test_extended_range(cx: &mut TestAppContext) {
    let base_text = "
            aaa
            bbb





            ccc
            ddd
        "
    .unindent();

    let buffer_text = "
            aaa
            bbb





            CCC
            ddd
        "
    .unindent();

    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), buffer_text);
    let old_buffer = buffer.snapshot().clone();
    let diff_a = BufferDiffSnapshot::new_sync(&buffer, base_text.clone(), cx);

    buffer.edit([(Point::new(1, 3)..Point::new(1, 3), "\n")]);
    let diff_b = BufferDiffSnapshot::new_sync(&buffer, base_text, cx);

    let DiffChanged {
        changed_range,
        base_text_changed_range: _,
        extended_range,
        base_text_changed: _,
    } = compare_hunks(
        &diff_b.hunks,
        &diff_a.hunks,
        &old_buffer,
        &buffer,
        &diff_a.base_text(),
        &diff_a.base_text(),
    );

    let changed_range = changed_range.unwrap();
    assert_eq!(
        changed_range.to_point(&buffer),
        Point::new(7, 0)..Point::new(9, 0),
        "changed_range should span from old hunk position to new hunk end"
    );

    let extended_range = extended_range.unwrap();
    assert_eq!(
        extended_range.start.to_point(&buffer),
        Point::new(1, 3),
        "extended_range.start should extend to include the edit outside changed_range"
    );
    assert_eq!(
        extended_range.end.to_point(&buffer),
        Point::new(9, 0),
        "extended_range.end should collapse to changed_range.end when no edits in end margin"
    );

    let base_text_2 = "
            one
            two
            three
            four
            five
            six
            seven
            eight
        "
    .unindent();

    let buffer_text_2 = "
            ONE
            two
            THREE
            four
            FIVE
            six
            SEVEN
            eight
        "
    .unindent();

    let mut buffer_2 = Buffer::new(ReplicaId::LOCAL, BufferId::new(2).unwrap(), buffer_text_2);
    let old_buffer_2 = buffer_2.snapshot().clone();
    let diff_2a = BufferDiffSnapshot::new_sync(&buffer_2, base_text_2.clone(), cx);

    buffer_2.edit([(Point::new(4, 0)..Point::new(4, 4), "FIVE_CHANGED")]);
    let diff_2b = BufferDiffSnapshot::new_sync(&buffer_2, base_text_2, cx);

    let DiffChanged {
        changed_range,
        base_text_changed_range: _,
        extended_range,
        base_text_changed: _,
    } = compare_hunks(
        &diff_2b.hunks,
        &diff_2a.hunks,
        &old_buffer_2,
        &buffer_2,
        &diff_2a.base_text(),
        &diff_2a.base_text(),
    );

    let changed_range = changed_range.unwrap();
    assert_eq!(
        changed_range.to_point(&buffer_2),
        Point::new(4, 0)..Point::new(5, 0),
        "changed_range should be just the hunk that changed (FIVE)"
    );

    let extended_range = extended_range.unwrap();
    assert_eq!(
        extended_range.to_point(&buffer_2),
        Point::new(4, 0)..Point::new(5, 0),
        "extended_range should equal changed_range when edit is within the hunk"
    );
}
