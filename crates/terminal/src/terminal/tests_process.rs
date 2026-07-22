use super::*;

#[gpui::test]
async fn test_basic_terminal(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (terminal, completion_rx) = build_test_terminal(cx, "echo", &["hello"]).await;
    assert_eq!(
        completion_rx.recv().await.unwrap(),
        Some(ExitStatus::default())
    );
    assert_content_eventually(&terminal, "hello", cx).await;

    // Inject additional output directly into the emulator (display-only path)
    terminal.update(cx, |term, cx| {
        term.write_output(b"\nfrom_injection", cx);
    });

    let content_after = terminal.update(cx, |term, _| term.get_content());
    assert!(
        content_after.contains("from_injection"),
        "expected injected output to appear, got: {content_after}"
    );
}

#[cfg(unix)]
#[gpui::test]
async fn test_foreground_process_command_tracks_path_command(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (terminal, completion_rx) =
        build_test_terminal_with_arguments(cx, "sleep".to_string(), vec!["1".to_string()]).await;

    assert_foreground_process_command_eventually(&terminal, "sleep", cx).await;

    assert!(
        completion_rx.recv().await.is_ok(),
        "expected terminal completion after sleep exits"
    );
}

// TODO should be tested on Linux too, but does not work there well
#[cfg(target_os = "macos")]
#[gpui::test(iterations = 10)]
async fn test_terminal_eof(cx: &mut TestAppContext) {
    init_test(cx);

    cx.executor().allow_parking();

    let (completion_tx, completion_rx) = async_channel::unbounded();
    let builder = cx
        .update(|cx| {
            TerminalBuilder::new(
                None,
                None,
                task::Shell::System,
                HashMap::default(),
                SettingsCursorShape::default(),
                AlternateScroll::On,
                None,
                vec![],
                0,
                false,
                0,
                Some(completion_tx),
                cx,
                Vec::new(),
                PathStyle::local(),
            )
        })
        .await
        .unwrap();
    // Build an empty command, which will result in a tty shell spawned.
    let terminal = cx.new(|cx| builder.subscribe(cx));

    let (event_tx, event_rx) = async_channel::unbounded::<Event>();
    cx.update(|cx| {
        cx.subscribe(&terminal, move |_, e, _| {
            event_tx.send_blocking(e.clone()).unwrap();
        })
    })
    .detach();
    cx.background_spawn(async move {
        assert_eq!(
            completion_rx.recv().await.unwrap(),
            Some(ExitStatus::default()),
            "EOF should result in the tty shell exiting successfully",
        );
    })
    .detach();

    let first_event = event_rx.recv().await.expect("No wakeup event received");

    terminal.update(cx, |terminal, _| {
        let success = terminal.try_keystroke(&Keystroke::parse("ctrl-d").unwrap(), false);
        assert!(success, "Should have registered ctrl-d sequence");
    });

    let mut all_events = vec![first_event];
    while let Ok(new_event) = event_rx.recv().await {
        all_events.push(new_event.clone());
        if new_event == Event::CloseTerminal {
            break;
        }
    }
    assert!(
        all_events.contains(&Event::CloseTerminal),
        "EOF command sequence should have triggered a TTY terminal exit, but got events: {all_events:?}",
    );
}

#[cfg(not(target_os = "windows"))]
#[gpui::test(iterations = 10)]
async fn test_terminal_closes_after_nonzero_exit(cx: &mut TestAppContext) {
    init_test(cx);

    cx.executor().allow_parking();

    let builder = cx
        .update(|cx| {
            TerminalBuilder::new(
                None,
                None,
                task::Shell::System,
                HashMap::default(),
                SettingsCursorShape::default(),
                AlternateScroll::On,
                None,
                vec![],
                0,
                false,
                0,
                None,
                cx,
                Vec::new(),
                PathStyle::local(),
            )
        })
        .await
        .unwrap();
    let terminal = cx.new(|cx| builder.subscribe(cx));

    let (event_tx, event_rx) = async_channel::unbounded::<Event>();
    cx.update(|cx| {
        cx.subscribe(&terminal, move |_, e, _| {
            event_tx.send_blocking(e.clone()).unwrap();
        })
    })
    .detach();

    let first_event = event_rx.recv().await.expect("No wakeup event received");

    terminal.update(cx, |terminal, _| {
        terminal.input(b"false\r".to_vec());
    });
    cx.executor().timer(Duration::from_millis(500)).await;
    terminal.update(cx, |terminal, _| {
        terminal.input(b"exit\r".to_vec());
    });

    let mut all_events = vec![first_event];
    while let Ok(new_event) = event_rx.recv().await {
        all_events.push(new_event.clone());
        if new_event == Event::CloseTerminal {
            break;
        }
    }
    assert!(
        all_events.contains(&Event::CloseTerminal),
        "Shell exiting after `false && exit` should close terminal, but got events: {all_events:?}",
    );
}

#[gpui::test(iterations = 10)]
async fn test_terminal_no_exit_on_spawn_failure(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (completion_tx, completion_rx) = async_channel::unbounded();
    let (program, args) = ShellBuilder::new(&Shell::System, false)
        .build(Some("asdasdasdasd".to_owned()), &["@@@@@".to_owned()]);
    let builder = cx
        .update(|cx| {
            TerminalBuilder::new(
                None,
                None,
                task::Shell::WithArguments {
                    program,
                    args,
                    title_override: None,
                },
                HashMap::default(),
                SettingsCursorShape::default(),
                AlternateScroll::On,
                None,
                Vec::new(),
                0,
                false,
                0,
                Some(completion_tx),
                cx,
                Vec::new(),
                PathStyle::local(),
            )
        })
        .await
        .unwrap();
    let terminal = cx.new(|cx| builder.subscribe(cx));

    let all_events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    cx.update({
        let all_events = all_events.clone();
        |cx| {
            cx.subscribe(&terminal, move |_, e, _| {
                all_events.lock().push(e.clone());
            })
        }
    })
    .detach();
    let completion_check_task = cx.background_spawn(async move {
        // The channel may be closed if the terminal is dropped before sending
        // the completion signal, which can happen with certain task scheduling orders.
        let exit_status = completion_rx.recv().await.ok().flatten();
        if let Some(exit_status) = exit_status {
            assert!(
                !exit_status.success(),
                "Wrong shell command should result in a failure"
            );
            #[cfg(target_os = "windows")]
            assert_eq!(exit_status.code(), Some(1));
            #[cfg(not(target_os = "windows"))]
            assert_eq!(exit_status.code(), Some(127)); // code 127 means "command not found" on Unix
        }
    });

    completion_check_task.await;
    cx.executor().timer(Duration::from_millis(500)).await;

    assert!(
        !all_events
            .lock()
            .iter()
            .any(|event| event == &Event::CloseTerminal),
        "Wrong shell command should update the title but not should not close the terminal to show the error message, but got events: {all_events:?}",
    );
}
