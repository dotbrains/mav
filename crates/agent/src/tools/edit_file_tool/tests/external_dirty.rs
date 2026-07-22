use super::*;

#[gpui::test]
async fn test_streaming_external_modification_matching_edit_succeeds(cx: &mut TestAppContext) {
    let (edit_tool, project, action_log, fs, _thread) =
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

    // Simulate external modification
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(2));
    fs.save(
        path!("/root/test.txt").as_ref(),
        &"externally modified content".into(),
        language::LineEnding::Unix,
    )
    .await
    .unwrap();

    // Reload the buffer to pick up the new mtime
    let project_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("root/test.txt", cx)
        })
        .expect("Should find project path");
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(project_path, cx))
        .await
        .unwrap();
    buffer
        .update(cx, |buffer, cx| buffer.reload(cx))
        .await
        .unwrap();

    cx.executor().run_until_parked();

    let result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/test.txt".into(),
                    edits: vec![Edit {
                        old_text: "externally modified content".into(),
                        new_text: "new content".into(),
                    }],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await
        .unwrap();

    let EditFileToolOutput::Success {
        new_text,
        input_path,
        ..
    } = result
    else {
        panic!("expected success");
    };

    assert_eq!(new_text, "new content");
    assert_eq!(input_path, PathBuf::from("root/test.txt"));
}

#[gpui::test]
async fn test_streaming_external_modification_mentioned_when_match_fails(cx: &mut TestAppContext) {
    let (edit_tool, project, action_log, fs, _thread) =
        setup_test(cx, json!({"test.txt": "original content"})).await;
    let read_tool = Arc::new(crate::ReadFileTool::new(
        project.clone(),
        action_log.clone(),
        true,
    ));

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

    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(2));
    fs.save(
        path!("/root/test.txt").as_ref(),
        &"externally modified content".into(),
        language::LineEnding::Unix,
    )
    .await
    .unwrap();

    let project_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("root/test.txt", cx)
        })
        .expect("Should find project path");
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(project_path, cx))
        .await
        .unwrap();
    buffer
        .update(cx, |buffer, cx| buffer.reload(cx))
        .await
        .unwrap();

    cx.executor().run_until_parked();

    let result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/test.txt".into(),
                    edits: vec![Edit {
                        old_text: "original content".into(),
                        new_text: "new content".into(),
                    }],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let EditFileToolOutput::Error {
        error,
        diff,
        input_path,
    } = result.unwrap_err()
    else {
        panic!("expected error");
    };

    assert!(
        error.contains("Could not find matching text for edit at index 0"),
        "Error should mention failed match, got: {error}"
    );
    assert!(
        error.contains("has changed on disk since you last read it"),
        "Error should mention possible disk change, got: {error}"
    );
    assert!(diff.is_empty());
    assert_eq!(input_path, Some(PathBuf::from("root/test.txt")));
}

/// When the buffer has unsaved changes and the user picks "Save", the
/// pending edits are flushed to disk and the agent's edit then proceeds
/// against the just-saved content.
#[gpui::test]
async fn test_streaming_dirty_buffer_save(cx: &mut TestAppContext) {
    let (edit_tool, project, action_log, fs, _thread) =
        setup_test(cx, json!({"test.txt": "original content"})).await;
    let read_tool = Arc::new(crate::ReadFileTool::new(
        project.clone(),
        action_log.clone(),
        true,
    ));

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

    let project_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("root/test.txt", cx)
        })
        .expect("Should find project path");
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(project_path, cx))
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        let end_point = buffer.max_point();
        buffer.edit([(end_point..end_point, " plus user edit")], None, cx);
    });
    assert!(buffer.read_with(cx, |buffer, _| buffer.is_dirty()));

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        edit_tool.clone().run(
            ToolInput::resolved(EditFileToolInput {
                path: "root/test.txt".into(),
                edits: vec![Edit {
                    old_text: "original content plus user edit".into(),
                    new_text: "replaced content".into(),
                }],
            }),
            stream_tx,
            cx,
        )
    });

    let _update = stream_rx.expect_update_fields().await;
    let auth = stream_rx.expect_authorization().await;
    let content = auth.tool_call.fields.content.as_deref().unwrap_or(&[]);
    let acp::ToolCallContent::Content(text) = content.first().expect("expected message body")
    else {
        panic!("expected text body, got: {:?}", content.first());
    };
    let acp::ContentBlock::Text(text) = &text.content else {
        panic!("expected text body, got: {:?}", text.content);
    };
    assert!(
        text.text.contains("unsaved changes")
            && text.text.contains("save")
            && text.text.contains("discard"),
        "unexpected message body: {:?}",
        text.text,
    );
    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("save"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();

    let EditFileToolOutput::Success { new_text, .. } = task.await.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "replaced content");
    assert!(!buffer.read_with(cx, |buffer, _| buffer.is_dirty()));
    let on_disk = fs.load(path!("/root/test.txt").as_ref()).await.unwrap();
    assert_eq!(on_disk, "replaced content");
}

