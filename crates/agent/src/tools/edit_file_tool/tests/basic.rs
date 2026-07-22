use super::*;

async fn test_streaming_edit_granular_edits(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "line 1\nline 2\nline 3\n"})).await;
    let result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/file.txt".into(),
                    edits: vec![Edit {
                        old_text: "line 2".into(),
                        new_text: "modified line 2".into(),
                    }],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "line 1\nmodified line 2\nline 3\n");
}

#[gpui::test]
async fn test_streaming_edit_multiple_edits(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) = setup_test(
        cx,
        json!({"file.txt": "line 1\nline 2\nline 3\nline 4\nline 5\n"}),
    )
    .await;
    let result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/file.txt".into(),
                    edits: vec![
                        Edit {
                            old_text: "line 5".into(),
                            new_text: "modified line 5".into(),
                        },
                        Edit {
                            old_text: "line 1".into(),
                            new_text: "modified line 1".into(),
                        },
                    ],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(
        new_text,
        "modified line 1\nline 2\nline 3\nline 4\nmodified line 5\n"
    );
}

#[gpui::test]
async fn test_streaming_edit_adjacent_edits(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) = setup_test(
        cx,
        json!({"file.txt": "line 1\nline 2\nline 3\nline 4\nline 5\n"}),
    )
    .await;
    let result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/file.txt".into(),
                    edits: vec![
                        Edit {
                            old_text: "line 2".into(),
                            new_text: "modified line 2".into(),
                        },
                        Edit {
                            old_text: "line 3".into(),
                            new_text: "modified line 3".into(),
                        },
                    ],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(
        new_text,
        "line 1\nmodified line 2\nmodified line 3\nline 4\nline 5\n"
    );
}

#[gpui::test]
async fn test_streaming_edit_ascending_order_edits(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) = setup_test(
        cx,
        json!({"file.txt": "line 1\nline 2\nline 3\nline 4\nline 5\n"}),
    )
    .await;
    let result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/file.txt".into(),
                    edits: vec![
                        Edit {
                            old_text: "line 1".into(),
                            new_text: "modified line 1".into(),
                        },
                        Edit {
                            old_text: "line 5".into(),
                            new_text: "modified line 5".into(),
                        },
                    ],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(
        new_text,
        "modified line 1\nline 2\nline 3\nline 4\nmodified line 5\n"
    );
}

#[gpui::test]
async fn test_streaming_edit_nonexistent_file(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) = setup_test(cx, json!({})).await;
    let result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/nonexistent_file.txt".into(),
                    edits: vec![Edit {
                        old_text: "foo".into(),
                        new_text: "bar".into(),
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
    assert_eq!(error, "Can't edit file: path not found");
    assert!(diff.is_empty());
    assert_eq!(input_path, None);
}

#[gpui::test]
async fn test_streaming_edit_global_skill_file(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;
    let skill_dir = agent_skills::global_skills_dir().join("my-skill");
    fs.insert_tree(&skill_dir, json!({ "SKILL.md": "old content\n" }))
        .await;
    let (edit_tool, _project, _action_log, fs, _thread) =
        setup_test_with_fs(cx, fs, &[path!("/root").as_ref()]).await;

    let input_path = PathBuf::from("~")
        .join(".agents")
        .join("skills")
        .join("my-skill")
        .join("SKILL.md");
    let skill_file = agent_skills::global_skills_dir()
        .join("my-skill")
        .join("SKILL.md");

    let (event_stream, mut event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        edit_tool.clone().run(
            ToolInput::resolved(EditFileToolInput {
                path: input_path,
                edits: vec![Edit {
                    old_text: "old content".into(),
                    new_text: "new content".into(),
                }],
            }),
            event_stream,
            cx,
        )
    });

    event_rx.expect_update_fields().await;
    let auth = event_rx.expect_authorization().await;
    let title = auth.tool_call.fields.title.as_deref().unwrap_or("");
    assert!(
        title.contains("agent skills"),
        "Authorization title should mention agent skills, got: {title}",
    );
    auth.response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .expect("authorization response should send");

    let EditFileToolOutput::Success { new_text, .. } = task.await.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "new content\n");
    assert_eq!(fs.load(&skill_file).await.unwrap(), "new content\n");
}

#[gpui::test]
async fn test_streaming_edit_failed_match(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "hello world"})).await;
    let result = cx
        .update(|cx| {
            edit_tool.clone().run(
                ToolInput::resolved(EditFileToolInput {
                    path: "root/file.txt".into(),
                    edits: vec![Edit {
                        old_text: "nonexistent text that is not in the file".into(),
                        new_text: "replacement".into(),
                    }],
                }),
                ToolCallEventStream::test().0,
                cx,
            )
        })
        .await;

    let EditFileToolOutput::Error { error, .. } = result.unwrap_err() else {
        panic!("expected error");
    };
    assert!(
        error.contains("Could not find matching text"),
        "Expected error containing 'Could not find matching text' but got: {error}"
    );
}

/// When the edit fails after a session is created but before any edits are
/// actually applied (e.g., the first `old_text` doesn't match), the empty
/// diff placeholder in the UI should be replaced with the error message.
#[gpui::test]
async fn test_streaming_edit_surfaces_error_when_no_edits_applied(cx: &mut TestAppContext) {
    async fn find_first_text_content_in_events(
        receiver: &mut crate::ToolCallEventStreamReceiver,
    ) -> Option<String> {
        use futures::StreamExt as _;
        while let Some(event) = receiver.next().await {
            let Ok(crate::ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(
                update,
            ))) = event
            else {
                continue;
            };
            let Some(content) = update.fields.content else {
                continue;
            };
            for item in content {
                if let acp::ToolCallContent::Content(c) = item
                    && let acp::ContentBlock::Text(text) = c.content
                {
                    return Some(text.text);
                }
            }
        }
        None
    }

    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "hello world"})).await;
    let (event_stream, mut receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| {
        edit_tool.clone().run(
            ToolInput::resolved(EditFileToolInput {
                path: "root/file.txt".into(),
                edits: vec![Edit {
                    old_text: "nonexistent text that is not in the file".into(),
                    new_text: "replacement".into(),
                }],
            }),
            event_stream,
            cx,
        )
    });

    let EditFileToolOutput::Error { error, diff, .. } = task.await.unwrap_err() else {
        panic!("expected error");
    };
    assert!(
        diff.is_empty(),
        "sanity check: no edits should have been applied",
    );

    let content_text = find_first_text_content_in_events(&mut receiver).await;
    assert_eq!(
        content_text.as_deref(),
        Some(error.as_str()),
        "expected the failure message to be surfaced as tool call content",
    );
}
