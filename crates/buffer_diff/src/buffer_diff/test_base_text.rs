use super::*;
use std::{fmt::Write as _, sync::mpsc};

use gpui::TestAppContext;
use pretty_assertions::{assert_eq, assert_ne};
use rand::{Rng as _, rngs::StdRng};
use text::{Buffer, BufferId, ReplicaId, Rope};
use unindent::Unindent as _;

#[gpui::test]
async fn test_buffer_diff_compare_with_base_text_change(_cx: &mut TestAppContext) {
    // Use a shared base text buffer so that anchors from old and new snapshots
    // share the same remote_id and resolve correctly across versions.
    let initial_base = "aaa\nbbb\nccc\nddd\neee\n";
    let mut base_text_buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(99).unwrap(),
        initial_base.to_string(),
    );

    // --- Scenario 1: Base text gains a line, producing a new deletion hunk ---
    //
    // Buffer has a modification (ccc → CCC). When the base text gains
    // a new line "XXX" after "aaa", the diff now also contains a
    // deletion for that line, and the modification hunk shifts in the
    // base text.
    let buffer_text_1 = "aaa\nbbb\nCCC\nddd\neee\n";
    let buffer = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(1).unwrap(),
        buffer_text_1.to_string(),
    );

    let old_base_snapshot_1 = base_text_buffer.snapshot().clone();
    let old_hunks_1 = compute_hunks(
        Some((Arc::from(initial_base), Rope::from(initial_base))),
        buffer.snapshot(),
        None,
    );

    // Insert "XXX\n" after "aaa\n" in the base text.
    base_text_buffer.edit([(4..4, "XXX\n")]);
    let new_base_str_1: Arc<str> = Arc::from(base_text_buffer.text().as_str());
    let new_base_snapshot_1 = base_text_buffer.snapshot();

    let new_hunks_1 = compute_hunks(
        Some((new_base_str_1.clone(), Rope::from(new_base_str_1.as_ref()))),
        buffer.snapshot(),
        None,
    );

    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &new_hunks_1,
        &old_hunks_1,
        &buffer.snapshot(),
        &buffer.snapshot(),
        &old_base_snapshot_1,
        &new_base_snapshot_1,
    );

    // The new deletion hunk (XXX) starts at buffer row 1 and the
    // modification hunk (ccc → CCC) now has a different
    // diff_base_byte_range, so the changed range spans both.
    let range = changed_range.unwrap();
    assert_eq!(range.to_point(&buffer), Point::new(1, 0)..Point::new(3, 0),);
    let base_range = base_text_changed_range.unwrap();
    assert_eq!(
        base_range.to_point(&new_base_snapshot_1),
        Point::new(1, 0)..Point::new(4, 0),
    );

    // --- Scenario 2: Base text changes to match the buffer (hunk disappears) ---
    //
    // Start fresh with a simple base text.
    let simple_base = "one\ntwo\nthree\n";
    let mut base_buf_2 = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(100).unwrap(),
        simple_base.to_string(),
    );

    let buffer_text_2 = "one\nTWO\nthree\n";
    let buffer_2 = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(2).unwrap(),
        buffer_text_2.to_string(),
    );

    let old_base_snapshot_2 = base_buf_2.snapshot().clone();
    let old_hunks_2 = compute_hunks(
        Some((Arc::from(simple_base), Rope::from(simple_base))),
        buffer_2.snapshot(),
        None,
    );

    // The base text is edited so "two" becomes "TWO", now matching the buffer.
    base_buf_2.edit([(4..7, "TWO")]);
    let new_base_str_2: Arc<str> = Arc::from(base_buf_2.text().as_str());
    let new_base_snapshot_2 = base_buf_2.snapshot();

    let new_hunks_2 = compute_hunks(
        Some((new_base_str_2.clone(), Rope::from(new_base_str_2.as_ref()))),
        buffer_2.snapshot(),
        None,
    );

    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &new_hunks_2,
        &old_hunks_2,
        &buffer_2.snapshot(),
        &buffer_2.snapshot(),
        &old_base_snapshot_2,
        &new_base_snapshot_2,
    );

    // The old modification hunk (two → TWO) is now gone because the
    // base text matches the buffer. The changed range covers where the
    // old hunk used to be.
    let range = changed_range.unwrap();
    assert_eq!(
        range.to_point(&buffer_2),
        Point::new(1, 0)..Point::new(2, 0),
    );
    let base_range = base_text_changed_range.unwrap();
    // The old hunk's diff_base_byte_range covered "two\n" (bytes 4..8).
    // anchor_after(4) is right-biased at the start of the deleted "two",
    // so after the edit replacing "two" with "TWO" it resolves past the
    // insertion to Point(1, 3).
    assert_eq!(
        base_range.to_point(&new_base_snapshot_2),
        Point::new(1, 3)..Point::new(2, 0),
    );

    // --- Scenario 3: Base text edit changes one hunk but not another ---
    //
    // Two modification hunks exist. Only one of them is resolved by
    // the base text change; the other remains identical.
    let base_3 = "aaa\nbbb\nccc\nddd\neee\n";
    let mut base_buf_3 = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(101).unwrap(),
        base_3.to_string(),
    );

    let buffer_text_3 = "aaa\nBBB\nccc\nDDD\neee\n";
    let buffer_3 = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(3).unwrap(),
        buffer_text_3.to_string(),
    );

    let old_base_snapshot_3 = base_buf_3.snapshot().clone();
    let old_hunks_3 = compute_hunks(
        Some((Arc::from(base_3), Rope::from(base_3))),
        buffer_3.snapshot(),
        None,
    );

    // Change "ddd" to "DDD" in the base text so that hunk disappears,
    // but "bbb" stays, so its hunk remains.
    base_buf_3.edit([(12..15, "DDD")]);
    let new_base_str_3: Arc<str> = Arc::from(base_buf_3.text().as_str());
    let new_base_snapshot_3 = base_buf_3.snapshot();

    let new_hunks_3 = compute_hunks(
        Some((new_base_str_3.clone(), Rope::from(new_base_str_3.as_ref()))),
        buffer_3.snapshot(),
        None,
    );

    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &new_hunks_3,
        &old_hunks_3,
        &buffer_3.snapshot(),
        &buffer_3.snapshot(),
        &old_base_snapshot_3,
        &new_base_snapshot_3,
    );

    // Only the second hunk (ddd → DDD) disappeared; the first hunk
    // (bbb → BBB) is unchanged, so the changed range covers only line 3.
    let range = changed_range.unwrap();
    assert_eq!(
        range.to_point(&buffer_3),
        Point::new(3, 0)..Point::new(4, 0),
    );
    let base_range = base_text_changed_range.unwrap();
    // anchor_after(12) is right-biased at the start of deleted "ddd",
    // so after the edit replacing "ddd" with "DDD" it resolves past
    // the insertion to Point(3, 3).
    assert_eq!(
        base_range.to_point(&new_base_snapshot_3),
        Point::new(3, 3)..Point::new(4, 0),
    );

    // --- Scenario 4: Both buffer and base text change simultaneously ---
    //
    // The buffer gains an edit that introduces a new hunk while the
    // base text also changes.
    let base_4 = "alpha\nbeta\ngamma\ndelta\n";
    let mut base_buf_4 = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(102).unwrap(),
        base_4.to_string(),
    );

    let buffer_text_4 = "alpha\nBETA\ngamma\ndelta\n";
    let mut buffer_4 = Buffer::new(
        ReplicaId::LOCAL,
        BufferId::new(4).unwrap(),
        buffer_text_4.to_string(),
    );

    let old_base_snapshot_4 = base_buf_4.snapshot().clone();
    let old_buffer_snapshot_4 = buffer_4.snapshot().clone();
    let old_hunks_4 = compute_hunks(
        Some((Arc::from(base_4), Rope::from(base_4))),
        buffer_4.snapshot(),
        None,
    );

    // Edit the buffer: change "delta" to "DELTA" (new modification hunk).
    buffer_4.edit_via_marked_text(
        &"
                alpha
                BETA
                gamma
                «DELTA»
            "
        .unindent(),
    );

    // Edit the base text: change "beta" to "BETA" (resolves that hunk).
    base_buf_4.edit([(6..10, "BETA")]);
    let new_base_str_4: Arc<str> = Arc::from(base_buf_4.text().as_str());
    let new_base_snapshot_4 = base_buf_4.snapshot();

    let new_hunks_4 = compute_hunks(
        Some((new_base_str_4.clone(), Rope::from(new_base_str_4.as_ref()))),
        buffer_4.snapshot(),
        None,
    );

    let DiffChanged {
        changed_range,
        base_text_changed_range,
        extended_range: _,
        base_text_changed: _,
    } = compare_hunks(
        &new_hunks_4,
        &old_hunks_4,
        &old_buffer_snapshot_4,
        &buffer_4.snapshot(),
        &old_base_snapshot_4,
        &new_base_snapshot_4,
    );

    // The old BETA hunk (line 1) is gone and a new DELTA hunk (line 3)
    // appeared, so the changed range spans from line 1 through line 4.
    let range = changed_range.unwrap();
    assert_eq!(
        range.to_point(&buffer_4),
        Point::new(1, 0)..Point::new(4, 0),
    );
    let base_range = base_text_changed_range.unwrap();
    // The old BETA hunk's base range started at byte 6 ("beta"). After
    // the base text edit replacing "beta" with "BETA", anchor_after(6)
    // resolves past the insertion to Point(1, 4).
    assert_eq!(
        base_range.to_point(&new_base_snapshot_4),
        Point::new(1, 4)..Point::new(4, 0),
    );
}
