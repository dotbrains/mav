use super::*;

// allow pattern matches
#[test]
fn allow_exact_match() {
    t("cargo test").allow(&[pattern("cargo")]).is_allow();
}
#[test]
fn allow_one_of_many_patterns() {
    t("npm install")
        .allow(&[pattern("cargo"), pattern("npm")])
        .is_allow();
    t("git status")
        .allow(&[pattern("cargo"), pattern("npm"), pattern("git")])
        .is_allow();
}
#[test]
fn allow_middle_pattern() {
    t("run cargo now").allow(&["cargo"]).is_allow();
}
#[test]
fn allow_anchor_prevents_middle() {
    t("run cargo now").allow(&["^cargo"]).is_confirm();
}

// allow pattern doesn't match -> falls through
#[test]
fn allow_no_match_confirms() {
    t("python x.py").allow(&[pattern("cargo")]).is_confirm();
}
#[test]
fn allow_no_match_global_allows() {
    t("python x.py")
        .allow(&[pattern("cargo")])
        .global_default(ToolPermissionMode::Allow)
        .is_allow();
}
#[test]
fn allow_no_match_tool_confirm_overrides_global_allow() {
    t("python x.py")
        .allow(&[pattern("cargo")])
        .mode(ToolPermissionMode::Confirm)
        .global_default(ToolPermissionMode::Allow)
        .is_confirm();
}
#[test]
fn allow_no_match_tool_allow_overrides_global_confirm() {
    t("python x.py")
        .allow(&[pattern("cargo")])
        .mode(ToolPermissionMode::Allow)
        .global_default(ToolPermissionMode::Confirm)
        .is_allow();
}

// deny pattern matches (using commands that aren't blocked by hardcoded rules)
#[test]
fn deny_blocks() {
    t("rm -rf ./temp").deny(&["rm\\s+-rf"]).is_deny();
}
// global default: allow does NOT bypass user-configured deny rules
#[test]
fn deny_not_bypassed_by_global_default_allow() {
    t("rm -rf ./temp")
        .deny(&["rm\\s+-rf"])
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
}
#[test]
fn deny_blocks_with_mode_allow() {
    t("rm -rf ./temp")
        .deny(&["rm\\s+-rf"])
        .mode(ToolPermissionMode::Allow)
        .is_deny();
}
#[test]
fn deny_middle_match() {
    t("echo rm -rf ./temp").deny(&["rm\\s+-rf"]).is_deny();
}
#[test]
fn deny_no_match_falls_through() {
    t("ls -la")
        .deny(&["rm\\s+-rf"])
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

// confirm pattern matches
#[test]
fn confirm_requires_confirm() {
    t("sudo apt install")
        .confirm(&[pattern("sudo")])
        .is_confirm();
}
// global default: allow does NOT bypass user-configured confirm rules
#[test]
fn global_default_allow_does_not_override_confirm_pattern() {
    t("sudo reboot")
        .confirm(&[pattern("sudo")])
        .global_default(ToolPermissionMode::Allow)
        .is_confirm();
}
#[test]
fn confirm_overrides_mode_allow() {
    t("sudo x")
        .confirm(&["sudo"])
        .mode(ToolPermissionMode::Allow)
        .is_confirm();
}

// confirm beats allow
#[test]
fn confirm_beats_allow() {
    t("git push --force")
        .allow(&[pattern("git")])
        .confirm(&["--force"])
        .is_confirm();
}
#[test]
fn confirm_beats_allow_overlap() {
    t("deploy prod")
        .allow(&["deploy"])
        .confirm(&["prod"])
        .is_confirm();
}
#[test]
fn allow_when_confirm_no_match() {
    t("git status")
        .allow(&[pattern("git")])
        .confirm(&["--force"])
        .is_allow();
}

// deny beats allow
#[test]
fn deny_beats_allow() {
    t("rm -rf ./tmp/x")
        .allow(&["/tmp/"])
        .deny(&["rm\\s+-rf"])
        .is_deny();
}

#[test]
fn deny_beats_confirm() {
    t("sudo rm -rf ./temp")
        .confirm(&["sudo"])
        .deny(&["rm\\s+-rf"])
        .is_deny();
}

// deny beats everything
#[test]
fn deny_beats_all() {
    t("bad cmd")
        .allow(&["cmd"])
        .confirm(&["cmd"])
        .deny(&["bad"])
        .is_deny();
}

// no patterns -> default
#[test]
fn default_confirm() {
    t("python x.py")
        .mode(ToolPermissionMode::Confirm)
        .is_confirm();
}
#[test]
fn default_allow() {
    t("python x.py").mode(ToolPermissionMode::Allow).is_allow();
}
#[test]
fn default_deny() {
    t("python x.py").mode(ToolPermissionMode::Deny).is_deny();
}
// Tool-specific default takes precedence over global default
#[test]
fn tool_default_deny_overrides_global_allow() {
    t("python x.py")
        .mode(ToolPermissionMode::Deny)
        .global_default(ToolPermissionMode::Allow)
        .is_deny();
}

// Tool-specific default takes precedence over global default
#[test]
fn tool_default_confirm_overrides_global_allow() {
    t("x")
        .mode(ToolPermissionMode::Confirm)
        .global_default(ToolPermissionMode::Allow)
        .is_confirm();
}

#[test]
fn no_rules_uses_global_default() {
    assert_eq!(
        no_rules("x", ToolPermissionMode::Confirm),
        ToolPermissionDecision::Confirm
    );
    assert_eq!(
        no_rules("x", ToolPermissionMode::Allow),
        ToolPermissionDecision::Allow
    );
    assert!(matches!(
        no_rules("x", ToolPermissionMode::Deny),
        ToolPermissionDecision::Deny(_)
    ));
}

#[test]
fn empty_input_no_match() {
    t("")
        .deny(&["rm"])
        .mode(ToolPermissionMode::Allow)
        .is_allow();
}

#[test]
fn empty_input_with_allow_falls_to_default() {
    t("").allow(&["^ls"]).is_confirm();
}

#[test]
fn multi_deny_any_match() {
    t("rm x").deny(&["rm", "del", "drop"]).is_deny();
    t("drop x").deny(&["rm", "del", "drop"]).is_deny();
}

#[test]
fn multi_allow_any_match() {
    t("cargo x").allow(&["^cargo", "^npm", "^git"]).is_allow();
}
#[test]
fn multi_none_match() {
    t("python x")
        .allow(&["^cargo", "^npm"])
        .deny(&["rm"])
        .is_confirm();
}

// tool isolation
#[test]
fn other_tool_not_affected() {
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
        Arc::from(EditFileTool::NAME),
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
            EditFileTool::NAME,
            &["x".to_string()],
            &p,
            ShellKind::Posix
        ),
        ToolPermissionDecision::Allow
    );
}

