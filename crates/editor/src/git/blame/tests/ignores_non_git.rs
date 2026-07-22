use super::*;

#[gpui::test]
async fn test_blame_ignores_buffers_outside_git_repositories(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/not-a-repo",
        json!({
            "foo": "bar",
        }),
    )
    .await;

    let project = Project::test(fs, ["/not-a-repo".as_ref()], cx).await;

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/not-a-repo/foo", cx)
        })
        .await
        .unwrap();

    let buffer_id = buffer.read_with(cx, |buffer, _| buffer.remote_id());

    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

    let events = Arc::new(Mutex::new(Vec::new()));

    let _subscription = project.update(cx, |_, cx| {
        cx.subscribe(&project, {
            let events = events.clone();

            move |_, _, event: &project::Event, _| {
                events
                    .lock()
                    .expect("events mutex poisoned")
                    .push(event.clone());
            }
        })
    });

    let blame = cx.new(|cx| GitBlame::new(buffer.clone(), project.clone(), true, true, cx));

    cx.executor().run_until_parked();

    assert!(events.lock().expect("events mutex poisoned").is_empty());

    blame.update(cx, |blame, cx| {
        assert_eq!(
            blame
                .blame_for_rows(
                    &(0..1)
                        .map(|row| RowInfo {
                            buffer_row: Some(row),
                            buffer_id: Some(buffer_id),
                            ..Default::default()
                        })
                        .collect::<Vec<_>>(),
                    cx
                )
                .collect::<Vec<_>>(),
            vec![None]
        );
    });
}
