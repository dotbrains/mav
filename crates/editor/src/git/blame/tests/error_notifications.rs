use super::*;

#[gpui::test]
async fn test_blame_error_notifications(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/my-repo",
        json!({
            ".git": {},
            "file.txt": r#"
                    irrelevant contents
                "#
            .unindent()
        }),
    )
    .await;

    // Creating a GitBlame without a corresponding blame state
    // will result in an error.

    let project = Project::test(fs, ["/my-repo".as_ref()], cx).await;
    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer("/my-repo/file.txt", cx)
        })
        .await
        .unwrap();
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

    let blame = cx.new(|cx| GitBlame::new(buffer.clone(), project.clone(), true, true, cx));

    let event = project.next_event(cx).await;
    assert_eq!(
        event,
        project::Event::Toast {
            notification_id: "git-blame".into(),
            message: "Failed to blame \"file.txt\": failed to get blame for \"file.txt\""
                .to_string(),
            link: None
        }
    );

    blame.update(cx, |blame, cx| {
        assert_eq!(
            blame
                .blame_for_rows(
                    &(0..1)
                        .map(|row| RowInfo {
                            buffer_row: Some(row),
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
