use super::*;
use pretty_assertions::assert_eq;

#[gpui::test(iterations = 10)]
async fn test_uncommitted_diff_opened_before_unstaged_diff(cx: &mut gpui::TestAppContext) {
    use DiffHunkSecondaryStatus::*;
    init_test(cx);

    let committed_contents = "one\ntwo\nthree\n";
    let file_contents = "one\nTWO\nthree\n";

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/dir",
        json!({
            ".git": {},
            "file.txt": file_contents,
        }),
    )
    .await;
    fs.set_head_and_index_for_repo(
        path!("/dir/.git").as_ref(),
        &[("file.txt", committed_contents.into())],
    );

    let project = Project::test(fs.clone(), ["/dir".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/dir/file.txt", cx)
        })
        .await
        .unwrap();

    let uncommitted_diff_task = project.update(cx, |project, cx| {
        project.open_uncommitted_diff(buffer.clone(), cx)
    });
    let unstaged_diff_task = project.update(cx, |project, cx| {
        project.open_unstaged_diff(buffer.clone(), cx)
    });
    let (uncommitted_diff, unstaged_diff) =
        futures::future::join(uncommitted_diff_task, unstaged_diff_task).await;
    let uncommitted_diff = uncommitted_diff.unwrap();
    let unstaged_diff = unstaged_diff.unwrap();

    cx.run_until_parked();

    uncommitted_diff.read_with(cx, |diff, _| {
        assert_eq!(
            diff.secondary_diff(),
            Some(unstaged_diff.clone()),
            "the unstaged diff returned to callers should be the uncommitted diff's secondary"
        );
    });
    project.read_with(cx, |project, cx| {
        let buffer_id = buffer.read(cx).remote_id();
        assert_eq!(
            project
                .git_store()
                .read(cx)
                .get_unstaged_diff(buffer_id, cx),
            Some(unstaged_diff.clone()),
            "the unstaged diff returned to callers should be the registered one"
        );
    });

    uncommitted_diff.read_with(cx, |diff, cx| {
        let snapshot = buffer.read(cx).snapshot();
        assert_hunks(
            diff.snapshot(cx).hunks_intersecting_range(
                Anchor::min_max_range_for_buffer(snapshot.remote_id()),
                &snapshot,
            ),
            &snapshot,
            &diff.base_text_string(cx).unwrap(),
            &[(
                1..2,
                "two\n",
                "TWO\n",
                DiffHunkStatus::modified(HasSecondaryHunk),
            )],
        );
    });
}

#[gpui::test(seeds(340, 472))]
async fn test_staging_hunks_with_delayed_fs_event(cx: &mut gpui::TestAppContext) {
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

    fs.set_head_for_repo(
        "/dir/.git".as_ref(),
        &[("file.txt", committed_contents.clone())],
        "deadbeef",
    );
    fs.set_index_for_repo(
        "/dir/.git".as_ref(),
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

    // Pause IO events
    fs.pause_events();

    // Stage the first hunk.
    uncommitted_diff.update(cx, |diff, cx| {
        let hunk = diff.snapshot(cx).hunks(&snapshot).next().unwrap();
        diff.stage_or_unstage_hunks(true, &[hunk], &snapshot, true, cx);
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

    // Stage the second hunk *before* receiving the FS event for the first hunk.
    cx.run_until_parked();
    uncommitted_diff.update(cx, |diff, cx| {
        let hunk = diff.snapshot(cx).hunks(&snapshot).nth(1).unwrap();
        diff.stage_or_unstage_hunks(true, &[hunk], &snapshot, true, cx);
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

    // Process the FS event for staging the first hunk (second event is still pending).
    fs.flush_events(1);
    cx.run_until_parked();

    // Stage the third hunk before receiving the second FS event.
    uncommitted_diff.update(cx, |diff, cx| {
        let hunk = diff.snapshot(cx).hunks(&snapshot).nth(2).unwrap();
        diff.stage_or_unstage_hunks(true, &[hunk], &snapshot, true, cx);
    });

    // Wait for all remaining IO.
    cx.run_until_parked();
    fs.flush_events(fs.buffered_event_count());

    // Now all hunks are staged.
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
