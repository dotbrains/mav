use super::*;
use std::{fmt::Write as _, sync::mpsc};

use gpui::TestAppContext;
use pretty_assertions::{assert_eq, assert_ne};
use rand::{Rng as _, rngs::StdRng};
use text::{Buffer, BufferId, ReplicaId, Rope};
use unindent::Unindent as _;

#[gpui::test]
async fn test_stage_hunk(cx: &mut TestAppContext) {
    struct Example {
        name: &'static str,
        head_text: String,
        index_text: String,
        buffer_marked_text: String,
        final_index_text: String,
    }

    let table = [
        Example {
            name: "uncommitted hunk straddles end of unstaged hunk",
            head_text: "
                    one
                    two
                    three
                    four
                    five
                "
            .unindent(),
            index_text: "
                    one
                    TWO_HUNDRED
                    three
                    FOUR_HUNDRED
                    five
                "
            .unindent(),
            buffer_marked_text: "
                    ZERO
                    one
                    two
                    «THREE_HUNDRED
                    FOUR_HUNDRED»
                    five
                    SIX
                "
            .unindent(),
            final_index_text: "
                    one
                    two
                    THREE_HUNDRED
                    FOUR_HUNDRED
                    five
                "
            .unindent(),
        },
        Example {
            name: "uncommitted hunk straddles start of unstaged hunk",
            head_text: "
                    one
                    two
                    three
                    four
                    five
                "
            .unindent(),
            index_text: "
                    one
                    TWO_HUNDRED
                    three
                    FOUR_HUNDRED
                    five
                "
            .unindent(),
            buffer_marked_text: "
                    ZERO
                    one
                    «TWO_HUNDRED
                    THREE_HUNDRED»
                    four
                    five
                    SIX
                "
            .unindent(),
            final_index_text: "
                    one
                    TWO_HUNDRED
                    THREE_HUNDRED
                    four
                    five
                "
            .unindent(),
        },
        Example {
            name: "uncommitted hunk strictly contains unstaged hunks",
            head_text: "
                    one
                    two
                    three
                    four
                    five
                    six
                    seven
                "
            .unindent(),
            index_text: "
                    one
                    TWO
                    THREE
                    FOUR
                    FIVE
                    SIX
                    seven
                "
            .unindent(),
            buffer_marked_text: "
                    one
                    TWO
                    «THREE_HUNDRED
                    FOUR
                    FIVE_HUNDRED»
                    SIX
                    seven
                "
            .unindent(),
            final_index_text: "
                    one
                    TWO
                    THREE_HUNDRED
                    FOUR
                    FIVE_HUNDRED
                    SIX
                    seven
                "
            .unindent(),
        },
        Example {
            name: "uncommitted deletion hunk",
            head_text: "
                    one
                    two
                    three
                    four
                    five
                "
            .unindent(),
            index_text: "
                    one
                    two
                    three
                    four
                    five
                "
            .unindent(),
            buffer_marked_text: "
                    one
                    ˇfive
                "
            .unindent(),
            final_index_text: "
                    one
                    five
                "
            .unindent(),
        },
        Example {
            name: "one unstaged hunk that contains two uncommitted hunks",
            head_text: "
                    one
                    two

                    three
                    four
                "
            .unindent(),
            index_text: "
                    one
                    two
                    three
                    four
                "
            .unindent(),
            buffer_marked_text: "
                    «one

                    three // modified
                    four»
                "
            .unindent(),
            final_index_text: "
                    one

                    three // modified
                    four
                "
            .unindent(),
        },
        Example {
            name: "one uncommitted hunk that contains two unstaged hunks",
            head_text: "
                    one
                    two
                    three
                    four
                    five
                "
            .unindent(),
            index_text: "
                    ZERO
                    one
                    TWO
                    THREE
                    FOUR
                    five
                "
            .unindent(),
            buffer_marked_text: "
                    «one
                    TWO_HUNDRED
                    THREE
                    FOUR_HUNDRED
                    five»
                "
            .unindent(),
            final_index_text: "
                    ZERO
                    one
                    TWO_HUNDRED
                    THREE
                    FOUR_HUNDRED
                    five
                "
            .unindent(),
        },
    ];

    for example in table {
        let (buffer_text, ranges) = marked_text_ranges(&example.buffer_marked_text, false);
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), buffer_text);
        let hunk_range = buffer.anchor_before(ranges[0].start)..buffer.anchor_before(ranges[0].end);

        let unstaged_diff =
            cx.new(|cx| BufferDiff::new_with_base_text(&example.index_text, &buffer, cx));

        let uncommitted_diff = cx.new(|cx| {
            let mut diff = BufferDiff::new_with_base_text(&example.head_text, &buffer, cx);
            diff.set_secondary_diff(unstaged_diff);
            diff
        });

        uncommitted_diff.update(cx, |diff, cx| {
            let hunks = diff
                .snapshot(cx)
                .hunks_intersecting_range(hunk_range.clone(), &buffer)
                .collect::<Vec<_>>();
            for hunk in &hunks {
                assert_ne!(
                    hunk.secondary_status,
                    DiffHunkSecondaryStatus::NoSecondaryHunk
                )
            }

            let new_index_text = diff
                .stage_or_unstage_hunks(true, &hunks, &buffer, true, cx)
                .unwrap()
                .to_string();

            let hunks = diff
                .snapshot(cx)
                .hunks_intersecting_range(hunk_range.clone(), &buffer)
                .collect::<Vec<_>>();
            for hunk in &hunks {
                assert_eq!(
                    hunk.secondary_status,
                    DiffHunkSecondaryStatus::SecondaryHunkRemovalPending
                )
            }

            pretty_assertions::assert_eq!(
                new_index_text,
                example.final_index_text,
                "example: {}",
                example.name
            );
        });
    }
}

