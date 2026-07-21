use super::*;

#[gpui::test]
async fn test_terminal_output_buffered_before_created_renders(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(
                project,
                PathList::new(&[std::path::Path::new(path!("/test"))]),
                cx,
            )
        })
        .await
        .unwrap();

    let terminal_id = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());

    // Send Output BEFORE Created - should be buffered by acp_thread
    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Output {
                terminal_id: terminal_id.clone(),
                data: b"hello buffered".to_vec(),
            },
            cx,
        );
    });

    // Create a display-only terminal and then send Created
    let lower = cx.new(|cx| {
        let builder = ::terminal::TerminalBuilder::new_display_only(
            ::terminal::terminal_settings::CursorShape::default(),
            ::terminal::terminal_settings::AlternateScroll::On,
            None,
            0,
            cx.background_executor(),
            PathStyle::local(),
        );
        builder.subscribe(cx)
    });

    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Created {
                terminal_id: terminal_id.clone(),
                label: "Buffered Test".to_string(),
                cwd: None,
                output_byte_limit: None,
                terminal: lower.clone(),
            },
            cx,
        );
    });

    // After Created, buffered Output should have been flushed into the renderer
    let content = thread.read_with(cx, |thread, cx| {
        let term = thread.terminal(terminal_id.clone()).unwrap();
        term.read_with(cx, |t, cx| t.inner().read(cx).get_content())
    });

    assert!(
        content.contains("hello buffered"),
        "expected buffered output to render, got: {content}"
    );
}

#[gpui::test]
async fn test_terminal_output_and_exit_buffered_before_created(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(
                project,
                PathList::new(&[std::path::Path::new(path!("/test"))]),
                cx,
            )
        })
        .await
        .unwrap();

    let terminal_id = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());

    // Send Output BEFORE Created
    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Output {
                terminal_id: terminal_id.clone(),
                data: b"pre-exit data".to_vec(),
            },
            cx,
        );
    });

    // Send Exit BEFORE Created
    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Exit {
                terminal_id: terminal_id.clone(),
                status: acp::TerminalExitStatus::new().exit_code(0),
            },
            cx,
        );
    });

    // Now create a display-only lower-level terminal and send Created
    let lower = cx.new(|cx| {
        let builder = ::terminal::TerminalBuilder::new_display_only(
            ::terminal::terminal_settings::CursorShape::default(),
            ::terminal::terminal_settings::AlternateScroll::On,
            None,
            0,
            cx.background_executor(),
            PathStyle::local(),
        );
        builder.subscribe(cx)
    });

    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Created {
                terminal_id: terminal_id.clone(),
                label: "Buffered Exit Test".to_string(),
                cwd: None,
                output_byte_limit: None,
                terminal: lower.clone(),
            },
            cx,
        );
    });

    // Output should be present after Created (flushed from buffer)
    let content = thread.read_with(cx, |thread, cx| {
        let term = thread.terminal(terminal_id.clone()).unwrap();
        term.read_with(cx, |t, cx| t.inner().read(cx).get_content())
    });

    assert!(
        content.contains("pre-exit data"),
        "expected pre-exit data to render, got: {content}"
    );
}

/// Test that killing a terminal via Terminal::kill properly:
/// 1. Causes wait_for_exit to complete (doesn't hang forever)
/// 2. The underlying terminal still has the output that was written before the kill
///
/// This test verifies that the fix to kill_active_task (which now also kills
/// the shell process in addition to the foreground process) properly allows
/// wait_for_exit to complete instead of hanging indefinitely.
#[cfg(unix)]
#[gpui::test]
async fn test_terminal_kill_allows_wait_for_exit_to_complete(cx: &mut gpui::TestAppContext) {
    use std::collections::HashMap;
    use task::Shell;
    use util::shell_builder::ShellBuilder;

    init_test(cx);
    cx.executor().allow_parking();

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(
                project.clone(),
                PathList::new(&[Path::new(path!("/test"))]),
                cx,
            )
        })
        .await
        .unwrap();

    let terminal_id = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());

    // Create a real PTY terminal that runs a command which prints output then sleeps
    // We use printf instead of echo and chain with && sleep to ensure proper execution
    let (completion_tx, _completion_rx) = async_channel::unbounded();
    let (program, args) = ShellBuilder::new(&Shell::System, false).build(
        Some("printf 'output_before_kill\\n' && sleep 60".to_owned()),
        &[],
    );

    let builder = cx
        .update(|cx| {
            ::terminal::TerminalBuilder::new(
                None,
                None,
                task::Shell::WithArguments {
                    program,
                    args,
                    title_override: None,
                },
                HashMap::default(),
                ::terminal::terminal_settings::CursorShape::default(),
                ::terminal::terminal_settings::AlternateScroll::On,
                None,
                vec![],
                0,
                false,
                0,
                Some(completion_tx),
                cx,
                vec![],
                PathStyle::local(),
            )
        })
        .await
        .unwrap();

    let lower_terminal = cx.new(|cx| builder.subscribe(cx));

    // Create the acp_thread Terminal wrapper
    thread.update(cx, |thread, cx| {
        thread.on_terminal_provider_event(
            TerminalProviderEvent::Created {
                terminal_id: terminal_id.clone(),
                label: "printf output_before_kill && sleep 60".to_string(),
                cwd: None,
                output_byte_limit: None,
                terminal: lower_terminal.clone(),
            },
            cx,
        );
    });

    // Poll until the printf command produces output, rather than using a
    // fixed sleep which is flaky on loaded machines.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let has_output = thread.read_with(cx, |thread, cx| {
            let term = thread
                .terminals
                .get(&terminal_id)
                .expect("terminal not found");
            let content = term.read(cx).inner().read(cx).get_content();
            content.contains("output_before_kill")
        });
        if has_output {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Timed out waiting for printf output to appear in terminal",
        );
        cx.executor().timer(Duration::from_millis(50)).await;
    }

    // Get the acp_thread Terminal and kill it
    let wait_for_exit = thread.update(cx, |thread, cx| {
        let term = thread.terminals.get(&terminal_id).unwrap();
        let wait_for_exit = term.read(cx).wait_for_exit();
        term.update(cx, |term, cx| {
            term.kill(cx);
        });
        wait_for_exit
    });

    // KEY ASSERTION: wait_for_exit should complete within a reasonable time (not hang).
    // Before the fix to kill_active_task, this would hang forever because
    // only the foreground process was killed, not the shell, so the PTY
    // child never exited and wait_for_completed_task never completed.
    let exit_result = futures::select! {
        result = futures::FutureExt::fuse(wait_for_exit) => Some(result),
        _ = futures::FutureExt::fuse(cx.background_executor.timer(Duration::from_secs(5))) => None,
    };

    assert!(
        exit_result.is_some(),
        "wait_for_exit should complete after kill, but it timed out. \
        This indicates kill_active_task is not properly killing the shell process."
    );

    // Give the system a chance to process any pending updates
    cx.run_until_parked();

    // Verify that the underlying terminal still has the output that was
    // written before the kill. This verifies that killing doesn't lose output.
    let inner_content = thread.read_with(cx, |thread, cx| {
        let term = thread.terminals.get(&terminal_id).unwrap();
        term.read(cx).inner().read(cx).get_content()
    });

    assert!(
        inner_content.contains("output_before_kill"),
        "Underlying terminal should contain output from before kill, got: {}",
        inner_content
    );
}
