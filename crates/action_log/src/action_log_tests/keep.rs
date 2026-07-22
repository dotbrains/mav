use super::common::*;

#[gpui::test(iterations = 10)]
async fn test_keep_edits(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "abc\ndef\nghi\njkl\nmno"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 1)..Point::new(1, 2), "E")], None, cx)
                .unwrap()
        });
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(4, 2)..Point::new(4, 3), "O")], None, cx)
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndEf\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(2, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(4, 0)..Point::new(4, 3),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "mno".into(),
                }
            ],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), Point::new(3, 0)..Point::new(4, 3), None, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(2, 0),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "def\n".into(),
            }],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), Point::new(0, 0)..Point::new(4, 3), None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_deletions(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({"file": "abc\ndef\nghi\njkl\nmno\npqr"}),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 0)..Point::new(2, 0), "")], None, cx)
                .unwrap();
            buffer.finalize_last_transaction();
        });
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(3, 0)..Point::new(4, 0), "")], None, cx)
                .unwrap();
            buffer.finalize_last_transaction();
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\nghi\njkl\npqr"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(1, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(3, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "mno\n".into(),
                }
            ],
        )]
    );

    buffer.update(cx, |buffer, cx| buffer.undo(cx));
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\nghi\njkl\nmno\npqr"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(1, 0),
                diff_status: DiffHunkStatusKind::Deleted,
                old_text: "def\n".into(),
            }],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), Point::new(1, 0)..Point::new(1, 0), None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_overlapping_user_edits(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "abc\ndef\nghi\njkl\nmno"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 2)..Point::new(2, 3), "F\nGHI")], None, cx)
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndeF\nGHI\njkl\nmno"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(3, 0),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "def\nghi\n".into(),
            }],
        )]
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit(
            [
                (Point::new(0, 2)..Point::new(0, 2), "X"),
                (Point::new(3, 0)..Point::new(3, 0), "Y"),
            ],
            None,
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abXc\ndeF\nGHI\nYjkl\nmno"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(3, 0),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "def\nghi\n".into(),
            }],
        )]
    );

    buffer.update(cx, |buffer, cx| {
        buffer.edit([(Point::new(1, 1)..Point::new(1, 1), "Z")], None, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abXc\ndZeF\nGHI\nYjkl\nmno"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(1, 0)..Point::new(3, 0),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "def\nghi\n".into(),
            }],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), Point::new(0, 0)..Point::new(1, 0), None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}
