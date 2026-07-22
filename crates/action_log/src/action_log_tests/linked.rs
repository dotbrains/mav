use super::common::*;

#[gpui::test]
async fn test_undo_last_reject(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file1": "abc\ndef\nghi"
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

    // Track the buffer and make an agent edit
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit(
                    [(Point::new(1, 0)..Point::new(1, 3), "AGENT_EDIT")],
                    None,
                    cx,
                )
                .unwrap()
        });
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    // Verify the agent edit is there
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\nAGENT_EDIT\nghi"
    );
    assert!(!unreviewed_hunks(&action_log, cx).is_empty());

    // Reject all edits
    action_log
        .update(cx, |log, cx| log.reject_all_edits(None, cx))
        .await;
    cx.run_until_parked();

    // Verify the buffer is back to original
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\ndef\nghi"
    );
    assert!(unreviewed_hunks(&action_log, cx).is_empty());

    // Verify undo state is available
    assert!(action_log.read_with(cx, |log, _| log.has_pending_undo()));

    // Undo the reject
    action_log
        .update(cx, |log, cx| log.undo_last_reject(cx))
        .await;

    cx.run_until_parked();

    // Verify the agent edit is restored
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "abc\nAGENT_EDIT\nghi"
    );

    // Verify undo state is cleared
    assert!(!action_log.read_with(cx, |log, _| log.has_pending_undo()));
}

#[gpui::test]
async fn test_linked_action_log_buffer_read(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "hello world"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
    });

    // Neither log considers the buffer stale immediately after reading it.
    let child_stale = cx.read(|cx| {
        child_log
            .read(cx)
            .stale_buffers(cx)
            .cloned()
            .collect::<Vec<_>>()
    });
    let parent_stale = cx.read(|cx| {
        parent_log
            .read(cx)
            .stale_buffers(cx)
            .cloned()
            .collect::<Vec<_>>()
    });
    assert!(child_stale.is_empty());
    assert!(parent_stale.is_empty());

    // Simulate a user edit after the agent read the file.
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(0..5, "goodbye")], None, cx).unwrap();
        });
    });
    cx.run_until_parked();

    // Both child and parent should see the buffer as stale because both tracked
    // it at the pre-edit version via buffer_read forwarding.
    let child_stale = cx.read(|cx| {
        child_log
            .read(cx)
            .stale_buffers(cx)
            .cloned()
            .collect::<Vec<_>>()
    });
    let parent_stale = cx.read(|cx| {
        parent_log
            .read(cx)
            .stale_buffers(cx)
            .cloned()
            .collect::<Vec<_>>()
    });
    assert_eq!(child_stale, vec![buffer.clone()]);
    assert_eq!(parent_stale, vec![buffer]);
}

#[gpui::test]
async fn test_linked_action_log_buffer_edited(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "abc\ndef\nghi"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| {
            buffer
                .edit([(Point::new(1, 0)..Point::new(1, 3), "DEF")], None, cx)
                .unwrap();
        });
        child_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    cx.run_until_parked();

    let expected_hunks = vec![(
        buffer,
        vec![HunkStatus {
            range: Point::new(1, 0)..Point::new(2, 0),
            diff_status: DiffHunkStatusKind::Modified,
            old_text: "def\n".into(),
        }],
    )];
    assert_eq!(
        unreviewed_hunks(&child_log, cx),
        expected_hunks,
        "child should track the agent edit"
    );
    assert_eq!(
        unreviewed_hunks(&parent_log, cx),
        expected_hunks,
        "parent should also track the agent edit via linked log forwarding"
    );
}

#[gpui::test]
async fn test_linked_action_log_buffer_created(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

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
        child_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("hello", cx));
        child_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();

    let expected_hunks = vec![(
        buffer.clone(),
        vec![HunkStatus {
            range: Point::new(0, 0)..Point::new(0, 5),
            diff_status: DiffHunkStatusKind::Added,
            old_text: "".into(),
        }],
    )];
    assert_eq!(
        unreviewed_hunks(&child_log, cx),
        expected_hunks,
        "child should track the created file"
    );
    assert_eq!(
        unreviewed_hunks(&parent_log, cx),
        expected_hunks,
        "parent should also track the created file via linked log forwarding"
    );
}

