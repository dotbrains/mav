use super::*;

#[test]
fn test_init_command_startup_marker_commands_do_not_contain_marker() {
    let marker_id = 42;
    let marker = init_command_startup_marker(marker_id);

    for shell_kind in [
        ShellKind::Posix,
        ShellKind::Csh,
        ShellKind::Tcsh,
        ShellKind::Rc,
        ShellKind::Fish,
        ShellKind::PowerShell,
        ShellKind::Pwsh,
        ShellKind::Nushell,
        ShellKind::Cmd,
        ShellKind::Xonsh,
        ShellKind::Elvish,
    ] {
        let command = init_command_startup_marker_command(shell_kind, marker_id);
        assert!(
            !command.contains(&marker),
            "startup marker command for {shell_kind:?} should not contain the full marker, got {command:?}"
        );
    }
}

#[gpui::test]
async fn test_init_command_startup_marker_ignores_echoed_command(cx: &mut TestAppContext) {
    let terminal = cx.new(|cx| {
        TerminalBuilder::new_display_only(
            SettingsCursorShape::default(),
            AlternateScroll::On,
            None,
            0,
            cx.background_executor(),
            PathStyle::local(),
        )
        .subscribe(cx)
    });
    let marker_id = 4242;
    let marker = init_command_startup_marker(marker_id);
    let command = init_command_startup_marker_command(ShellKind::Posix, marker_id);
    let (startup_tx, startup_rx) = async_channel::bounded(1);

    terminal.update(cx, |terminal, cx| {
        terminal.init_command_startup_marker = Some(marker.clone());
        terminal.init_command_startup_tx = Some(startup_tx);
        terminal.write_output(command.as_bytes(), cx);
    });
    assert!(matches!(
        startup_rx.try_recv(),
        Err(async_channel::TryRecvError::Empty)
    ));

    terminal.update(cx, |terminal, cx| {
        terminal.write_output(marker.as_bytes(), cx);
    });
    assert!(startup_rx.try_recv().is_ok());
}

#[test]
fn test_normalize_path_command_name() {
    assert_eq!(normalize_path_command_name("claude"), Some("claude".into()));
    assert_eq!(normalize_path_command_name("Cargo"), Some("cargo".into()));
    assert_eq!(normalize_path_command_name("node.exe"), Some("node".into()));
    assert_eq!(
        normalize_path_command_name("my-agent_cli.1"),
        Some("my-agent_cli.1".into())
    );
    assert_eq!(normalize_path_command_name("./local-agent"), None);
    assert_eq!(normalize_path_command_name("../local-agent"), None);
    assert_eq!(normalize_path_command_name("/usr/local/bin/cargo"), None);
    assert_eq!(
        normalize_path_command_name("target\\debug\\agent.exe"),
        None
    );
    assert_eq!(normalize_path_command_name(".hidden-agent"), None);
    assert_eq!(normalize_path_command_name("agent with spaces"), None);
    assert_eq!(normalize_path_command_name("zsh"), Some("zsh".into()));
    assert_eq!(normalize_path_command_name("-zsh"), None);
    assert_eq!(normalize_path_command_name("pwsh.exe"), Some("pwsh".into()));
}

#[test]
fn test_foreground_process_command_from_interpreter_wrapper() {
    assert_eq!(
        foreground_process_command_from_argv(&[
            "node".to_string(),
            "/opt/homebrew/lib/node_modules/@google/gemini-cli/dist/index.js".to_string(),
        ]),
        Some("gemini".to_string())
    );
    assert_eq!(
        foreground_process_command_from_argv(&[
            "python3".to_string(),
            "/Users/me/.local/bin/codex.py".to_string(),
        ]),
        Some("codex".to_string())
    );
    assert_eq!(
        foreground_process_command_from_argv(&[
            "node".to_string(),
            "/Users/me/private-project/scripts/customer-data-export.js".to_string(),
        ]),
        Some("customer-data-export".to_string())
    );
}

#[test]
fn test_convert_lf_to_crlf_preserves_split_crlf() {
    let mut previous_byte_was_cr = false;
    assert_eq!(
        convert_lf_to_crlf(b"one\n", &mut previous_byte_was_cr),
        b"one\r\n"
    );
    assert!(!previous_byte_was_cr);

    let mut previous_byte_was_cr = false;
    assert_eq!(
        convert_lf_to_crlf(b"two\r", &mut previous_byte_was_cr),
        b"two\r"
    );
    assert!(previous_byte_was_cr);
    assert_eq!(
        convert_lf_to_crlf(b"\nthree", &mut previous_byte_was_cr),
        b"\nthree"
    );
    assert!(!previous_byte_was_cr);
}

/// Regression test for the agent terminal failing with `Not a tty (os error
/// 25)` in headless/eval sandboxes: a `no_pty` task terminal must run
/// without a PTY, capture stdout, and report its exit status.
#[cfg(not(target_os = "windows"))]
async fn build_test_subprocess_terminal(
    cx: &mut TestAppContext,
    program: String,
    args: Vec<String>,
) -> (Entity<Terminal>, Receiver<Option<ExitStatus>>) {
    let (completion_tx, completion_rx) = async_channel::unbounded();
    let task_state = TaskState {
        status: TaskStatus::Running,
        completion_rx: completion_rx.clone(),
        spawned_task: SpawnInTerminal {
            command: Some(program.clone()),
            args: args.clone(),
            ..Default::default()
        },
    };
    let builder = cx
        .update(|cx| {
            cx.set_global(HeadlessTerminal(true));
            TerminalBuilder::new(
                None,
                Some(task_state),
                task::Shell::WithArguments {
                    program,
                    args,
                    title_override: None,
                },
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
                vec![],
                PathStyle::local(),
            )
        })
        .await
        .unwrap();
    let terminal = cx.new(|cx| builder.subscribe(cx));
    (terminal, completion_rx)
}

#[cfg(not(target_os = "windows"))]
#[gpui::test]
async fn test_no_pty_task_terminal_captures_output(cx: &mut TestAppContext) {
    cx.executor().allow_parking();

    let (program, args) = ShellBuilder::new(&Shell::System, false)
        .non_interactive()
        .build(Some("echo hello-from-subprocess".to_owned()), &[]);
    let (terminal, completion_rx) = build_test_subprocess_terminal(cx, program, args).await;

    assert!(
        !terminal.update(cx, |term, _| term.is_pty()),
        "no_pty terminal should not be PTY-backed"
    );
    assert_eq!(
        completion_rx.recv().await.unwrap(),
        Some(ExitStatus::default())
    );
    assert_content_eventually(&terminal, "hello-from-subprocess", cx).await;
}
