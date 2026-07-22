use super::*;

#[gpui::test]
async fn test_streaming_early_buffer_open(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "line 1\nline 2\nline 3\n"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Send partials simulating LLM streaming: description first, then path, then mode
    sender.send_partial(json!({}));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt"
    }));
    cx.run_until_parked();

    // Path is NOT yet complete because mode hasn't appeared — no buffer open yet
    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    // Now send the final complete input
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
async fn test_streaming_cancellation_during_partials(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "hello world"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver, mut cancellation_tx) =
        ToolCallEventStream::test_with_cancellation();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Send a partial
    sender.send_partial(json!({}));
    cx.run_until_parked();

    // Cancel during streaming
    ToolCallEventStream::signal_cancellation_with_sender(&mut cancellation_tx);
    cx.run_until_parked();

    // The sender is still alive so the partial loop should detect cancellation
    // We need to drop the sender to also unblock recv() if the loop didn't catch it
    drop(sender);

    let result = task.await;
    let EditFileToolOutput::Error { error, .. } = result.unwrap_err() else {
        panic!("expected error");
    };
    assert!(
        error.contains("cancelled"),
        "Expected cancellation error but got: {error}"
    );
}

#[gpui::test]
async fn test_streaming_edit_with_multiple_partials(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) = setup_test(
        cx,
        json!({"file.txt": "line 1\nline 2\nline 3\nline 4\nline 5\n"}),
    )
    .await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Simulate fine-grained streaming of the JSON
    sender.send_partial(json!({}));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt"
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "line 1"}]
    }));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "line 1", "new_text": "modified line 1"},
            {"old_text": "line 5"}
        ]
    }));
    cx.run_until_parked();

    // Send final complete input
    sender.send_full(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "line 1", "new_text": "modified line 1"},
            {"old_text": "line 5", "new_text": "modified line 5"}
        ]
    }));

    let result = task.await;
    let EditFileToolOutput::Success { new_text, .. } = result.unwrap() else {
        panic!("expected success");
    };
    assert_eq!(
        new_text,
        "modified line 1\nline 2\nline 3\nline 4\nmodified line 5\n"
    );
}

#[gpui::test]
async fn test_streaming_no_partials_direct_final(cx: &mut TestAppContext) {
    let (edit_tool, _project, _action_log, _fs, _thread) =
        setup_test(cx, json!({"file.txt": "line 1\nline 2\nline 3\n"})).await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Send final immediately with no partials (simulates non-streaming path)
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
async fn test_streaming_incremental_edit_application(cx: &mut TestAppContext) {
    let (edit_tool, project, _action_log, _fs, _thread) = setup_test(
        cx,
        json!({"file.txt": "line 1\nline 2\nline 3\nline 4\nline 5\n"}),
    )
    .await;
    let (mut sender, input) = ToolInput::<EditFileToolInput>::test();
    let (event_stream, _receiver) = ToolCallEventStream::test();
    let task = cx.update(|cx| edit_tool.clone().run(input, event_stream, cx));

    // Stream description, path, mode
    sender.send_partial(json!({}));
    cx.run_until_parked();

    sender.send_partial(json!({
        "path": "root/file.txt",
    }));
    cx.run_until_parked();

    // First edit starts streaming (old_text only, still in progress)
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [{"old_text": "line 1"}]
    }));
    cx.run_until_parked();

    // Buffer should not have changed yet — the first edit is still in progress
    // (no second edit has appeared to prove the first is complete)
    let buffer_text = project.update(cx, |project, cx| {
        let project_path = project.find_project_path(&PathBuf::from("root/file.txt"), cx);
        project_path.and_then(|pp| {
            project
                .get_open_buffer(&pp, cx)
                .map(|buffer| buffer.read(cx).text())
        })
    });
    // Buffer is open (from streaming) but edit 1 is still in-progress
    assert_eq!(
        buffer_text.as_deref(),
        Some("line 1\nline 2\nline 3\nline 4\nline 5\n"),
        "Buffer should not be modified while first edit is still in progress"
    );

    // Second edit appears — this proves the first edit is complete, so it
    // should be applied immediately during streaming
    sender.send_partial(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "line 1", "new_text": "MODIFIED 1"},
            {"old_text": "line 5"}
        ]
    }));
    cx.run_until_parked();

    // First edit should now be applied to the buffer
    let buffer_text = project.update(cx, |project, cx| {
        let project_path = project.find_project_path(&PathBuf::from("root/file.txt"), cx);
        project_path.and_then(|pp| {
            project
                .get_open_buffer(&pp, cx)
                .map(|buffer| buffer.read(cx).text())
        })
    });
    assert_eq!(
        buffer_text.as_deref(),
        Some("MODIFIED 1\nline 2\nline 3\nline 4\nline 5\n"),
        "First edit should be applied during streaming when second edit appears"
    );

    // Send final complete input
    sender.send_full(json!({
        "path": "root/file.txt",
        "edits": [
            {"old_text": "line 1", "new_text": "MODIFIED 1"},
            {"old_text": "line 5", "new_text": "MODIFIED 5"}
        ]
    }));

    let result = task.await;
    let EditFileToolOutput::Success {
        new_text, old_text, ..
    } = result.unwrap()
    else {
        panic!("expected success");
    };
    assert_eq!(new_text, "MODIFIED 1\nline 2\nline 3\nline 4\nMODIFIED 5\n");
    assert_eq!(
        *old_text, "line 1\nline 2\nline 3\nline 4\nline 5\n",
        "old_text should reflect the original file content before any edits"
    );
}
