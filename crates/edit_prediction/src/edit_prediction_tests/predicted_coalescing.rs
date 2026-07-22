use super::*;

#[gpui::test]
async fn test_predicted_flag_coalescing(cx: &mut TestAppContext) {
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
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
    });

    // Case 1: Manual edits have `predicted` set to false.
    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(0..6, "LINE ZERO")], None, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });

    assert_eq!(
        render_events_with_predicted(&events),
        vec![indoc! {"
            manual
            @@ -1,4 +1,4 @@
            -line 0
            +LINE ZERO
             line 1
             line 2
             line 3
        "}]
    );

    // Case 2: Multiple successive manual edits near each other are merged into one
    // event with `predicted` set to false.
    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(1, 0).to_offset(buffer);
        let end = Point::new(1, 6).to_offset(buffer);
        buffer.edit(vec![(offset..end, "LINE ONE")], None, cx);
    });

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
            +LINE ZERO
            +LINE ONE
             line 2
             line 3
             line 4
        "}]
    );

    // Case 3: Accepted predictions have `predicted` set to true.
    // Case 5: A manual edit that follows a predicted edit is not merged with the
    // predicted edit, even if it is nearby.
    ep_store.update(cx, |ep_store, cx| {
        buffer.update(cx, |buffer, cx| {
            let offset = Point::new(2, 0).to_offset(buffer);
            let end = Point::new(2, 6).to_offset(buffer);
            buffer.edit(vec![(offset..end, "LINE TWO")], None, cx);
        });
        ep_store.report_changes_for_buffer(&buffer, &project, true, true, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(
        render_events_with_predicted(&events),
        vec![
            indoc! {"
                manual
                @@ -1,5 +1,5 @@
                -line 0
                -line 1
                +LINE ZERO
                +LINE ONE
                 line 2
                 line 3
                 line 4
            "},
            indoc! {"
                predicted
                @@ -1,6 +1,6 @@
                 LINE ZERO
                 LINE ONE
                -line 2
                +LINE TWO
                 line 3
                 line 4
                 line 5
            "}
        ]
    );

    // Case 4: Multiple successive accepted predictions near each other are merged
    // into one event with `predicted` set to true.
    ep_store.update(cx, |ep_store, cx| {
        buffer.update(cx, |buffer, cx| {
            let offset = Point::new(3, 0).to_offset(buffer);
            let end = Point::new(3, 6).to_offset(buffer);
            buffer.edit(vec![(offset..end, "LINE THREE")], None, cx);
        });
        ep_store.report_changes_for_buffer(&buffer, &project, true, true, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(
        render_events_with_predicted(&events),
        vec![
            indoc! {"
                manual
                @@ -1,5 +1,5 @@
                -line 0
                -line 1
                +LINE ZERO
                +LINE ONE
                 line 2
                 line 3
                 line 4
            "},
            indoc! {"
                predicted
                @@ -1,7 +1,7 @@
                 LINE ZERO
                 LINE ONE
                -line 2
                -line 3
                +LINE TWO
                +LINE THREE
                 line 4
                 line 5
                 line 6
            "}
        ]
    );

    // Case 5 (continued): A manual edit that follows a predicted edit is not merged
    // with the predicted edit, even if it is nearby.
    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(4, 0).to_offset(buffer);
        let end = Point::new(4, 6).to_offset(buffer);
        buffer.edit(vec![(offset..end, "LINE FOUR")], None, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(
        render_events_with_predicted(&events),
        vec![
            indoc! {"
                manual
                @@ -1,5 +1,5 @@
                -line 0
                -line 1
                +LINE ZERO
                +LINE ONE
                 line 2
                 line 3
                 line 4
            "},
            indoc! {"
                predicted
                @@ -1,7 +1,7 @@
                 LINE ZERO
                 LINE ONE
                -line 2
                -line 3
                +LINE TWO
                +LINE THREE
                 line 4
                 line 5
                 line 6
            "},
            indoc! {"
                manual
                @@ -2,7 +2,7 @@
                 LINE ONE
                 LINE TWO
                 LINE THREE
                -line 4
                +LINE FOUR
                 line 5
                 line 6
                 line 7
            "}
        ]
    );

    // Case 6: If we then perform a manual edit at a *different* location (more than
    // 8 lines away), then the edits at the prior location can be merged with each
    // other, even if some are predicted and some are not. `predicted` means all
    // constituent edits were predicted.
    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(14, 0).to_offset(buffer);
        let end = Point::new(14, 7).to_offset(buffer);
        buffer.edit(vec![(offset..end, "LINE FOURTEEN")], None, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(
        render_events_with_predicted(&events),
        vec![
            indoc! {"
                manual
                @@ -1,8 +1,8 @@
                -line 0
                -line 1
                -line 2
                -line 3
                -line 4
                +LINE ZERO
                +LINE ONE
                +LINE TWO
                +LINE THREE
                +LINE FOUR
                 line 5
                 line 6
                 line 7
            "},
            indoc! {"
                manual
                @@ -12,4 +12,4 @@
                 line 11
                 line 12
                 line 13
                -line 14
                +LINE FOURTEEN
            "}
        ]
    );
}