#[test]
fn partial_tool_name_no_match() {
    let mut tools = collections::HashMap::default();
    tools.insert(
        Arc::from("term"),
        ToolRules {
            default: Some(ToolPermissionMode::Deny),
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
    // "terminal" should not match "term" rules, so falls back to Confirm (no rules)
    assert_eq!(
        ToolPermissionDecision::from_input(
            TerminalTool::NAME,
            &["x".to_string()],
            &p,
            ShellKind::Posix
        ),
        ToolPermissionDecision::Confirm
    );
}

// invalid patterns block the tool
#[test]
fn invalid_pattern_blocks() {
    let mut tools = collections::HashMap::default();
    tools.insert(
        Arc::from(TerminalTool::NAME),
        ToolRules {
            default: Some(ToolPermissionMode::Allow),
            always_allow: vec![CompiledRegex::new("echo", false).unwrap()],
            always_deny: vec![],
            always_confirm: vec![],
            invalid_patterns: vec![InvalidRegexPattern {
                pattern: "[bad".into(),
                rule_type: "always_deny".into(),
                error: "err".into(),
            }],
        },
    );
    let p = ToolPermissions {
        default: ToolPermissionMode::Confirm,
        tools,
    };
    // Invalid patterns block the tool regardless of other settings
    assert!(matches!(
        ToolPermissionDecision::from_input(
            TerminalTool::NAME,
            &["echo hi".to_string()],
            &p,
            ShellKind::Posix
        ),
        ToolPermissionDecision::Deny(_)
    ));
}

#[test]
fn invalid_substitution_bearing_command_denies_by_default() {
    let decision = no_rules("echo $HOME", ToolPermissionMode::Deny);
    assert!(matches!(decision, ToolPermissionDecision::Deny(_)));
}

#[test]
fn invalid_substitution_bearing_command_denies_in_confirm_mode() {
    let decision = no_rules("echo $(whoami)", ToolPermissionMode::Confirm);
    assert!(matches!(decision, ToolPermissionDecision::Deny(_)));
}

#[test]
fn unconditional_allow_all_bypasses_invalid_command_rejection_without_tool_rules() {
    let decision = no_rules("echo $HOME", ToolPermissionMode::Allow);
    assert_eq!(decision, ToolPermissionDecision::Allow);
}

#[test]
fn unconditional_allow_all_bypasses_invalid_command_rejection_with_terminal_default_allow() {
    let mut tools = collections::HashMap::default();
    tools.insert(
        Arc::from(TerminalTool::NAME),
        ToolRules {
            default: Some(ToolPermissionMode::Allow),
            always_allow: vec![],
            always_deny: vec![],
            always_confirm: vec![],
            invalid_patterns: vec![],
        },
    );
    let permissions = ToolPermissions {
        default: ToolPermissionMode::Confirm,
        tools,
    };

    assert_eq!(
        ToolPermissionDecision::from_input(
            TerminalTool::NAME,
            &["echo $(whoami)".to_string()],
            &permissions,
            ShellKind::Posix,
        ),
        ToolPermissionDecision::Allow
    );
}
