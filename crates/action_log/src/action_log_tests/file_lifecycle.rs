use super::common::*;

#[gpui::test(iterations = 10)]
async fn test_creating_files(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();

    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("lorem", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 5),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    buffer.update(cx, |buffer, cx| buffer.edit([(0..0, "X")], None, cx));
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 6),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    action_log.update(cx, |log, cx| {
        log.keep_edits_in_range(buffer.clone(), 0..5, None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 10)]
async fn test_overwriting_files(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "Lorem ipsum dolor"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();

    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("sit amet consecteur", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 19),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(buffer.clone(), vec![2..5], None, cx);
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
    assert_eq!(
        buffer.read_with(cx, |buffer, _cx| buffer.text()),
        "Lorem ipsum dolor"
    );
}

#[gpui::test(iterations = 10)]
async fn test_overwriting_previously_edited_files(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "Lorem ipsum dolor"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();

    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.append(" sit amet consecteur", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 37),
                diff_status: DiffHunkStatusKind::Modified,
                old_text: "Lorem ipsum dolor".into(),
            }],
        )]
    );

    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("rewritten", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 9),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(buffer.clone(), vec![2..5], None, cx);
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
    assert_eq!(
        buffer.read_with(cx, |buffer, _cx| buffer.text()),
        "Lorem ipsum dolor"
    );
}

#[gpui::test(iterations = 10)]
async fn test_deleting_files(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({"file1": "lorem\n", "file2": "ipsum\n"}),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let file1_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file1", cx))
        .unwrap();
    let file2_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file2", cx))
        .unwrap();

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let buffer1 = project
        .update(cx, |project, cx| {
            project.open_buffer(file1_path.clone(), cx)
        })
        .await
        .unwrap();
    let buffer2 = project
        .update(cx, |project, cx| {
            project.open_buffer(file2_path.clone(), cx)
        })
        .await
        .unwrap();

    action_log.update(cx, |log, cx| log.will_delete_buffer(buffer1.clone(), cx));
    action_log.update(cx, |log, cx| log.will_delete_buffer(buffer2.clone(), cx));
    project
        .update(cx, |project, cx| {
            project.delete_file(file1_path.clone(), false, cx)
        })
        .unwrap()
        .await
        .unwrap();
    project
        .update(cx, |project, cx| {
            project.delete_file(file2_path.clone(), false, cx)
        })
        .unwrap()
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![
            (
                buffer1.clone(),
                vec![HunkStatus {
                    range: Point::new(0, 0)..Point::new(0, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "lorem\n".into(),
                }]
            ),
            (
                buffer2.clone(),
                vec![HunkStatus {
                    range: Point::new(0, 0)..Point::new(0, 0),
                    diff_status: DiffHunkStatusKind::Deleted,
                    old_text: "ipsum\n".into(),
                }],
            )
        ]
    );

    // Simulate file1 being recreated externally.
    fs.insert_file(path!("/dir/file1"), "LOREM".as_bytes().to_vec())
        .await;

    // Simulate file2 being recreated by a tool.
    let buffer2 = project
        .update(cx, |project, cx| project.open_buffer(file2_path, cx))
        .await
        .unwrap();
    action_log.update(cx, |log, cx| log.buffer_created(buffer2.clone(), cx));
    buffer2.update(cx, |buffer, cx| buffer.set_text("IPSUM", cx));
    action_log.update(cx, |log, cx| log.buffer_edited(buffer2.clone(), cx));
    project
        .update(cx, |project, cx| project.save_buffer(buffer2.clone(), cx))
        .await
        .unwrap();

    cx.run_until_parked();
    assert_eq!(
        unreviewed_hunks(&action_log, cx),
        vec![(
            buffer2.clone(),
            vec![HunkStatus {
                range: Point::new(0, 0)..Point::new(0, 5),
                diff_status: DiffHunkStatusKind::Added,
                old_text: "".into(),
            }],
        )]
    );

    // Simulate file2 being deleted externally.
    fs.remove_file(path!("/dir/file2").as_ref(), RemoveOptions::default())
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}
