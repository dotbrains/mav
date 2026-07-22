use super::*;

#[gpui::test]
async fn test_streaming_incremental_three_edits(cx: &mut TestAppContext) {
    let (edit_tool, project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "aaa\nbbb\nccc\nddd\neee\n"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Setup: description + path + mode
    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    // Edit 1 in progress
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "aaa", "new_text": "AAA"}]
    }));
    cx.run_until_parked();

    // Edit 2 appears — edit 1 is now complete and should be applied
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "aaa", "new_text": "AAA"},
            {"old_text": "ccc"}
        ]
    }));
    cx.run_until_parked();
    sender.send_partial(json!({
        "path": "root/file.txt",
        "mode": "edit",
        "edits": [
            {"old_text": "aaa", "new_text": "AAA"},
            {"old_text": "ccc", "new_text": "CCC"}
        ]
    }));
    cx.run_until_parked();

    // Verify edit 1 fully applied. Edit 2's new_text is being
    // streamed: "CCC" is inserted but the old "ccc" isn't deleted
    // yet (StreamingDiff::finish runs when edit 3 marks edit 2 done).
    let buffer_text = project.update(cx, |project, cx| {
        let pp = project
            .find_project_path(&PathBuf::from("root/file.txt"), cx)
            .unwrap();
        project.get_open_buffer(&pp, cx).map(|b| b.read(cx).text())
    });
    assert_eq!(buffer_text.as_deref(), Some("AAA\nbbb\nCCCccc\nddd\neee\n"));

    // Edit 3 appears — edit 2 is now complete and should be applied
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "aaa", "new_text": "AAA"},
            {"old_text": "ccc", "new_text": "CCC"},
            {"old_text": "eee"}
        ]
    }));
    cx.run_until_parked();
    sender.send_partial(json!({
        "path": "root/file.txt",
        "mode": "edit",
        "edits": [
            {"old_text": "aaa", "new_text": "AAA"},
            {"old_text": "ccc", "new_text": "CCC"},
            {"old_text": "eee", "new_text": "EEE"}
        ]
    }));
    cx.run_until_parked();

    // Verify edits 1 and 2 fully applied. Edit 3's new_text is being
    // streamed: "EEE" is inserted but old "eee" isn't deleted yet.
    let buffer_text = project.update(cx, |project, cx| {
        let pp = project
            .find_project_path(&PathBuf::from("root/file.txt"), cx)
            .unwrap();
        project.get_open_buffer(&pp, cx).map(|b| b.read(cx).text())
    });
    assert_eq!(buffer_text.as_deref(), Some("AAA\nbbb\nCCC\nddd\nEEEeee\n"));

    // Send final
    sender.send_full(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "aaa", "new_text": "AAA"},
            {"old_text": "ccc", "new_text": "CCC"},
            {"old_text": "eee", "new_text": "EEE"}
        ]
    }));

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "AAA\nbbb\nCCC\nddd\nEEE\n");
}

#[gpui::test]
async fn test_streaming_edit_failure_mid_stream(cx: &mut TestAppContext) {
    let (edit_tool, project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "line 1\nline 2\nline 3\n"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Setup
    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    // Edit 1 (valid) in progress — not yet complete (no second edit)
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "line 1", "new_text": "MODIFIED"}
        ]
    }));
    cx.run_until_parked();

    // Edit 2 appears (will fail to match) — this makes edit 1 complete.
    // Edit 1 should be applied. Edit 2 is still in-progress (last edit).
    sender.send_partial(json!({
            "path": "root/file.txt",
            "edits": [
                {"old_text": "line 1", "new_text": "MODIFIED"},
                {"old_text": "nonexistent text that does not appear anywhere in the file at all", "new_text": "whatever"}
            ]
        }));
    cx.run_until_parked();

    let buffer = project.update(cx, |project, cx| {
        let pp = project
            .find_project_path(&PathBuf::from("root/file.txt"), cx)
            .unwrap();
        project.get_open_buffer(&pp, cx).unwrap()
    });

    // Verify edit 1 was applied
    let buffer_text = buffer.read_with(cx, |buffer, _cx| buffer.text());
    assert_eq!(
        buffer_text, "MODIFIED\nline 2\nline 3\n",
        "First edit should be applied even though second edit will fail"
    );

    // Edit 3 appears — this makes edit 2 "complete", triggering its
    // resolution which should fail (old_text doesn't exist in the file).
    sender.send_partial(json!({
            "path": "root/file.txt",
            "edits": [
                {"old_text": "line 1", "new_text": "MODIFIED"},
                {"old_text": "nonexistent text that does not appear anywhere in the file at all", "new_text": "whatever"},
                {"old_text": "line 3", "new_text": "MODIFIED 3"}
            ]
        }));
    cx.run_until_parked();

    // The error from edit 2 should have propagated out of the partial loop.
    // Drop sender to unblock recv() if the loop didn't catch it.
    drop(sender);

    let result = task.await;
    let EditFileToolOutput::Error {
        error,
        diff,
        input_path,
    } = result.unwrap_err()
    else {
        panic!("expected error");
    };

    assert!(
        error.contains("Could not find matching text for edit at index 1"),
        "Expected error about edit 1 failing, got: {error}"
    );
    // Ensure that first edit was applied successfully and that we saved the buffer
    assert_eq!(input_path, Some(PathBuf::from("root/file.txt")));
    assert_eq!(
        diff,
        "@@ -1,3 +1,3 @@\n-line 1\n+MODIFIED\n line 2\n line 3\n"
    );
}