#[gpui::test]
async fn test_stage_all_with_nested_hunks(cx: &mut TestAppContext) {
    // This test reproduces a crash where staging all hunks would cause an underflow
    // when there's one large unstaged hunk containing multiple uncommitted hunks.
    let head_text = "
            aaa
            bbb
            ccc
            ddd
            eee
            fff
            ggg
            hhh
            iii
            jjj
            kkk
            lll
        "
    .unindent();

    let index_text = "
            aaa
            bbb
            CCC-index
            DDD-index
            EEE-index
            FFF-index
            GGG-index
            HHH-index
            III-index
            JJJ-index
            kkk
            lll
        "
    .unindent();

    let buffer_text = "
            aaa
            bbb
            ccc-modified
            ddd
            eee-modified
            fff
            ggg
            hhh-modified
            iii
            jjj
            kkk
            lll
        "
    .unindent();

    let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), buffer_text);

    let unstaged_diff = cx.new(|cx| BufferDiff::new_with_base_text(&index_text, &buffer, cx));
    let uncommitted_diff = cx.new(|cx| {
        let mut diff = BufferDiff::new_with_base_text(&head_text, &buffer, cx);
        diff.set_secondary_diff(unstaged_diff);
        diff
    });

    uncommitted_diff.update(cx, |diff, cx| {
        diff.stage_or_unstage_all_hunks(true, &buffer, true, cx);
    });
}

#[gpui::test]
async fn test_stage_all_with_stale_buffer(cx: &mut TestAppContext) {
    // Regression test for MAV-5R2: when the buffer is edited after the diff is
    // computed but before staging, anchor positions shift while diff_base_byte_range
    // values don't. If the primary (HEAD) hunk extends past the unstaged (index)
    // hunk, an edit in the extension region shifts the primary hunk end without
    // shifting the unstaged hunk end. The overshoot calculation then produces an
    // index_end that exceeds index_text.len().
    //
    // Setup:
    //   HEAD:   "aaa\nbbb\nccc\n"  (primary hunk covers lines 1-2)
    //   Index:  "aaa\nbbb\nCCC\n"  (unstaged hunk covers line 1 only)
    //   Buffer: "aaa\nBBB\nCCC\n"  (both lines differ from HEAD)
    //
    // The primary hunk spans buffer offsets 4..12, but the unstaged hunk only
    // spans 4..8. The pending hunk extends 4 bytes past the unstaged hunk.
    // An edit at offset 9 (inside "CCC") shifts the primary hunk end from 12
    // to 13 but leaves the unstaged hunk end at 8, making index_end = 13 > 12.
    let head_text = "aaa\nbbb\nccc\n";
    let index_text = "aaa\nbbb\nCCC\n";
    let buffer_text = "aaa\nBBB\nCCC\n";

    let mut buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        buffer_text.to_string(),
    );

    let unstaged_diff = cx.new(|cx| BufferDiff::new_with_base_text(index_text, &buffer, cx));
    let uncommitted_diff = cx.new(|cx| {
        let mut diff = BufferDiff::new_with_base_text(head_text, &buffer, cx);
        diff.set_secondary_diff(unstaged_diff);
        diff
    });

    // Edit the buffer in the region between the unstaged hunk end (offset 8)
    // and the primary hunk end (offset 12). This shifts the primary hunk end
    // but not the unstaged hunk end.
    buffer.edit([(9..9, "Z")]);

    uncommitted_diff.update(cx, |diff, cx| {
        diff.stage_or_unstage_all_hunks(true, &buffer, true, cx);
    });
}

#[gpui::test]
async fn test_toggling_stage_and_unstage_same_hunk(cx: &mut TestAppContext) {
    let head_text = "
            one
            two
            three
        "
    .unindent();
    let index_text = head_text.clone();
    let buffer_text = "
            one
            three
        "
    .unindent();

    let buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        buffer_text.clone(),
    );
    let unstaged_diff = cx.new(|cx| BufferDiff::new_with_base_text(&index_text, &buffer, cx));
    let uncommitted_diff = cx.new(|cx| {
        let mut diff = BufferDiff::new_with_base_text(&head_text, &buffer, cx);
        diff.set_secondary_diff(unstaged_diff.clone());
        diff
    });

    uncommitted_diff.update(cx, |diff, cx| {
        let hunk = diff.snapshot(cx).hunks(&buffer).next().unwrap();

        let new_index_text = diff
            .stage_or_unstage_hunks(true, std::slice::from_ref(&hunk), &buffer, true, cx)
            .unwrap()
            .to_string();
        assert_eq!(new_index_text, buffer_text);

        let hunk = diff.snapshot(cx).hunks(&buffer).next().unwrap();
        assert_eq!(
            hunk.secondary_status,
            DiffHunkSecondaryStatus::SecondaryHunkRemovalPending
        );

        let index_text = diff
            .stage_or_unstage_hunks(false, &[hunk], &buffer, true, cx)
            .unwrap()
            .to_string();
        assert_eq!(index_text, head_text);

        let hunk = diff.snapshot(cx).hunks(&buffer).next().unwrap();
        // optimistically unstaged (fine, could also be HasSecondaryHunk)
        assert_eq!(
            hunk.secondary_status,
            DiffHunkSecondaryStatus::SecondaryHunkAdditionPending
        );
    });
}
