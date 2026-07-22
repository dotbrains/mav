use super::*;

#[gpui::test]
async fn test_read_file_symlink_escape_requests_authorization(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "main.rs": "fn main() {}" }
            },
            "external": {
                "secret.txt": "SECRET_KEY=abc123"
            }
        }),
    )
    .await;

    fs.create_symlink(
        path!("/root/project/secret_link.txt").as_ref(),
        PathBuf::from("../external/secret.txt"),
    )
    .await
    .unwrap();

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project.clone(), action_log, true));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.clone().run(
            ToolInput::resolved(ReadFileToolInput {
                path: "project/secret_link.txt".to_string(),
                start_line: None,
                end_line: None,
            }),
            event_stream,
            cx,
        )
    });

    let auth = event_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("points outside the project"),
        "title: {title}"
    );

    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();

    let result = task.await;
    assert!(result.is_ok(), "should succeed after approval: {result:?}");
}

#[gpui::test]
async fn test_read_file_symlink_escape_denied(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "main.rs": "fn main() {}" }
            },
            "external": {
                "secret.txt": "SECRET_KEY=abc123"
            }
        }),
    )
    .await;

    fs.create_symlink(
        path!("/root/project/secret_link.txt").as_ref(),
        PathBuf::from("../external/secret.txt"),
    )
    .await
    .unwrap();

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project.clone(), action_log, true));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        tool.clone().run(
            ToolInput::resolved(ReadFileToolInput {
                path: "project/secret_link.txt".to_string(),
                start_line: None,
                end_line: None,
            }),
            event_stream,
            cx,
        )
    });

    let auth = event_rx.expect_authorization().await;
    drop(auth);

    let result = task.await;
    assert!(
        result.is_err(),
        "Tool should fail when authorization is denied"
    );
}

#[gpui::test]
async fn test_read_file_symlink_escape_private_path_no_authorization(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "project": {
                "src": { "main.rs": "fn main() {}" }
            },
            "external": {
                "secret.txt": "SECRET_KEY=abc123"
            }
        }),
    )
    .await;

    fs.create_symlink(
        path!("/root/project/secret_link.txt").as_ref(),
        PathBuf::from("../external/secret.txt"),
    )
    .await
    .unwrap();

    cx.update(|cx| {
        settings::SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.private_files =
                    Some(vec!["**/secret_link.txt".to_string()].into());
            });
        });
    });

    let project = Project::test(fs.clone(), [path!("/root/project").as_ref()], cx).await;
    cx.executor().run_until_parked();

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project.clone(), action_log, true));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let result = cx
        .update(|cx| {
            tool.clone().run(
                ToolInput::resolved(ReadFileToolInput {
                    path: "project/secret_link.txt".to_string(),
                    start_line: None,
                    end_line: None,
                }),
                event_stream,
                cx,
            )
        })
        .await;

    assert!(
        result.is_err(),
        "Expected read_file to fail on private path"
    );
    let error = error_text(result.unwrap_err());
    assert!(
        error.contains("private_files"),
        "Expected private-files validation error, got: {error}"
    );

    let event = event_rx.try_recv();
    assert!(
        !matches!(
            event,
            Ok(Ok(crate::thread::ThreadEvent::ToolCallAuthorization(_)))
        ),
        "No authorization should be requested when validation fails before read",
    );
}
