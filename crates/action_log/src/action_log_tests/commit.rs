use super::common::*;

#[gpui::test]
async fn test_keep_edits_on_commit(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "file.txt": "a\nb\nc\nd\ne\nf\ng\nh\ni\nj",
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "a\nb\nc\nd\ne\nf\ng\nh\ni\nj".into())],
        "0000000",
    );
    cx.run_until_parked();

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path(path!("/project/file.txt"), cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer.edit(
                [
                    // Edit at the very start: a -> A
                    (Point::new(0, 0)..Point::new(0, 1), "A"),
                    // Deletion in the middle: remove lines d and e
                    (Point::new(3, 0)..Point::new(5, 0), ""),
                    // Modification: g -> GGG
                    (Point::new(6, 0)..Point::new(6, 1), "GGG"),
                    // Addition: insert new line after h
                    (Point::new(7, 1)..Point::new(7, 1), "\nNEW"),
                    // Edit the very last character: j -> J
                    (Point::new(9, 0)..Point::new(9, 1), "J"),
                ],
                None,
                cx,
            );
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(0, 0)..Point::new(1, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "a\n".into()
                },
                HunkStatus {
                    range: Point::new(3, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "d\ne\n".into()
                },
                HunkStatus {
                    range: Point::new(4, 0)..Point::new(5, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "g\n".into()
                },
                HunkStatus {
                    range: Point::new(6, 0)..Point::new(7, 0),
                    diff_status: DiffHunkStatusKind::Added,
                    old_text: "".into()
                },
                HunkStatus {
                    range: Point::new(8, 0)..Point::new(8, 1),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "j".into()
                }
            ]
        )]
    );

    // Simulate a git commit that matches some edits but not others:
    // - Accepts the first edit (a -> A)
    // - Accepts the deletion (remove d and e)
    // - Makes a different change to g (g -> G instead of GGG)
    // - Ignores the NEW line addition
    // - Ignores the last line edit (j stays as j)
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "A\nb\nc\nf\nG\nh\ni\nj".into())],
        "0000001",
    );
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(4, 0)..Point::new(5, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "g\n".into()
                },
                HunkStatus {
                    range: Point::new(6, 0)..Point::new(7, 0),
                    diff_status: DiffHunkStatusKind::Added,
                    old_text: "".into()
                },
                HunkStatus {
                    range: Point::new(8, 0)..Point::new(8, 1),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "j".into()
                }
            ]
        )]
    );

    // Make another commit that accepts the NEW line but with different content
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "A\nb\nc\nf\nGGG\nh\nDIFFERENT\ni\nj".into())],
        "0000002",
    );
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer,
            vec![
                HunkStatus {
                    range: Point::new(6, 0)..Point::new(7, 0),
                    diff_status: DiffHunkStatusKind::Added,
                    old_text: "".into()
                },
                HunkStatus {
                    range: Point::new(8, 0)..Point::new(8, 1),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "j".into()
                }
            ]
        )]
    );

    // Final commit that accepts all remaining edits
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "A\nb\nc\nf\nGGG\nh\nNEW\ni\nJ".into())],
        "0000003",
    );
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test]
async fn test_keep_edits_on_commit_with_shifted_diff_boundaries(cx: &mut TestAppContext) {
    init_test(cx);

    let initial_text = indoc! {"
            use crate::{Alpha, Beta};

            fn keep() {
                work();
            }

            fn remove() {
                work();
            }

            fn after() {
                work();
            }
        "};
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "file.rs": initial_text,
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.rs", initial_text.into())],
        "0000000",
    );
    cx.run_until_parked();

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path(path!("/project/file.rs"), cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    let final_text = indoc! {"
            use crate::{Alpha};

            fn keep() {
                work();
            }

            fn after() {
                work();
            }
        "};

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer.set_text(final_text, cx);
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert!(!unreviewed_hunks(&action_log, cx).is_empty());

    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.rs", final_text.into())],
        "0000001",
    );
    cx.run_until_parked();

    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

/// Regression test: when head_commit updates before the BufferDiff's base
/// text does, an intermediate DiffChanged (e.g. from a buffer-edit diff
/// recalculation) must NOT consume the commit signal.  The subscription
/// should only fire once the base text itself has changed.
#[gpui::test]
async fn test_keep_edits_on_commit_with_stale_diff_changed(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".git": {},
            "file.txt": "aaa\nbbb\nccc\nddd\neee",
        }),
    )
    .await;
    fs.set_head_for_repo(
        path!("/project/.git").as_ref(),
        &[("file.txt", "aaa\nbbb\nccc\nddd\neee".into())],
        "0000000",
    );
    cx.run_until_parked();

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));

    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path(path!("/project/file.txt"), cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    // Agent makes an edit: bbb -> BBB
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(Point::new(1, 0)..Point::new(1, 3), "BBB")], None, cx);
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    // Verify the edit is tracked
    let hunks = unreviewed_hunks(&action_log, cx);
    assert_eq!(hunks.len(), 1);
    let hunk = &hunks[0].1;
    assert_eq!(hunk.len(), 1);
    assert_eq!(hunk[0].old_text, "bbb\n");

    // Simulate the race condition: update only the HEAD SHA first,
    // without changing the committed file contents. This is analogous
    // to compute_snapshot updating head_commit before
    // reload_buffer_diff_bases has loaded the new base text.
    fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
        state.refs.insert("HEAD".into(), "0000001".into());
    })
    .unwrap();
    cx.run_until_parked();

    // Make a user edit (on a different line) to trigger a buffer diff
    // recalculation.  This fires DiffChanged while the BufferDiff base
    // text is still the OLD text.  With the old head_commit-based
    // subscription this would "consume" the commit detection.
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(Point::new(3, 0)..Point::new(3, 3), "DDD")], None, cx);
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    // Now update the committed file contents to match the buffer
    // (the agent edit was committed). Keep the same SHA so head_commit
    // does NOT change again — this is the second half of the race.
    {
        use git::repository::repo_path;
        fs.with_git_state(path!("/project/.git").as_ref(), true, |state| {
            state
                .head_contents
                .insert(repo_path("file.txt"), "aaa\nBBB\nccc\nDDD\neee".into());
        })
        .unwrap();
    }
    cx.run_until_parked();

    // The agent's edit (bbb -> BBB) should be accepted because the
    // committed content now matches. Only the user edit (ddd -> DDD)
    // should remain, but since the user edit is tracked as coming from
    // the user (ChangeAuthor::User) it would have been rebased into
    // the diff base already. So no unreviewed hunks should remain.
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![],
        "agent edits should have been accepted after the base text update"
    );
}
