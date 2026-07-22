use super::*;

#[gpui::test]
async fn test_write_init_command_after_startup_clears_without_shell_command(
    cx: &mut TestAppContext,
) {
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

    terminal.update(cx, |terminal, cx| {
        terminal.write_output(b"startup output\nprompt", cx);
    });

    let wrote = terminal.update(cx, |terminal, cx| {
        terminal.write_init_command_after_startup(b"agent\r".to_vec(), cx)
    });
    assert!(wrote);
    let content = terminal.update(cx, |terminal, _| terminal.get_content());
    assert!(
        !content.contains("startup output"),
        "startup output should be cleared internally before writing the init command"
    );
    let input_log = terminal.update(cx, |terminal, _| terminal.take_input_log());
    assert_eq!(input_log, vec![b"agent\r".to_vec()]);
}

#[gpui::test]
async fn test_write_init_command_after_startup_skips_after_keyboard_input(cx: &mut TestAppContext) {
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

    let wrote = terminal.update(cx, |terminal, cx| {
        terminal.write_output(b"startup output\nprompt", cx);
        terminal.input(b"user input".to_vec());
        terminal.write_init_command_after_startup(b"agent\r".to_vec(), cx)
    });
    assert!(!wrote);
    let content = terminal.update(cx, |terminal, _| terminal.get_content());
    assert!(
        content.contains("startup output"),
        "startup output should be left alone when the init command is skipped"
    );
    let input_log = terminal.update(cx, |terminal, _| terminal.take_input_log());
    assert_eq!(input_log, vec![b"user input".to_vec()]);
}

#[gpui::test]
async fn test_write_init_command_after_startup_skips_after_child_exit(cx: &mut TestAppContext) {
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

    terminal.update(cx, |terminal, cx| {
        terminal.write_output(b"shell failed to start\nprompt", cx);
        #[cfg(unix)]
        let exit_status = <ExitStatus as std::os::unix::process::ExitStatusExt>::from_raw(1 << 8);
        #[cfg(windows)]
        let exit_status = <ExitStatus as std::os::windows::process::ExitStatusExt>::from_raw(1);
        terminal.register_task_finished(Some(exit_status), cx);
    });

    let wrote = terminal.update(cx, |terminal, cx| {
        terminal.write_init_command_after_startup(b"agent\r".to_vec(), cx)
    });
    assert!(!wrote);
    let content = terminal.update(cx, |terminal, _| terminal.get_content());
    assert!(
        content.contains("shell failed to start"),
        "startup failure output should be preserved when the init command is skipped"
    );
    let input_log = terminal.update(cx, |terminal, _| terminal.take_input_log());
    assert!(
        input_log.is_empty(),
        "init command should not be written after the child has exited, got {input_log:?}"
    );
}

#[gpui::test]
async fn test_write_output_converts_lf_to_crlf(cx: &mut TestAppContext) {
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

    // Test simple LF conversion
    terminal.update(cx, |terminal, cx| {
        terminal.write_output(b"line1\nline2\n", cx);
    });

    // Get the content by directly accessing the term
    let content = terminal.update(cx, |terminal, _cx| {
        let term = terminal.term.lock_unfair();
        make_content(&term, &terminal.last_content)
    });

    // If LF is properly converted to CRLF, each line should start at column 0
    // The diagonal staircase bug would cause increasing column positions

    // Get the cells and check that lines start at column 0
    let cells = &content.cells;
    let mut line1_col0 = false;
    let mut line2_col0 = false;

    for cell in cells {
        if cell.character() == 'l' && cell.point.column == 0 {
            if cell.point.line == 0 && !line1_col0 {
                line1_col0 = true;
            } else if cell.point.line == 1 && !line2_col0 {
                line2_col0 = true;
            }
        }
    }

    assert!(line1_col0, "First line should start at column 0");
    assert!(line2_col0, "Second line should start at column 0");
}

#[gpui::test]
async fn test_write_output_preserves_existing_crlf(cx: &mut TestAppContext) {
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

    // Test that existing CRLF doesn't get doubled
    terminal.update(cx, |terminal, cx| {
        terminal.write_output(b"line1\r\nline2\r\n", cx);
    });

    // Get the content by directly accessing the term
    let content = terminal.update(cx, |terminal, _cx| {
        let term = terminal.term.lock_unfair();
        make_content(&term, &terminal.last_content)
    });

    let cells = &content.cells;

    // Check that both lines start at column 0
    let mut found_lines_at_column_0 = 0;
    for cell in cells {
        if cell.character() == 'l' && cell.point.column == 0 {
            found_lines_at_column_0 += 1;
        }
    }

    assert!(
        found_lines_at_column_0 >= 2,
        "Both lines should start at column 0"
    );
}

#[gpui::test]
async fn test_write_output_preserves_bare_cr(cx: &mut TestAppContext) {
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

    // Test that bare CR (without LF) is preserved
    terminal.update(cx, |terminal, cx| {
        terminal.write_output(b"hello\rworld", cx);
    });

    // Get the content by directly accessing the term
    let content = terminal.update(cx, |terminal, _cx| {
        let term = terminal.term.lock_unfair();
        make_content(&term, &terminal.last_content)
    });

    let cells = &content.cells;

    // Check that we have "world" at the beginning of the line
    let mut text = String::new();
    for cell in cells.iter().take(5) {
        if cell.point.line == 0 {
            text.push(cell.character());
        }
    }

    assert!(
        text.starts_with("world"),
        "Bare CR should allow overwriting: got '{}'",
        text
    );
}

#[gpui::test]
async fn test_display_only_write_output_ignores_osc52(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = settings::SettingsStore::test(cx);
        cx.set_global(settings_store);
        cx.write_to_clipboard(ClipboardItem::new_string("original".to_string()));
    });

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

    terminal.update(cx, |terminal, cx| {
        terminal.write_output(b"\x1b]52;c;b3ZlcndyaXR0ZW4=\x07", cx);
    });
    cx.run_until_parked();

    let clipboard_text = cx.update(|cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(clipboard_text.as_deref(), Some("original"));
}
