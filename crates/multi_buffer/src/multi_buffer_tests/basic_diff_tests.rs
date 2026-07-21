use super::*;

#[gpui::test]
async fn test_basic_diff_hunks(cx: &mut TestAppContext) {
    let text = indoc!(
        "
        ZERO
        one
        TWO
        three
        six
        "
    );
    let base_text = indoc!(
        "
        one
        two
        three
        four
        five
        six
        "
    );

    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    cx.run_until_parked();

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::singleton(buffer.clone(), cx);
        multibuffer.add_diff(diff.clone(), cx);
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
            "
        ),
    );

    assert_eq!(
        snapshot
            .row_infos(MultiBufferRow(0))
            .map(|info| (info.buffer_row, info.diff_status))
            .collect::<Vec<_>>(),
        vec![
            (Some(0), Some(DiffHunkStatus::added_none())),
            (Some(1), None),
            (Some(1), Some(DiffHunkStatus::deleted_none())),
            (Some(2), Some(DiffHunkStatus::added_none())),
            (Some(3), None),
            (Some(3), Some(DiffHunkStatus::deleted_none())),
            (Some(4), Some(DiffHunkStatus::deleted_none())),
            (Some(4), None),
            (Some(5), None)
        ]
    );

    assert_chunks_in_ranges(&snapshot);
    assert_consistent_line_numbers(&snapshot);
    assert_position_translation(&snapshot);
    assert_line_indents(&snapshot);

    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.collapse_diff_hunks(vec![Anchor::Min..Anchor::Max], cx)
    });
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
            ZERO
            one
            TWO
            three
            six
            "
        ),
    );

    assert_chunks_in_ranges(&snapshot);
    assert_consistent_line_numbers(&snapshot);
    assert_position_translation(&snapshot);
    assert_line_indents(&snapshot);

    // Expand the first diff hunk
    multibuffer.update(cx, |multibuffer, cx| {
        let position = multibuffer.read(cx).anchor_before(Point::new(2, 2));
        multibuffer.expand_diff_hunks(vec![position..position], cx)
    });
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
              ZERO
              one
            - two
            + TWO
              three
              six
            "
        ),
    );

    // Expand the second diff hunk
    multibuffer.update(cx, |multibuffer, cx| {
        let start = multibuffer.read(cx).anchor_before(Point::new(4, 0));
        let end = multibuffer.read(cx).anchor_before(Point::new(5, 0));
        multibuffer.expand_diff_hunks(vec![start..end], cx)
    });
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
              ZERO
              one
            - two
            + TWO
              three
            - four
            - five
              six
            "
        ),
    );

    assert_chunks_in_ranges(&snapshot);
    assert_consistent_line_numbers(&snapshot);
    assert_position_translation(&snapshot);
    assert_line_indents(&snapshot);

    // Edit the buffer before the first hunk
    buffer.update(cx, |buffer, cx| {
        buffer.edit_via_marked_text(
            indoc!(
                "
                ZERO
                one« hundred
                  thousand»
                TWO
                three
                six
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
              ZERO
              one hundred
                thousand
            - two
            + TWO
              three
            - four
            - five
              six
            "
        ),
    );

    assert_chunks_in_ranges(&snapshot);
    assert_consistent_line_numbers(&snapshot);
    assert_position_translation(&snapshot);
    assert_line_indents(&snapshot);

    // Recalculate the diff, changing the first diff hunk.
    diff.update(cx, |diff, cx| {
        diff.recalculate_diff_sync(&buffer.read(cx).text_snapshot(), cx);
    });
    cx.run_until_parked();
    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
              ZERO
              one hundred
                thousand
              TWO
              three
            - four
            - five
              six
            "
        ),
    );

    assert_eq!(
        snapshot
            .diff_hunks_in_range(MultiBufferOffset(0)..snapshot.len())
            .map(|hunk| hunk.row_range.start.0..hunk.row_range.end.0)
            .collect::<Vec<_>>(),
        &[0..4, 5..7]
    );
}

#[gpui::test]
fn test_text_for_range_with_diff_transform_boundary_inside_multibyte_character(cx: &mut App) {
    let buffer = cx.new(|cx| Buffer::local("タx", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let mut snapshot = multibuffer.read(cx).snapshot(cx);

    fn ascii_summary_with_byte_len(byte_len: usize) -> MBTextSummary {
        let text = "x".repeat(byte_len);
        MBTextSummary::from(TextSummary::from(text.as_str()))
    }

    // FR-16 shown a diff transform boundary two bytes into the leading 'タ'.
    // Build that transform tree directly so this test stays focused on chunk iteration.
    let mut diff_transforms = SumTree::default();
    diff_transforms.push(
        DiffTransform::BufferContent {
            summary: ascii_summary_with_byte_len(2),
            inserted_hunk_info: None,
        },
        (),
    );
    diff_transforms.push(
        DiffTransform::BufferContent {
            summary: ascii_summary_with_byte_len("タx".len() - 2),
            inserted_hunk_info: None,
        },
        (),
    );
    snapshot.diff_transforms = diff_transforms;

    let text = snapshot
        .text_for_range(MultiBufferOffset(0)..snapshot.len())
        .collect::<String>();
    assert_eq!(text, "タx");
}

#[gpui::test]
async fn test_repeatedly_expand_a_diff_hunk(cx: &mut TestAppContext) {
    let text = indoc!(
        "
        one
        TWO
        THREE
        four
        FIVE
        six
        "
    );
    let base_text = indoc!(
        "
        one
        four
        five
        six
        "
    );

    let buffer = cx.new(|cx| Buffer::local(text, cx));
    let diff = cx
        .new(|cx| BufferDiff::new_with_base_text(base_text, &buffer.read(cx).text_snapshot(), cx));
    cx.run_until_parked();

    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::singleton(buffer.clone(), cx);
        multibuffer.add_diff(diff.clone(), cx);
        multibuffer
    });

    let (mut snapshot, mut subscription) = multibuffer.update(cx, |multibuffer, cx| {
        (multibuffer.snapshot(cx), multibuffer.subscribe())
    });

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
              one
            + TWO
            + THREE
              four
            - five
            + FIVE
              six
            "
        ),
    );

    // Regression test: expanding diff hunks that are already expanded should not change anything.
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.expand_diff_hunks(
            vec![
                snapshot.anchor_before(Point::new(2, 0))..snapshot.anchor_before(Point::new(2, 0)),
            ],
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
            + TWO
            + THREE
              four
            - five
            + FIVE
              six
            "
        ),
    );

    // Now collapse all diff hunks
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.collapse_diff_hunks(vec![Anchor::Min..Anchor::Max], cx);
    });

    assert_new_snapshot(
        &multibuffer,
        &mut snapshot,
        &mut subscription,
        cx,
        indoc!(
            "
            one
            TWO
            THREE
            four
            FIVE
            six
            "
        ),
    );

    // Expand the hunks again, but this time provide two ranges that are both within the same hunk
    // Target the first hunk which is between "one" and "four"
    multibuffer.update(cx, |multibuffer, cx| {
        multibuffer.expand_diff_hunks(
            vec![
                snapshot.anchor_before(Point::new(4, 0))..snapshot.anchor_before(Point::new(4, 0)),
                snapshot.anchor_before(Point::new(4, 2))..snapshot.anchor_before(Point::new(4, 2)),
            ],
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
              TWO
              THREE
              four
            - five
            + FIVE
              six
            "
        ),
    );
}
