#[test]
fn test_process_content_user_stopped() {
    let output = acp::TerminalOutputResponse::new("partial output".to_string(), false);

    let result = process_content(
        output,
        "cargo build",
        false,
        true,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("user stopped"),
        "Expected 'user stopped' message, got: {}",
        result
    );
    assert!(
        result.contains("partial output"),
        "Expected output to be included, got: {}",
        result
    );
    assert!(
        result.contains("ask them what they would like to do"),
        "Should instruct agent to ask user, got: {}",
        result
    );
}

#[test]
fn test_select_terminal_output_head_lines() {
    let output = "one\ntwo\nthree\nfour";
    let result = select_terminal_output_lines(
        output,
        TerminalOutputSelection {
            head_lines: Some(2),
            tail_lines: None,
        },
    );

    assert_eq!(result, "one\ntwo");
}

#[test]
fn test_select_terminal_output_tail_lines() {
    let output = "one\ntwo\nthree\nfour";
    let result = select_terminal_output_lines(
        output,
        TerminalOutputSelection {
            head_lines: None,
            tail_lines: Some(2),
        },
    );

    assert_eq!(result, "three\nfour");
}

#[test]
fn test_select_terminal_output_head_and_tail_lines() {
    let output = "one\ntwo\nthree\nfour\nfive";
    let result = select_terminal_output_lines(
        output,
        TerminalOutputSelection {
            head_lines: Some(2),
            tail_lines: Some(2),
        },
    );

    assert_eq!(result, "one\ntwo\n\nfour\nfive");
}

#[test]
fn test_select_terminal_output_head_and_tail_lines_overlap() {
    let output = "one\ntwo\nthree";
    let result = select_terminal_output_lines(
        output,
        TerminalOutputSelection {
            head_lines: Some(2),
            tail_lines: Some(2),
        },
    );

    assert_eq!(result, "one\ntwo\n\ntwo\nthree");
}

#[test]
fn test_select_terminal_output_allows_zero_lines() {
    let output = "one\ntwo\nthree";

    assert_eq!(
        select_terminal_output_lines(
            output,
            TerminalOutputSelection {
                head_lines: Some(0),
                tail_lines: None,
            },
        ),
        ""
    );
    assert_eq!(
        select_terminal_output_lines(
            output,
            TerminalOutputSelection {
                head_lines: None,
                tail_lines: Some(0),
            },
        ),
        ""
    );
    assert_eq!(
        select_terminal_output_lines(
            output,
            TerminalOutputSelection {
                head_lines: Some(0),
                tail_lines: Some(0),
            },
        ),
        "\n\n"
    );
}

#[test]
fn test_select_terminal_output_handles_unicode_without_trailing_newline() {
    let output = "α\nβ\nγ";
    let result = select_terminal_output_lines(
        output,
        TerminalOutputSelection {
            head_lines: None,
            tail_lines: Some(2),
        },
    );

    assert_eq!(result, "β\nγ");
}

#[test]
fn test_process_content_filters_success_output_for_model() {
    let output = acp::TerminalOutputResponse::new("one\ntwo\nthree\nfour".to_string(), false)
        .exit_status(acp::TerminalExitStatus::new().exit_code(0));

    let result = process_content(
        output,
        "printf lines",
        false,
        false,
        TerminalOutputSelection {
            head_lines: Some(1),
            tail_lines: Some(1),
        },
    );

    assert_eq!(result, "```\none\n\nfour\n```");
}

#[test]
fn test_process_content_filters_failure_output_for_model() {
    let output = acp::TerminalOutputResponse::new("one\ntwo\nthree".to_string(), false)
        .exit_status(acp::TerminalExitStatus::new().exit_code(1));

    let result = process_content(
        output,
        "failing command",
        false,
        false,
        TerminalOutputSelection {
            head_lines: None,
            tail_lines: Some(1),
        },
    );

    assert!(result.contains("failed with exit code 1"));
    assert!(result.contains("three"));
    assert!(!result.contains("one"));
    assert!(!result.contains("two"));
}

#[test]
fn test_process_content_filters_timeout_output_for_model() {
    let output = acp::TerminalOutputResponse::new("one\ntwo\nthree".to_string(), false);

    let result = process_content(
        output,
        "slow command",
        true,
        false,
        TerminalOutputSelection {
            head_lines: Some(1),
            tail_lines: None,
        },
    );

    assert!(result.contains("timed out"));
    assert!(result.contains("one"));
    assert!(!result.contains("two"));
    assert!(!result.contains("three"));
}

