use super::common::*;

#[gpui::test(iterations = 10)]
async fn test_reject_edits(cx: &mut TestAppContext) {
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
                .edit([(Point::new(1, 1)..Point::new(1, 2), "E\nXYZ")], None, cx)
                .unwrap()
        });
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(5, 2)..Point::new(5, 3), "O")], None, cx)
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndE\nXYZf\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(5, 0)..Point::new(5, 3),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "mno".into(),
                }
            ],
        )]
    );

    // If the rejected range doesn't overlap with any hunk, we ignore it.
    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(4, 0)..Point::new(4, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndE\nXYZf\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(5, 0)..Point::new(5, 3),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "mno".into(),
                }
            ],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(0, 0)..Point::new(1, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndef\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(4, 0)..Point::new(4, 3),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "mno".into(),
            }],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(4, 0)..Point::new(4, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndef\nghi\njkl\nmno"
    );
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_reject_multiple_edits(cx: &mut TestAppContext) {
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
                .edit([(Point::new(1, 1)..Point::new(1, 2), "E\nXYZ")], None, cx)
                .unwrap()
        });
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(5, 2)..Point::new(5, 3), "O")], None, cx)
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndE\nXYZf\nghi\njkl\nmnO"
    );
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![
                HunkStatus {
                    range: Point::new(1, 0)..Point::new(3, 0),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "def\n".into(),
                },
                HunkStatus {
                    range: Point::new(5, 0)..Point::new(5, 3),
                    diff_status: DiffHunkStatusKind::Modified,
                    old_text: "mno".into(),
                }
            ],
        )]
    );

    action_log.update(cx, |log, cx| {
        let range_1 = buffer.read(cx).anchor_before(Point::new(0, 0))
            ..buffer.read(cx).anchor_before(Point::new(1, 0));
        let range_2 = buffer.read(cx).anchor_before(Point::new(5, 0))
            ..buffer.read(cx).anchor_before(Point::new(5, 3));

        let (task, _) =
            log.reject_edits_in_ranges(buffer.clone(), vec![range_1, range_2], None, cx);
        task.detach();
        assert_eq!(
            buffer.read_with(cx, |buffer, _| buffer.text()),
            "abc\ndef\nghi\njkl\nmno"
        );
    });
    cx.run_until_parked();
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndef\nghi\njkl\nmno"
    );
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_reject_deleted_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "content"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path.clone(), cx))
        .await
        .unwrap();

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.will_delete_buffer(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| {
            project.delete_file(file_path.clone(), false, cx)
        })
        .unwrap()
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(!fs.is_file(path!("/dir/file").as_ref()).await);
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 0),
                diff_status: DiffHunkStatusKind::Deleted,
                old_text: "content".into(),
            }]
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(0, 0)..Point::new(0, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(buffer.read_with(cx, |buffer, _| buffer.text()), "content");
    assert!(fs.is_file(path!("/dir/file").as_ref()).await);
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_reject_created_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/new_file", cx)
        })
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("content", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 7),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(0, 0)..Point::new(0, 11)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(!fs.is_file(path!("/dir/new_file").as_ref()).await);
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}