#[gpui::test]
async fn test_streaming_single_edit_no_incremental(cx: &mut TestAppContext) {
    let (edit_tool, project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "hello world\n"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Setup + single edit that stays in-progress (no second edit to prove completion)
    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "hello world"}]
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "hello world", "new_text": "goodbye world"}]
    }));
    cx.run_until_parked();

    // The edit's old_text and new_text both arrived in one partial, so
    // the old_text is resolved and new_text is being streamed via
    // StreamingDiff. The buffer reflects the in-progress diff (new text
    // inserted, old text not yet fully removed until finalization).
    let buffer_text = project.update(cx, |project, cx| {
        let pp = project
            .find_project_path(&PathBuf::from("root/file.txt"), cx)
            .unwrap();
        project.get_open_buffer(&pp, cx).map(|b| b.read(cx).text())
    });
    assert_eq!(
        buffer_text.as_deref(),
        Some("goodbye worldhello world\n"),
        "In-progress streaming diff: new text inserted, old text not yet removed"
    );

    // Send final — the edit is applied during finalization
    sender.send_full(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "hello world", "new_text": "goodbye world"}]
    }));

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "goodbye world\n");
}

#[gpui::test]
async fn test_streaming_input_partials_then_final(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "line 1\nline 2\nline 3\n"})).await;
    let (mut sender, input): (ToolInputSender, ToolInput<EditFileToolInput>) = ToolInput::test();
    let (event_stream, _event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Send progressively more complete partial snapshots, as the LLM would
    sender.send_partial(json!({}));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "line 2", "new_text": "modified line 2"}]
    }));
    cx.run_until_parked();

    // Send the final complete input
    sender.send_full(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "line 2", "new_text": "modified line 2"}]
    }));

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(new_text, "line 1\nmodified line 2\nline 3\n");
}

#[gpui::test]
async fn test_streaming_input_sender_dropped_before_final(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "hello world\n"})).await;
    let (mut sender, input): (ToolInputSender, ToolInput<EditFileToolInput>) = ToolInput::test();
    let (event_stream, _event_rx) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Send a partial then drop the sender without sending final
    sender.send_partial(json!({}));
    cx.run_until_parked();

    drop(sender);

    let result = task.await;
    assert!(
        result.is_err(),
        "Tool should error when sender is dropped without sending final input"
    );
}

#[gpui::test]
async fn test_streaming_resolve_path_for_editing_file(cx: &mut TestAppContext) {
    let mode = EditSessionMode::Edit;

    let path_with_root = "root/dir/subdir/existing.txt";
    let path_without_root = "dir/subdir/existing.txt";
    let result = test_resolve_path(&mode, path_with_root, cx);
    assert_resolved_path_eq(result.await, rel_path(path_without_root));

    let result = test_resolve_path(&mode, path_without_root, cx);
    assert_resolved_path_eq(result.await, rel_path(path_without_root));

    let result = test_resolve_path(&mode, "root/nonexistent.txt", cx);
    assert_eq!(result.await.unwrap_err(), "Can't edit file: path not found");

    let result = test_resolve_path(&mode, "root/dir", cx);
    assert_eq!(
        result.await.unwrap_err(),
        "Can't edit file: path is a directory"
    );
}

async fn test_resolve_path(
    mode: &EditSessionMode,
    path: &str,
    cx: &mut TestAppContext,
) -> Result<ProjectPath, String> {
    init_test(cx);

    let fs = project::FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir": {
                "subdir": {
                    "existing.txt": "hello"
                }
            }
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;

    crate::tools::edit_session::test_resolve_path(mode, path, &project, cx).await
}

#[track_caller]
fn assert_resolved_path_eq(path: Result<ProjectPath, String>, expected: &RelPath) {
    let actual = path.expect("Should return valid path").path;
    assert_eq!(actual.as_ref(), expected);
}