#[test]
fn test_process_content_filters_user_stopped_output_for_model() {
    let output = acp::TerminalOutputResponse::new("one\ntwo\nthree".to_string(), false);

    let result = process_content(
        output,
        "stopped command",
        false,
        true,
        TerminalOutputSelection {
            head_lines: None,
            tail_lines: Some(1),
        },
    );

    assert!(result.contains("user stopped"));
    assert!(result.contains("ask them what they would like to do"));
    assert!(result.contains("three"));
    assert!(!result.contains("one"));
    assert!(!result.contains("two"));
}

#[test]
fn test_process_content_selected_output_has_no_explanatory_note() {
    let output = acp::TerminalOutputResponse::new("one\ntwo\nthree".to_string(), false)
        .exit_status(acp::TerminalExitStatus::new().exit_code(0));

    let result = process_content(
        output,
        "printf lines",
        false,
        false,
        TerminalOutputSelection {
            head_lines: Some(1),
            tail_lines: Some(1),
        },
    );

    assert!(!result.contains("Showing"));
    assert!(!result.contains("first"));
    assert!(!result.contains("last"));
}

#[test]
fn test_process_content_user_stopped_empty_output() {
    let output = acp::TerminalOutputResponse::new("".to_string(), false);

    let result = process_content(
        output,
        "cargo build",
        false,
        true,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("user stopped"),
        "Expected 'user stopped' message, got: {}",
        result
    );
    assert!(
        result.contains("No output was captured"),
        "Expected 'No output was captured', got: {}",
        result
    );
}

#[test]
fn test_process_content_timed_out() {
    let output = acp::TerminalOutputResponse::new("build output here".to_string(), false);

    let result = process_content(
        output,
        "cargo build",
        true,
        false,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("timed out"),
        "Expected 'timed out' message for timeout, got: {}",
        result
    );
    assert!(
        result.contains("build output here"),
        "Expected output to be included, got: {}",
        result
    );
}

#[test]
fn test_process_content_timed_out_with_empty_output() {
    let output = acp::TerminalOutputResponse::new("".to_string(), false);

    let result = process_content(
        output,
        "sleep 1000",
        true,
        false,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("timed out"),
        "Expected 'timed out' for timeout, got: {}",
        result
    );
    assert!(
        result.contains("No output was captured"),
        "Expected 'No output was captured' for empty output, got: {}",
        result
    );
}

#[test]
fn test_process_content_with_success() {
    let output = acp::TerminalOutputResponse::new("success output".to_string(), false)
        .exit_status(acp::TerminalExitStatus::new().exit_code(0));

    let result = process_content(
        output,
        "echo hello",
        false,
        false,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("success output"),
        "Expected output to be included, got: {}",
        result
    );
    assert!(
        !result.contains("failed"),
        "Success should not say 'failed', got: {}",
        result
    );
}

#[test]
fn test_process_content_with_success_empty_output() {
    let output = acp::TerminalOutputResponse::new("".to_string(), false)
        .exit_status(acp::TerminalExitStatus::new().exit_code(0));

    let result = process_content(
        output,
        "true",
        false,
        false,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("executed successfully"),
        "Expected success message for empty output, got: {}",
        result
    );
}

#[test]
fn test_process_content_with_error_exit() {
    let output = acp::TerminalOutputResponse::new("error output".to_string(), false)
        .exit_status(acp::TerminalExitStatus::new().exit_code(1));

    let result = process_content(
        output,
        "false",
        false,
        false,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("failed with exit code 1"),
        "Expected failure message, got: {}",
        result
    );
    assert!(
        result.contains("error output"),
        "Expected output to be included, got: {}",
        result
    );
}

#[test]
fn test_process_content_with_error_exit_empty_output() {
    let output = acp::TerminalOutputResponse::new("".to_string(), false)
        .exit_status(acp::TerminalExitStatus::new().exit_code(1));

    let result = process_content(
        output,
        "false",
        false,
        false,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("failed with exit code 1"),
        "Expected failure message, got: {}",
        result
    );
}

#[test]
fn test_process_content_unexpected_termination() {
    let output = acp::TerminalOutputResponse::new("some output".to_string(), false);

    let result = process_content(
        output,
        "some_command",
        false,
        false,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("terminated unexpectedly"),
        "Expected 'terminated unexpectedly' message, got: {}",
        result
    );
    assert!(
        result.contains("some output"),
        "Expected output to be included, got: {}",
        result
    );
}

#[test]
fn test_process_content_unexpected_termination_empty_output() {
    let output = acp::TerminalOutputResponse::new("".to_string(), false);

    let result = process_content(
        output,
        "some_command",
        false,
        false,
        TerminalOutputSelection::default(),
    );

    assert!(
        result.contains("terminated unexpectedly"),
        "Expected 'terminated unexpectedly' message, got: {}",
        result
    );
    assert!(
        result.contains("No output was captured"),
        "Expected 'No output was captured' for empty output, got: {}",
        result
    );
}
