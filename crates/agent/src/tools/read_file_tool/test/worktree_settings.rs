use super::*;

#[gpui::test]
async fn test_read_image_symlink_requires_authorization(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;
    fs.insert_tree(path!("/outside"), json!({})).await;
    fs.insert_file(path!("/outside/secret.png"), single_pixel_png())
        .await;
    fs.insert_symlink(
        path!("/root/secret.png"),
        PathBuf::from("/outside/secret.png"),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let read_task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(ReadFileToolInput {
                path: "root/secret.png".to_string(),
                start_line: None,
                end_line: None,
            }),
            event_stream,
            cx,
        )
    });

    let authorization = event_rx.expect_authorization().await;
    assert!(
        authorization
            .tool_call
            .fields
            .title
            .as_deref()
            .is_some_and(|title| title.contains("points outside the project")),
        "Expected symlink escape authorization before reading the image"
    );
    authorization
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();

    let result = read_task.await;
    assert!(result.is_ok());
}

#[gpui::test]
async fn test_read_file_with_multiple_worktree_settings(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    // Create first worktree with its own private_files setting
    fs.insert_tree(
        path!("/worktree1"),
        json!({
            "src": {
                "main.rs": "fn main() { println!(\"Hello from worktree1\"); }",
                "secret.rs": "const API_KEY: &str = \"secret_key_1\";",
                "config.toml": "[database]\nurl = \"postgres://localhost/db1\""
            },
            "tests": {
                "test.rs": "mod tests { fn test_it() {} }",
                "fixture.sql": "CREATE TABLE users (id INT, name VARCHAR(255));"
            },
            ".mav": {
                "settings.json": r#"{
                    "file_scan_exclusions": ["**/fixture.*"],
                    "private_files": ["**/secret.rs", "**/config.toml"]
                }"#
            }
        }),
    )
    .await;

    // Create second worktree with different private_files setting
    fs.insert_tree(
        path!("/worktree2"),
        json!({
            "lib": {
                "public.js": "export function greet() { return 'Hello from worktree2'; }",
                "private.js": "const SECRET_TOKEN = \"private_token_2\";",
                "data.json": "{\"api_key\": \"json_secret_key\"}"
            },
            "docs": {
                "README.md": "# Public Documentation",
                "internal.md": "# Internal Secrets and Configuration"
            },
            ".mav": {
                "settings.json": r#"{
                    "file_scan_exclusions": ["**/internal.*"],
                    "private_files": ["**/private.js", "**/data.json"]
                }"#
            }
        }),
    )
    .await;

    // Set global settings
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions =
                    Some(vec!["**/.git".to_string(), "**/node_modules".to_string()]);
                settings.project.worktree.private_files = Some(vec!["**/.env".to_string()].into());
            });
        });
    });

    let project = Project::test(
        fs.clone(),
        [path!("/worktree1").as_ref(), path!("/worktree2").as_ref()],
        cx,
    )
    .await;

    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project.clone(), action_log.clone(), true));

    // Test reading allowed files in worktree1
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "worktree1/src/main.rs".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();

    assert_eq!(
        result,
        "     1\tfn main() { println!(\"Hello from worktree1\"); }".into()
    );

    // Test reading private file in worktree1 should fail
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "worktree1/src/secret.rs".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    assert!(result.is_err());
    assert!(
        error_text(result.unwrap_err()).contains("worktree `private_files` setting"),
        "Error should mention worktree private_files setting"
    );

    // Test reading excluded file in worktree1 should fail
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "worktree1/tests/fixture.sql".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    assert!(result.is_err());
    assert!(
        error_text(result.unwrap_err()).contains("worktree `file_scan_exclusions` setting"),
        "Error should mention worktree file_scan_exclusions setting"
    );

    // Test reading allowed files in worktree2
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "worktree2/lib/public.js".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();

    assert_eq!(
        result,
        "     1\texport function greet() { return 'Hello from worktree2'; }".into()
    );

    // Test reading private file in worktree2 should fail
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "worktree2/lib/private.js".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    assert!(result.is_err());
    assert!(
        error_text(result.unwrap_err()).contains("worktree `private_files` setting"),
        "Error should mention worktree private_files setting"
    );

    // Test reading excluded file in worktree2 should fail
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "worktree2/docs/internal.md".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    assert!(result.is_err());
    assert!(
        error_text(result.unwrap_err()).contains("worktree `file_scan_exclusions` setting"),
        "Error should mention worktree file_scan_exclusions setting"
    );

    // Test that files allowed in one worktree but not in another are handled correctly
    // (e.g., config.toml is private in worktree1 but doesn't exist in worktree2)
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "worktree1/src/config.toml".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.clone().run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    assert!(result.is_err());
    assert!(
        error_text(result.unwrap_err()).contains("worktree `private_files` setting"),
        "Config.toml should be blocked by worktree1's private_files setting"
    );
}
