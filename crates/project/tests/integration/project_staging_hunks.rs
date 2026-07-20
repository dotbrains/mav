use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_staging_hunks(cx: &mut gpui::TestAppContext) {
    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_contents = r#"
        zero
        one
        two
        three
        four
        five
    "#
    .unindent();
    let file_contents = r#"
        one
        TWO
        three
        FOUR
        five
    "#
    .unindent();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
            "file.txt": file_contents.clone()
        }),
    )
    .await;

    fs.set_head_and_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", committed_contents.clone())],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/file.txt", cx)
        })
        .await
        .unwrap();
    let snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
    let uncommitted_diff = project
        .update(cx, |project, cx| {
            project.open_uncommitted_diff(buffer.clone(), cx)
        })
        .await
        .unwrap();
    let mut diff_events = cx.events(&uncommitted_diff);

    // The hunks are initially unstaged.
    uncommitted_diff.read_with(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // Stage a hunk. It appears as optimistically staged.
    uncommitted_diff.update(cx, |diff, cx| {
        let range =
            snapshot.anchor_before(Point::new(1, 0))..snapshot.anchor_before(Point::new(2, 0));
        let hunks = diff
            .snapshot(cx)
            .hunks_intersecting_range(range, &snapshot)
            .collect::<Vec<_>>();
        diff.stage_or_unstage_hunks(true, &hunks, &snapshot, true, cx);

        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(SecondaryHunkRemovalPending),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // The diff emits a change event for the range of the staged hunk.
    assert!(matches!(
        diff_events.next().await.unwrap(),
        BufferDiffEvent::HunksStagedOrUnstaged(_)
    ));
    let event = diff_events.next().await.unwrap();
    if let BufferDiffEvent::DiffChanged(DiffChanged {
        changed_range: Some(changed_range),
        base_text_changed_range: _,
        extended_range: _,
        base_text_changed: _,
    }) = event
    {
        let changed_range = changed_range.to_point(&snapshot);
        assert_eq!(changed_range, Point::new(1, 0)..Point::new(2, 0));
    } else {
        panic!("Unexpected event {event:?}");
    }

    // When the write to the index completes, it appears as staged.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    // The diff emits a change event for the changed index text.
    let event = diff_events.next().await.unwrap();
    if let BufferDiffEvent::DiffChanged(DiffChanged {
        changed_range: Some(changed_range),
        base_text_changed_range: _,
        extended_range: _,
        base_text_changed: _,
    }) = event
    {
        let changed_range = changed_range.to_point(&snapshot);
        assert_eq!(changed_range, Point::new(1, 0)..Point::new(2, 0));
    } else {
        panic!("Unexpected event {event:?}");
    }

    // Simulate a problem writing to the git index.
    fs.set_error_message_for_index_write(
        "/dir/.git".as_ref(),
        Some("failed to write git index".into()),
    );

    // Stage another hunk.
    uncommitted_diff.update(cx, |diff, cx| {
        let range =
            snapshot.anchor_before(Point::new(3, 0))..snapshot.anchor_before(Point::new(4, 0));
        let hunks = diff
            .snapshot(cx)
            .hunks_intersecting_range(range, &snapshot)
            .collect::<Vec<_>>();
        diff.stage_or_unstage_hunks(true, &hunks, &snapshot, true, cx);

        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(SecondaryHunkRemovalPending),
                ),
            ],
        );
    });
    assert!(matches!(
        diff_events.next().await.unwrap(),
        BufferDiffEvent::HunksStagedOrUnstaged(_)
    ));
    let event = diff_events.next().await.unwrap();
    if let BufferDiffEvent::DiffChanged(DiffChanged {
        changed_range: Some(changed_range),
        base_text_changed_range: _,
        extended_range: _,
        base_text_changed: _,
    }) = event
    {
        let changed_range = changed_range.to_point(&snapshot);
        assert_eq!(changed_range, Point::new(3, 0)..Point::new(4, 0));
    } else {
        panic!("Unexpected event {event:?}");
    }

    // When the write fails, the hunk returns to being unstaged.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(HasSecondaryHunk),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(HasSecondaryHunk),
                ),
            ],
        );
    });

    let event = diff_events.next().await.unwrap();
    if let BufferDiffEvent::DiffChanged(DiffChanged {
        changed_range: Some(changed_range),
        base_text_changed_range: _,
        extended_range: _,
        base_text_changed: _,
    }) = event
    {
        let changed_range = changed_range.to_point(&snapshot);
        assert_eq!(changed_range, Point::new(0, 0)..Point::new(5, 0));
    } else {
        panic!("Unexpected event {event:?}");
    }

    // Allow writing to the git index to succeed again.
    fs.set_error_message_for_index_write("/dir/.git".as_ref(), None);

    // Stage two hunks with separate operations.
    uncommitted_diff.update(cx, |diff, cx| {
        let hunks = diff.snapshot(cx).hunks(&snapshot).collect::<Vec<_>>();
        diff.stage_or_unstage_hunks(true, &hunks[0..1], &snapshot, true, cx);
        diff.stage_or_unstage_hunks(true, &hunks[2..3], &snapshot, true, cx);
    });

    // Both staged hunks appear as pending.
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (
                    0..0,
                    "zero\n",
                    "",
                    DiffHunkStatus::deleted(SecondaryHunkRemovalPending),
                ),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(SecondaryHunkRemovalPending),
                ),
            ],
        );
    });

    // Both staging operations take effect.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        assert_hunks(
            diff.snapshot(cx).hunks(&snapshot),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[
                (0..0, "zero\n", "", DiffHunkStatus::deleted(NoSecondaryHunk)),
                (
                    1..2,
                    "two\n",
                    "TWO\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
                (
                    3..4,
                    "four\n",
                    "FOUR\n",
                    DiffHunkStatus::modified(NoSecondaryHunk),
                ),
            ],
        );
    });
}
