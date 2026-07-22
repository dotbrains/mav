use super::*;

#[gpui::test]
async fn test_read_file_security(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        path!("/"),
        json!({
            "project_root": {
                "allowed_file.txt": "This file is in the project",
                ".mysecrets": "SECRET_KEY=abc123",
                ".secretdir": {
                    "config": "special configuration"
                },
                ".mymetadata": "custom metadata",
                "subdir": {
                    "normal_file.txt": "Normal file content",
                    "special.privatekey": "private key content",
                    "data.mysensitive": "sensitive data"
                }
            },
            "outside_project": {
                "sensitive_file.txt": "This file is outside the project"
            }
        }),
    )
    .await;

    cx.update(|cx| {
        use gpui::UpdateGlobal;
        use settings::SettingsStore;
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(vec![
                    "**/.secretdir".to_string(),
                    "**/.mymetadata".to_string(),
                ]);
                settings.project.worktree.private_files = Some(
                    vec![
                        "**/.mysecrets".to_string(),
                        "**/*.privatekey".to_string(),
                        "**/*.mysensitive".to_string(),
                    ]
                    .into(),
                );
            });
        });
    });

    let project = Project::test(fs.clone(), [path!("/project_root").as_ref()], cx).await;
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    let tool = Arc::new(ReadFileTool::new(project, action_log, true));

    // Reading a file outside the project worktree should fail
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "/outside_project/sensitive_file.txt".to_string(),
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
    assert!(
        result.is_err(),
        "read_file_tool should error when attempting to read an absolute path outside a worktree"
    );

    // Reading a file within the project should succeed
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "project_root/allowed_file.txt".to_string(),
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
    assert!(
        result.is_ok(),
        "read_file_tool should be able to read files inside worktrees"
    );

    // Reading files that match file_scan_exclusions should fail
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "project_root/.secretdir/config".to_string(),
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
    assert!(
        result.is_err(),
        "read_file_tool should error when attempting to read files in .secretdir (file_scan_exclusions)"
    );

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "project_root/.mymetadata".to_string(),
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
    assert!(
        result.is_err(),
        "read_file_tool should error when attempting to read .mymetadata files (file_scan_exclusions)"
    );

    // Reading private files should fail
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "project_root/.mysecrets".to_string(),
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
    assert!(
        result.is_err(),
        "read_file_tool should error when attempting to read .mysecrets (private_files)"
    );

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "project_root/subdir/special.privatekey".to_string(),
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
    assert!(
        result.is_err(),
        "read_file_tool should error when attempting to read .privatekey files (private_files)"
    );

    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "project_root/subdir/data.mysensitive".to_string(),
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
    assert!(
        result.is_err(),
        "read_file_tool should error when attempting to read .mysensitive files (private_files)"
    );

    // Reading a normal file should still work, even with private_files configured
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "project_root/subdir/normal_file.txt".to_string(),
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
    assert!(result.is_ok(), "Should be able to read normal files");
    assert_eq!(result.unwrap(), "     1\tNormal file content".into());

    // Path traversal attempts with .. should fail
    let result = cx
        .update(|cx| {
            let input = ReadFileToolInput {
                path: "project_root/../outside_project/sensitive_file.txt".to_string(),
                start_line: None,
                end_line: None,
            };
            tool.run(
                ToolInput::resolved(input),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;
    assert!(
        result.is_err(),
        "read_file_tool should error when attempting to read a relative path that resolves to outside a worktree"
    );
}
