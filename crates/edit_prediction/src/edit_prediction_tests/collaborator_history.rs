use super::*;

#[gpui::test]
async fn test_nearby_collaborator_edits_are_kept_in_history(cx: &mut TestAppContext) {
    let (ep_store, _requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.rs": "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\nline 13\nline 14\n"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.rs"), cx).unwrap();
            project.set_active_path(Some(path.clone()), cx);
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    let cursor = buffer.read_with(cx, |buffer, _cx| buffer.anchor_before(Point::new(1, 0)));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
        let _ = ep_store.prediction_at(&buffer, Some(cursor), &project, cx);
    });

    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(0..6, "LOCAL ZERO")], None, cx);
    });

    let (collaborator, mut collaborator_version) = make_collaborator_replica(&buffer, cx);

    let (line_one_start, line_one_len) = collaborator.read_with(cx, |buffer, _cx| {
        (Point::new(1, 0).to_offset(buffer), buffer.line_len(1))
    });

    apply_collaborator_edit(
        &collaborator,
        &buffer,
        &mut collaborator_version,
        line_one_start..line_one_start + line_one_len as usize,
        "REMOTE ONE",
        cx,
    )
    .await;

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });

    assert_eq!(
        render_events_with_predicted(&events),
        vec![indoc! {"
            manual
            @@ -1,5 +1,5 @@
            -line 0
            -line 1
            +LOCAL ZERO
            +REMOTE ONE
             line 2
             line 3
             line 4
        "}]
    );
}

#[gpui::test]
async fn test_distant_collaborator_edits_are_omitted_from_history(cx: &mut TestAppContext) {
    let (ep_store, _requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.rs": (0..1000)
                .map(|i| format!("line {i}\n"))
                .collect::<String>()
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.rs"), cx).unwrap();
            project.set_active_path(Some(path.clone()), cx);
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    let cursor = buffer.read_with(cx, |buffer, _cx| buffer.anchor_before(Point::new(1, 0)));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
        let _ = ep_store.prediction_at(&buffer, Some(cursor), &project, cx);
    });

    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(0..6, "LOCAL ZERO")], None, cx);
    });

    let (collaborator, mut collaborator_version) = make_collaborator_replica(&buffer, cx);

    let far_line_start = buffer.read_with(cx, |buffer, _cx| Point::new(900, 0).to_offset(buffer));

    apply_collaborator_edit(
        &collaborator,
        &buffer,
        &mut collaborator_version,
        far_line_start..far_line_start + 7,
        "REMOTE FAR",
        cx,
    )
    .await;

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });

    assert_eq!(
        render_events_with_predicted(&events),
        vec![indoc! {"
            manual
            @@ -1,4 +1,4 @@
            -line 0
            +LOCAL ZERO
             line 1
             line 2
             line 3
        "}]
    );
}

#[gpui::test]
async fn test_irrelevant_collaborator_edits_in_different_files_are_omitted_from_history(
    cx: &mut TestAppContext,
) {
    let (ep_store, _requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.rs": "line 0\nline 1\nline 2\nline 3\n",
            "bar.rs": "line 0\nline 1\nline 2\nline 3\n"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let foo_buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.rs"), cx).unwrap();
            project.set_active_path(Some(path.clone()), cx);
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();
    let bar_buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/bar.rs"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    let foo_cursor = foo_buffer.read_with(cx, |buffer, _cx| buffer.anchor_before(Point::new(1, 0)));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&foo_buffer, &project, cx);
        ep_store.register_buffer(&bar_buffer, &project, cx);
        let _ = ep_store.prediction_at(&foo_buffer, Some(foo_cursor), &project, cx);
    });

    let (bar_collaborator, mut bar_version) = make_collaborator_replica(&bar_buffer, cx);

    apply_collaborator_edit(
        &bar_collaborator,
        &bar_buffer,
        &mut bar_version,
        0..6,
        "REMOTE BAR",
        cx,
    )
    .await;

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });

    assert!(events.is_empty());
}

#[gpui::test]
async fn test_large_edits_are_omitted_from_history(cx: &mut TestAppContext) {
    let (ep_store, _requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.rs": (0..20)
                .map(|i| format!("line {i}\n"))
                .collect::<String>()
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.rs"), cx).unwrap();
            project.set_active_path(Some(path.clone()), cx);
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    let cursor = buffer.read_with(cx, |buffer, _cx| buffer.anchor_before(Point::new(1, 0)));

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
        let _ = ep_store.prediction_at(&buffer, Some(cursor), &project, cx);
    });

    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(0..6, "LOCAL ZERO")], None, cx);
    });

    let (collaborator, mut collaborator_version) = make_collaborator_replica(&buffer, cx);

    let (line_three_start, line_three_len) = collaborator.read_with(cx, |buffer, _cx| {
        (Point::new(3, 0).to_offset(buffer), buffer.line_len(3))
    });
    let large_edit = "X".repeat(EDIT_HISTORY_DIFF_SIZE_LIMIT + 1);

    apply_collaborator_edit(
        &collaborator,
        &buffer,
        &mut collaborator_version,
        line_three_start..line_three_start + line_three_len as usize,
        &large_edit,
        cx,
    )
    .await;

    buffer.update(cx, |buffer, cx| {
        let line_seven_start = Point::new(7, 0).to_offset(buffer);
        let line_seven_end = Point::new(7, 6).to_offset(buffer);
        buffer.edit(
            vec![(line_seven_start..line_seven_end, "LOCAL SEVEN")],
            None,
            cx,
        );
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });

    let rendered_events = render_events_with_predicted(&events);

    assert_eq!(rendered_events.len(), 2);
    assert!(rendered_events[0].contains("+LOCAL ZERO"));
    assert!(!rendered_events[0].contains(&large_edit));
    assert!(rendered_events[1].contains("+LOCAL SEVEN"));
    assert!(!rendered_events[1].contains(&large_edit));
}
