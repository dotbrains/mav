use super::*;

#[gpui::test]
async fn test_edit_history_getter_pause_splits_last_event(cx: &mut TestAppContext) {
    let (ep_store, _requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "foo.md": "Hello!\n\nBye\n"
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.md"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
    });

    // First burst: insert "How"
    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(7..7, "How")], None, cx);
    });

    // Simulate a pause longer than the grouping threshold (e.g. 500ms).
    cx.executor().advance_clock(LAST_CHANGE_GROUPING_TIME * 2);
    cx.run_until_parked();

    // Second burst: append " are you?" immediately after "How" on the same line.
    //
    // Keeping both bursts on the same line ensures the existing line-span coalescing logic
    // groups them into a single `LastEvent`, allowing the pause-split getter to return two diffs.
    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(10..10, " are you?")], None, cx);
    });

    // A second edit shortly after the first post-pause edit ensures the last edit timestamp is
    // advanced after the pause boundary is recorded, making pause-splitting deterministic.
    buffer.update(cx, |buffer, cx| {
        buffer.edit(vec![(19..19, "!")], None, cx);
    });

    // With time-based splitting, there are two distinct events.
    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(events.len(), 2);

    let first_total_edit_range = buffer.read_with(cx, |buffer, _| {
        events[0].total_edit_range.to_point(&buffer.snapshot())
    });
    assert_eq!(first_total_edit_range, Point::new(1, 0)..Point::new(1, 3));

    let zeta_prompt::Event::BufferChange { diff, .. } = events[0].event.as_ref();
    assert_eq!(
        diff.as_str(),
        indoc! {"
            @@ -1,3 +1,3 @@
             Hello!
            -
            +How
             Bye
        "}
    );

    let second_total_edit_range = buffer.read_with(cx, |buffer, _| {
        events[1].total_edit_range.to_point(&buffer.snapshot())
    });
    assert_eq!(second_total_edit_range, Point::new(1, 3)..Point::new(1, 13));

    let zeta_prompt::Event::BufferChange { diff, .. } = events[1].event.as_ref();
    assert_eq!(
        diff.as_str(),
        indoc! {"
            @@ -1,3 +1,3 @@
             Hello!
            -How
            +How are you?!
             Bye
        "}
    );
}

#[gpui::test]
async fn test_predicted_edits_are_separated_in_edit_history(cx: &mut TestAppContext) {
    let (ep_store, _requests) = init_test_with_fake_client(cx);
    let fs = FakeFs::new(cx.executor());

    // Create a file with 30 lines to test line-based coalescing
    let content = (1..=30)
        .map(|i| format!("Line {}\n", i))
        .collect::<String>();
    fs.insert_tree(
        "/root",
        json!({
            "foo.md": content
        }),
    )
    .await;
    let project = Project::test(fs, vec![path!("/root").as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            let path = project.find_project_path(path!("root/foo.md"), cx).unwrap();
            project.open_buffer(path, cx)
        })
        .await
        .unwrap();

    ep_store.update(cx, |ep_store, cx| {
        ep_store.register_buffer(&buffer, &project, cx);
    });

    // First edit: multi-line edit spanning rows 10-12 (replacing lines 11-13)
    buffer.update(cx, |buffer, cx| {
        let start = Point::new(10, 0).to_offset(buffer);
        let end = Point::new(13, 0).to_offset(buffer);
        buffer.edit(vec![(start..end, "Middle A\nMiddle B\n")], None, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(
        render_events(&events),
        indoc! {"
            @@ -8,9 +8,8 @@
             Line 8
             Line 9
             Line 10
            -Line 11
            -Line 12
            -Line 13
            +Middle A
            +Middle B
             Line 14
             Line 15
             Line 16
        "},
        "After first edit"
    );

    // Second edit: insert ABOVE the first edit's range (row 5, within 8 lines of row 10)
    // This tests that coalescing considers the START of the existing range
    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(5, 0).to_offset(buffer);
        buffer.edit(vec![(offset..offset, "Above\n")], None, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(
        render_events(&events),
        indoc! {"
            @@ -3,14 +3,14 @@
             Line 3
             Line 4
             Line 5
            +Above
             Line 6
             Line 7
             Line 8
             Line 9
             Line 10
            -Line 11
            -Line 12
            -Line 13
            +Middle A
            +Middle B
             Line 14
             Line 15
             Line 16
        "},
        "After inserting above (should coalesce)"
    );

    // Third edit: insert BELOW the first edit's range (row 14 in current buffer, within 8 lines of row 12)
    // This tests that coalescing considers the END of the existing range
    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(14, 0).to_offset(buffer);
        buffer.edit(vec![(offset..offset, "Below\n")], None, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(
        render_events(&events),
        indoc! {"
            @@ -3,15 +3,16 @@
             Line 3
             Line 4
             Line 5
            +Above
             Line 6
             Line 7
             Line 8
             Line 9
             Line 10
            -Line 11
            -Line 12
            -Line 13
            +Middle A
            +Middle B
             Line 14
            +Below
             Line 15
             Line 16
             Line 17
        "},
        "After inserting below (should coalesce)"
    );

    // Fourth edit: insert FAR BELOW (row 25, beyond 8 lines from the current range end ~row 15)
    // This should NOT coalesce - creates a new event
    buffer.update(cx, |buffer, cx| {
        let offset = Point::new(25, 0).to_offset(buffer);
        buffer.edit(vec![(offset..offset, "Far below\n")], None, cx);
    });

    let events = ep_store.update(cx, |ep_store, cx| {
        ep_store.edit_history_for_project(&project, cx)
    });
    assert_eq!(
        render_events(&events),
        indoc! {"
            @@ -3,15 +3,16 @@
             Line 3
             Line 4
             Line 5
            +Above
             Line 6
             Line 7
             Line 8
             Line 9
             Line 10
            -Line 11
            -Line 12
            -Line 13
            +Middle A
            +Middle B
             Line 14
            +Below
             Line 15
             Line 16
             Line 17

            ---
            @@ -23,6 +23,7 @@
             Line 22
             Line 23
             Line 24
            +Far below
             Line 25
             Line 26
             Line 27
        "},
        "After inserting far below (should NOT coalesce)"
    );
}

fn render_events(events: &[StoredEvent]) -> String {
    events
        .iter()
        .map(|e| {
            let zeta_prompt::Event::BufferChange { diff, .. } = e.event.as_ref();
            diff.as_str()
        })
        .collect::<Vec<_>>()
        .join("\n---\n")
}
