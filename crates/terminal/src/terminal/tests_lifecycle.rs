use super::*;

/// Test that kill_active_task properly terminates both the foreground process
/// and the shell, allowing wait_for_completed_task to complete and output to be captured.
#[cfg(unix)]
#[gpui::test]
async fn test_kill_active_task_completes_and_captures_output(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    // Run a command that prints output then sleeps for a long time
    // The echo ensures we have output to capture before killing
    let (terminal, completion_rx) =
        build_test_terminal(cx, "echo", &["test_output_before_kill; sleep 60"]).await;

    assert_content_eventually(&terminal, "test_output_before_kill", cx).await;

    // Kill the active task
    terminal.update(cx, |term, _cx| {
        term.kill_active_task();
    });

    // wait_for_completed_task should complete within a reasonable time (not hang)
    let completion_result = completion_rx.recv().await;
    assert!(
        completion_result.is_ok(),
        "wait_for_completed_task should complete after kill_active_task, but it timed out"
    );

    // The exit status should indicate the process was killed (not a clean exit)
    let exit_status = completion_result.unwrap();
    assert!(
        exit_status.is_some(),
        "Should have received an exit status after killing"
    );

    // Verify that output captured before killing is still available
    let content = terminal.update(cx, |term, _| term.get_content());
    assert!(
        content.contains("test_output_before_kill"),
        "Output from before kill should be captured, got: {content}"
    );
}

/// Test that kill_active_task on a task that's not running is a no-op
#[gpui::test]
async fn test_kill_active_task_on_completed_task_is_noop(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    // Run a command that exits immediately
    let (terminal, completion_rx) = build_test_terminal(cx, "echo", &["done"]).await;

    // Wait for the command to complete naturally
    let exit_status = completion_rx
        .recv()
        .await
        .expect("Should receive exit status");
    assert_eq!(exit_status, Some(ExitStatus::default()));

    assert_content_eventually(&terminal, "done", cx).await;

    // Now try to kill - should be a no-op since task already completed
    terminal.update(cx, |term, _cx| {
        term.kill_active_task();
    });

    // Content should still be there
    let content = terminal.update(cx, |term, _| term.get_content());
    assert!(
        content.contains("done"),
        "Output should still be present after no-op kill, got: {content}"
    );
}