/// When the buffer has unsaved changes and the user picks "Discard", the
/// pending edits are reverted to match disk and the agent's edit then
/// proceeds against the on-disk content.
#[gpui::test]
async fn test_streaming_dirty_buffer_discard(cx: &mut TestAppContext) {
    let (edit_tool, project, action_log, fs, _thread) =
        setup_test(cx, json!({"test.txt": "original content"})).await;
    let read_tool = Arc::new(crate::ReadFileTool::new(
        project.clone(),
        action_log.clone(),
        true,
    ));

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

    let project_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("root/test.txt", cx)
        })
        .expect("Should find project path");
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(project_path, cx))
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        let end_point = buffer.max_point();
        buffer.edit([(end_point..end_point, " plus user edit")], None, cx);
    });
    assert!(buffer.read_with(cx, |buffer, _| buffer.is_dirty()));

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        edit_tool.clone().run(
            ToolInput::resolved(EditFileToolInput {
                path: "root/test.txt".into(),
                // Match the on-disk content, not the dirty in-memory content.
                edits: vec![Edit {
                    old_text: "original content".into(),
                    new_text: "replaced content".into(),
                }],
            }),
            stream_tx,
            cx,
        )
    });

    let _update = stream_rx.expect_update_fields().await;
    let auth = stream_rx.expect_authorization().await;
    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("discard"),
            acp::PermissionOptionKind::RejectOnce,
        ))
        .unwrap();

    let EditFileToolOutput::Success { new_text, .. } = task.await.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "replaced content");
    assert!(!buffer.read_with(cx, |buffer, _| buffer.is_dirty()));
    let on_disk = fs.load(path!("/root/test.txt").as_ref()).await.unwrap();
    assert_eq!(on_disk, "replaced content");
}

/// When the buffer is dirty and the user resolves it manually — e.g.
/// pressing `cmd-s` while the prompt is visible — the prompt is
/// dismissed automatically and the edit proceeds against the saved
/// content. The user shouldn't have to also click a button.
#[gpui::test]
async fn test_streaming_dirty_buffer_resolved_externally(cx: &mut TestAppContext) {
    let (edit_tool, project, action_log, fs, _thread) =
        setup_test(cx, json!({"test.txt": "original content"})).await;
    let read_tool = Arc::new(crate::ReadFileTool::new(
        project.clone(),
        action_log.clone(),
        true,
    ));

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

    let project_path = project
        .read_with(cx, |project, cx| {
            project.find_project_path("root/test.txt", cx)
        })
        .expect("Should find project path");
    let buffer = project
        .update(cx, |project, cx| project.open_buffer(project_path, cx))
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        let end_point = buffer.max_point();
        buffer.edit([(end_point..end_point, " plus user edit")], None, cx);
    });
    assert!(buffer.read_with(cx, |buffer, _| buffer.is_dirty()));

    let (stream_tx, mut stream_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        edit_tool.clone().run(
            ToolInput::resolved(EditFileToolInput {
                path: "root/test.txt".into(),
                edits: vec![Edit {
                    old_text: "original content plus user edit".into(),
                    new_text: "replaced content".into(),
                }],
            }),
            stream_tx,
            cx,
        )
    });

    let _update = stream_rx.expect_update_fields().await;
    let auth = stream_rx.expect_authorization().await;

    // Simulate the user saving the buffer manually (e.g. cmd-s) while
    // the prompt is visible. The tool should detect the buffer became
    // clean and proceed without the user clicking anything.
    project
        .update(cx, |project, cx| project.save_buffer(buffer.clone(), cx))
        .await
        .unwrap();

    // The prompt's response channel should drop without a click; the
    // tool dismisses the prompt by resolving the pending authorization.
    let (_, outcome) = stream_rx.expect_authorization_resolved().await;
    assert_eq!(outcome.option_id, acp::PermissionOptionId::new("save"));
    assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
    drop(auth);

    let EditFileToolOutput::Success { new_text, .. } = task.await.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "replaced content");
    assert!(!buffer.read_with(cx, |buffer, _| buffer.is_dirty()));
    let on_disk = fs.load(path!("/root/test.txt").as_ref()).await.unwrap();
    assert_eq!(on_disk, "replaced content");
}
