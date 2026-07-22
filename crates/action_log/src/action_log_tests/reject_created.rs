use super::common::*;

#[gpui::test]
async fn test_reject_created_file_with_user_edits(cx: &mut TestAppContext) {
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

    // AI creates file with initial content
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });

    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();

    cx.run_until_parked();

    // User makes additional edits
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(10..10, "\nuser added this line")], None, cx);
        });
    });

    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();

    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);

    // Reject all
    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Point::new(0, 0)..Point::new(100, 0)],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();

    // File should still contain all the content
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);

    let content = buffer.read_with(cx, |buffer, _| buffer.text());
    assert_eq!(content, "ai content\nuser added this line");
}

#[gpui::test]
async fn test_reject_after_accepting_hunk_on_created_file(cx: &mut TestAppContext) {
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
        .update(cx, |project, cx| project.open_buffer(file_path.clone(), cx))
        .await
        .unwrap();

    // AI creates file with initial content
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content v1", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_ne!(unreviewed_hunks(&action_log, cx), vec![]);

    // User accepts the single hunk
    action_log.update(cx, |log, cx| {
        let buffer_range = Anchor::min_max_range_for_buffer(buffer.read(cx).remote_id());
        log.keep_edits_in_range(buffer.clone(), buffer_range, None, cx)
    });
    cx.run_until_parked();
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);

    // AI modifies the file
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content v2", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_ne!(unreviewed_hunks(&action_log, cx), vec![]);

    // User rejects the hunk
    action_log
        .update(cx, |log, cx| {
            let (task, _) = log.reject_edits_in_ranges(
                buffer.clone(),
                vec![Anchor::min_max_range_for_buffer(
                    buffer.read(cx).remote_id(),
                )],
                None,
                cx,
            );
            task
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await,);
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "ai content v1"
    );
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test]
async fn test_reject_edits_on_previously_accepted_created_file(cx: &mut TestAppContext) {
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
        .update(cx, |project, cx| project.open_buffer(file_path.clone(), cx))
        .await
        .unwrap();

    // AI creates file with initial content
    cx.update(|cx| {
        action_log.update(cx, |log, cx| log.buffer_created(buffer.clone(), cx));
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content v1", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();

    // User clicks "Accept All"
    action_log.update(cx, |log, cx| log.keep_all_edits(None, cx));
    cx.run_until_parked();
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]); // Hunks are cleared

    // AI modifies file again
    cx.update(|cx| {
        buffer.update(cx, |buffer, cx| buffer.set_text("ai content v2", cx));
        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
    });
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();
    cx.run_until_parked();
    assert_ne!(unreviewed_hunks(&action_log, cx), vec![]);

    // User clicks "Reject All"
    action_log
        .update(cx, |log, cx| log.reject_all_edits(None, cx))
        .await;
    cx.run_until_parked();
    assert!(fs.is_file(path!("/dir/new_file").as_ref()).await);
    assert_eq!(
        buffer.read_with(cx, |buffer, _| buffer.text()),
        "ai content v1"
    );
    assert_eq!(unreviewed_hunks(&action_log, cx), vec![]);
}

#[gpui::test(iterations = 100)]
async fn test_random_diffs(mut rng: StdRng, cx: &mut TestAppContext) {
    init_test(cx);

    let operations = env::var("OPERATIONS")
        .map(|i| i.parse().expect("invalid `OPERATIONS` variable"))
        .unwrap_or(20);

    let text = RandomCharIter::new(&mut rng).take(50).collect::<String>();
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({"file": text})).await;
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let file_path = project
        .read_with(cx, |project, cx| project.find_project_path("dir/file", cx))
        .unwrap();
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(file_path, cx))
        .await
        .unwrap();

    action_log.update(cx, |log, cx| log.buffer_read(buffer.clone(), cx));

    for _ in 0..operations {
        match rng.random_range(0..100) {
            0..25 => {
                action_log.update(cx, |log, cx| {
                    let range = buffer.read(cx).random_byte_range(0, &mut rng);
                    log::info!("keeping edits in range {:?}", range);
                    log.keep_edits_in_range(buffer.clone(), range, None, cx)
                });
            }
            25..50 => {
                action_log
                    .update(cx, |log, cx| {
                        let range = buffer.read(cx).random_byte_range(0, &mut rng);
                        log::info!("rejecting edits in range {:?}", range);
                        let (task, _) =
                            log.reject_edits_in_ranges(buffer.clone(), vec![range], None, cx);
                        task
                    })
                    .await
                    .unwrap();
            }
            _ => {
                let is_agent_edit = rng.random_bool(0.5);
                if is_agent_edit {
                    log::info!("agent edit");
                } else {
                    log::info!("user edit");
                }
                cx.update(|cx| {
                    buffer.update(cx, |buffer, cx| buffer.randomly_edit(&mut rng, 1, cx));
                    if is_agent_edit {
                        action_log.update(cx, |log, cx| log.buffer_edited(buffer.clone(), cx));
                    }
                });
            }
        }

        if rng.random_bool(0.2) {
            quiesce(&action_log, &buffer, cx);
        }
    }

    quiesce(&action_log, &buffer, cx);

    fn quiesce(action_log: &Entity<ActionLog>, buffer: &Entity<Buffer>, cx: &mut TestAppContext) {
        log::info!("quiescing...");
        cx.run_until_parked();
        action_log.update(cx, |log, cx| {
            let tracked_buffer = log.tracked_buffers.get(buffer).unwrap();
            let mut old_text = tracked_buffer.diff_base.clone();
            let new_text = buffer.read(cx).as_rope();
            for edit in tracked_buffer.unreviewed_edits.edits() {
                let old_start = old_text.point_to_offset(Point::new(edit.new.start, 0));
                let old_end = old_text.point_to_offset(cmp::min(
                    Point::new(edit.new.start + edit.old_len(), 0),
                    old_text.max_point(),
                ));
                old_text.replace(
                    old_start..old_end,
                    &new_text.slice_rows(edit.new.clone()).to_string(),
                );
            }
            pretty_assertions::assert_eq!(old_text.to_string(), new_text.to_string());
        })
    }
}
