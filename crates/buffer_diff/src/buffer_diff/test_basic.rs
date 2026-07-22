use super::*;
use util::test::marked_text_ranges;

#[ctor::ctor(unsafe)]
fn init_logger() {
    zlog::init_test();
}

#[gpui::test]
async fn test_buffer_diff_simple(cx: &mut gpui::TestAppContext) {
    let diff_base = "
        one
        two
        three
    "
    .unindent();

    let buffer_text = "
        one
        HELLO
        three
    "
    .unindent();

    let mut buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), buffer_text);
    let mut diff = BufferDiffSnapshot::new_sync(&buffer, diff_base.clone(), cx);
    assert_hunks(
        diff.hunks_intersecting_range(
            Anchor::min_max_range_for_buffer(buffer.remote_id()),
            &buffer,
        ),
        &buffer,
        &diff_base,
        &[(1..2, "two\n", "HELLO\n", DiffHunkStatus::modified_none())],
    );

    buffer.edit([(0..0, "point five\n")]);
    diff = BufferDiffSnapshot::new_sync(&buffer, diff_base.clone(), cx);
    assert_hunks(
        diff.hunks_intersecting_range(
            Anchor::min_max_range_for_buffer(buffer.remote_id()),
            &buffer,
        ),
        &buffer,
        &diff_base,
        &[
            (0..1, "", "point five\n", DiffHunkStatus::added_none()),
            (2..3, "two\n", "HELLO\n", DiffHunkStatus::modified_none()),
        ],
    );

    diff = cx.update(|cx| BufferDiff::new(&buffer, None, None, cx).snapshot(cx));
    assert_hunks::<&str, _>(
        diff.hunks_intersecting_range(
            Anchor::min_max_range_for_buffer(buffer.remote_id()),
            &buffer,
        ),
        &buffer,
        &diff_base,
        &[],
    );
}

#[gpui::test]
async fn test_buffer_diff_with_secondary(cx: &mut gpui::TestAppContext) {
    let head_text = "
        zero
        one
        two
        three
        four
        five
        six
        seven
        eight
        nine
    "
    .unindent();

    let index_text = "
        zero
        one
        TWO
        three
        FOUR
        five
        six
        seven
        eight
        NINE
    "
    .unindent();

    let buffer_text = "
        zero
        one
        TWO
        three
        FOUR
        FIVE
        six
        SEVEN
        eight
        nine
    "
    .unindent();

    let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), buffer_text);
    let unstaged_diff = BufferDiffSnapshot::new_sync(&buffer, index_text, cx);
    let mut uncommitted_diff = BufferDiffSnapshot::new_sync(&buffer, head_text.clone(), cx);
    uncommitted_diff.secondary_diff = Some(Arc::new(unstaged_diff));

    let expected_hunks = vec![
        (2..3, "two\n", "TWO\n", DiffHunkStatus::modified_none()),
        (
            4..6,
            "four\nfive\n",
            "FOUR\nFIVE\n",
            DiffHunkStatus::modified(DiffHunkSecondaryStatus::OverlapsWithSecondaryHunk),
        ),
        (
            7..8,
            "seven\n",
            "SEVEN\n",
            DiffHunkStatus::modified(DiffHunkSecondaryStatus::HasSecondaryHunk),
        ),
    ];

    assert_hunks(
        uncommitted_diff.hunks_intersecting_range(
            Anchor::min_max_range_for_buffer(buffer.remote_id()),
            &buffer,
        ),
        &buffer,
        &head_text,
        &expected_hunks,
    );
}

#[gpui::test]
async fn test_buffer_diff_range(cx: &mut TestAppContext) {
    let diff_base = "
        one
        two
        three
        four
        five
        six
        seven
        eight
        nine
        ten
    "
    .unindent();

    let buffer_text = "
        A
        one
        B
        two
        C
        three
        HELLO
        four
        five
        SIXTEEN
        seven
        eight
        WORLD
        nine

        ten

    "
    .unindent();

    let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), buffer_text);
    let diff = BufferDiffSnapshot::new_sync(buffer.snapshot(), diff_base.clone(), cx);
    assert_eq!(
        diff.hunks_intersecting_range(
            Anchor::min_max_range_for_buffer(buffer.remote_id()),
            &buffer
        )
        .count(),
        8
    );

    assert_hunks(
        diff.hunks_intersecting_range(
            buffer.anchor_before(Point::new(7, 0))..buffer.anchor_before(Point::new(12, 0)),
            &buffer,
        ),
        &buffer,
        &diff_base,
        &[
            (6..7, "", "HELLO\n", DiffHunkStatus::added_none()),
            (9..10, "six\n", "SIXTEEN\n", DiffHunkStatus::modified_none()),
            (12..13, "", "WORLD\n", DiffHunkStatus::added_none()),
        ],
    );
}
