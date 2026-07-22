use super::*;

#[gpui::test]
async fn test_streaming_needs_confirmation_with_multiple_worktrees(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        "/workspace/frontend",
        json!({
            "src": {
                "main.js": "console.log('frontend');"
            }
        }),
    )
    .await;
    fs.insert_tree(
        "/workspace/backend",
        json!({
            "src": {
                "main.rs": "fn main() {}"
            }
        }),
    )
    .await;
    fs.insert_tree(
        "/workspace/shared",
        json!({
            ".mav": {
                "settings.json": "{}"
            }
        }),
    )
    .await;
    let (edit_tool, _project, _action_log, _fs, _thread) = setup_test_with_fs(
        cx,
        fs,
        &[
            path!("/workspace/frontend").as_ref(),
            path!("/workspace/backend").as_ref(),
            path!("/workspace/shared").as_ref(),
        ],
    )
    .await;

    let test_cases = vec![
        ("frontend/src/main.js", false, "File in first worktree"),
        ("backend/src/main.rs", false, "File in second worktree"),
        (
            "shared/.mav/settings.json",
            true,
            ".mav file in third worktree",
        ),
        ("/etc/hosts", true, "Absolute path outside all worktrees"),
        (
            "../outside/file.txt",
            true,
            "Relative path outside worktrees",
        ),
    ];

    for (path, should_confirm, description) in test_cases {
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let auth = cx.update(|cx| edit_tool.authorize(&PathBuf::from(path), &stream_tx, cx));

        if should_confirm {
            stream_rx.expect_authorization().await;
        } else {
            auth.await.unwrap();
            assert!(
                stream_rx.try_recv().is_err(),
                "Failed for case: {} - path: {} - expected no confirmation but got one",
                description,
                path
            );
        }
    }
}

#[gpui::test]
async fn test_streaming_needs_confirmation_edge_cases(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".mav": {
                "settings.json": "{}"
            },
            "src": {
                ".mav": {
                    "local.json": "{}"
                }
            }
        }),
    )
    .await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/project").as_ref()]).await;

    let test_cases = vec![
        ("", false, "Empty path is treated as project root"),
        ("/", true, "Root directory should be outside project"),
        (
            "project/../other",
            true,
            "Path with .. that goes outside of root directory",
        ),
        (
            "project/./src/file.rs",
            false,
            "Path with . should work normally",
        ),
        #[cfg(target_os = "windows")]
        ("C:\\Windows\\System32\\hosts", true, "Windows system path"),
        #[cfg(target_os = "windows")]
        ("project\\src\\main.rs", false, "Windows-style project path"),
    ];

    for (path, should_confirm, description) in test_cases {
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let auth = cx.update(|cx| edit_tool.authorize(&PathBuf::from(path), &stream_tx, cx));

        cx.run_until_parked();

        if should_confirm {
            stream_rx.expect_authorization().await;
        } else {
            assert!(
                stream_rx.try_recv().is_err(),
                "Failed for case: {} - path: {} - expected no confirmation but got one",
                description,
                path
            );
            auth.await.unwrap();
        }
    }
}

#[gpui::test]
async fn test_streaming_needs_confirmation_with_different_modes(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            "existing.txt": "content",
            ".mav": {
                "settings.json": "{}"
            }
        }),
    )
    .await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/project").as_ref()]).await;

    let modes = vec![EditSessionMode::Edit, EditSessionMode::Write];

    for _mode in modes {
        // Test .mav path with different modes
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let _auth = cx.update(|cx| {
            edit_tool.authorize(&PathBuf::from("project/.mav/settings.json"), &stream_tx, cx)
        });

        stream_rx.expect_authorization().await;

        // Test outside path with different modes
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        let _auth = cx
            .update(|cx| edit_tool.authorize(&PathBuf::from("/outside/file.txt"), &stream_tx, cx));

        stream_rx.expect_authorization().await;

        // Test normal path with different modes
        let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
        cx.update(|cx| edit_tool.authorize(&PathBuf::from("project/normal.txt"), &stream_tx, cx))
            .await
            .unwrap();
        assert!(stream_rx.try_recv().is_err());
    }
}

#[gpui::test]
async fn test_streaming_initial_title_with_partial_input(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({})).await;
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/project").as_ref()]).await;

    cx.update(|cx| {
        assert_eq!(
            edit_tool.initial_title(
                Err(json!({
                    "path": "src/main.rs",
                })),
                cx
            ),
            "src/main.rs"
        );
        assert_eq!(
            edit_tool.initial_title(
                Err(json!({
                    "path": "",
                })),
                cx
            ),
            DEFAULT_UI_TEXT
        );
        assert_eq!(
            edit_tool.initial_title(Err(serde_json::Value::Null), cx),
            DEFAULT_UI_TEXT
        );
    });
}

#[gpui::test]
async fn test_streaming_consecutive_edits_work(cx: &mut TestAppContext) {
    let (edit_tool, project, action_log, _fs, _thread) =
        setup_test(cx, json!({"test.txt": "original content"})).await;
    let read_tool = Arc::new(crate::ReadFileTool::new(
        project.clone(),
        action_log.clone(),
        true,
    ));

    // Read the file first
    cx.update(|cx| {
        read_tool.clone().run(
            ToolInput::resolved(crate::ReadFileToolInput {
                path: "root/test.txt".to_string(),
                start_line: None,
                end_line: None,
            }),
            ToolCallEventStream::test().0,
            cx,
        )
    })
    .await
    .unwrap();

    // First edit should work
    let edit_result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/test.txt".into(),
                    edits: vec![Edit {
                        old_text: "original content".into(),
                        new_text: "modified content".into(),
                    }],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;
    assert!(
        edit_result.is_ok(),
        "First edit should succeed, got error: {:?}",
        edit_result.as_ref().err()
    );

    // Second edit should also work because the edit updated the recorded read time
    let edit_result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/test.txt".into(),
                    edits: vec![Edit {
                        old_text: "modified content".into(),
                        new_text: "further modified content".into(),
                    }],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;
    assert!(
        edit_result.is_ok(),
        "Second consecutive edit should succeed, got error: {:?}",
        edit_result.as_ref().err()
    );
}