#[gpui::test]
async fn test_linked_action_log_will_delete_buffer(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": "hello\n"}))
        .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path.clone(), cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log.update(cx, |log, cx| log.will_delete_buffer(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.delete_file(file_path, false, cx))
        .unwrap()
        .await
        .unwrap();
    cx.run_until_parked();

    let expected_hunks = vec![(
        buffer.clone(),
        vec![HunkStatus {
            range: Point::new(0, 0)..Point::new(0, 0),
            diff_status: DiffHunkStatusKind::Deleted,
            old_text: "hello\n".into(),
        }],
    )];
    assert_eq!(
        unreviewed_hunks(&child_log, cx),
        expected_hunks,
        "child should track the deleted file"
    );
    assert_eq!(
        unreviewed_hunks(&parent_log, cx),
        expected_hunks,
        "parent should also track the deleted file via linked log forwarding"
    );
}

/// Simulates the subagent scenario: two child logs linked to the same parent, each
/// editing a different file. The parent accumulates all edits while each child
/// only sees its own.
#[gpui::test]
async fn test_linked_action_log_independent_tracking(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "file_a": "content of a",
            "file_b": "content of b",
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let parent_log = cx.new(|_| ActionLog::new(project.clone()));
    let child_log_1 =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));
    let child_log_2 =
        cx.new(|_| ActionLog::new(project.clone()).with_linked_action_log(parent_log.clone()));

    let file_a_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/file_a", cx)
        })
        .unwrap();
    let file_b_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("dir/file_b", cx)
        })
        .unwrap();
    let buffer_a = project
        .update(cx, |project, cx| project.open_buffer(file_a_path, cx))
        .await
        .unwrap();
    let buffer_b = project
        .update(cx, |project, cx| project.open_buffer(file_b_path, cx))
        .await
        .unwrap();

    cx.update(|cx| {
        child_log_1.update(cx, |log, cx| log.buffer_read(buffer_a.clone(), cx));
        buffer_a.update(cx, |buffer, cx| {
            buffer.edit([(0..0, "MODIFIED: ")], None, cx).unwrap();
        });
        child_log_1.update(cx, |log, cx| log.buffer_edited(buffer_a.clone(), cx));

        child_log_2.update(cx, |log, cx| log.buffer_read(buffer_b.clone(), cx));
        buffer_b.update(cx, |buffer, cx| {
            buffer.edit([(0..0, "MODIFIED: ")], None, cx).unwrap();
        });
        child_log_2.update(cx, |log, cx| log.buffer_edited(buffer_b.clone(), cx));
    });
    cx.run_until_parked();

    let child_1_changed: Vec<_> = cx.read(|cx| {
        child_log_1
            .read(cx)
            .changed_buffers(cx)
            .map(|(buffer, _)| buffer)
            .collect()
    });
    let child_2_changed: Vec<_> = cx.read(|cx| {
        child_log_2
            .read(cx)
            .changed_buffers(cx)
            .map(|(buffer, _)| buffer)
            .collect()
    });
    let parent_changed: Vec<_> = cx.read(|cx| {
        parent_log
            .read(cx)
            .changed_buffers(cx)
            .map(|(buffer, _)| buffer)
            .collect()
    });

    assert_eq!(
        child_1_changed,
        vec![buffer_a.clone()],
        "child 1 should only track file_a"
    );
    assert_eq!(
        child_2_changed,
        vec![buffer_b.clone()],
        "child 2 should only track file_b"
    );
    assert_eq!(parent_changed.len(), 2, "parent should track both files");
    assert!(
        parent_changed.contains(&buffer_a) && parent_changed.contains(&buffer_b),
        "parent should contain both buffer_a and buffer_b"
    );
}
