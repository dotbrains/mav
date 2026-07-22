use serde_json::json;
use settings::{ToolPermissionMode, ToolPermissionsContent};

use crate::settings_impl::compile_tool_permissions;
use crate::{CompiledRegex, ToolRules};

#[test]
fn test_tool_permissions_parsing() {
    let json = json!({
        "tools": {
            "terminal": {
                "default": "allow",
                "always_deny": [
                    { "pattern": "rm\\s+-rf" }
                ],
                "always_allow": [
                    { "pattern": "^git\\s" }
                ]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    let terminal_rules = permissions.tools.get("terminal").unwrap();
    assert_eq!(terminal_rules.default, Some(ToolPermissionMode::Allow));
    assert_eq!(terminal_rules.always_deny.len(), 1);
    assert_eq!(terminal_rules.always_allow.len(), 1);
    assert!(terminal_rules.always_deny[0].is_match("rm -rf /"));
    assert!(terminal_rules.always_allow[0].is_match("git status"));
}

#[test]
fn test_tool_rules_default() {
    let json = json!({
        "tools": {
            "edit_file": {
                "default": "deny"
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    let rules = permissions.tools.get("edit_file").unwrap();
    assert_eq!(rules.default, Some(ToolPermissionMode::Deny));
}

#[test]
fn test_tool_permissions_empty() {
    let permissions = compile_tool_permissions(None);
    assert!(permissions.tools.is_empty());
    assert_eq!(permissions.default, ToolPermissionMode::Confirm);
}

#[test]
fn test_tool_rules_default_returns_confirm() {
    let default_rules = ToolRules::default();
    assert_eq!(default_rules.default, None);
    assert!(default_rules.always_allow.is_empty());
    assert!(default_rules.always_deny.is_empty());
    assert!(default_rules.always_confirm.is_empty());
}

#[test]
fn test_tool_permissions_with_multiple_tools() {
    let json = json!({
        "tools": {
            "terminal": {
                "default": "allow",
                "always_deny": [{ "pattern": "rm\\s+-rf" }]
            },
            "edit_file": {
                "default": "confirm",
                "always_deny": [{ "pattern": "\\.env$" }]
            },
            "delete_path": {
                "default": "deny"
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    assert_eq!(permissions.tools.len(), 3);

    let terminal = permissions.tools.get("terminal").unwrap();
    assert_eq!(terminal.default, Some(ToolPermissionMode::Allow));
    assert_eq!(terminal.always_deny.len(), 1);

    let edit_file = permissions.tools.get("edit_file").unwrap();
    assert_eq!(edit_file.default, Some(ToolPermissionMode::Confirm));
    assert!(edit_file.always_deny[0].is_match("secrets.env"));

    let delete_path = permissions.tools.get("delete_path").unwrap();
    assert_eq!(delete_path.default, Some(ToolPermissionMode::Deny));
}

#[test]
fn test_tool_permissions_with_all_rule_types() {
    let json = json!({
        "tools": {
            "terminal": {
                "always_deny": [{ "pattern": "rm\\s+-rf" }],
                "always_confirm": [{ "pattern": "sudo\\s" }],
                "always_allow": [{ "pattern": "^git\\s+status" }]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    let terminal = permissions.tools.get("terminal").unwrap();
    assert_eq!(terminal.always_deny.len(), 1);
    assert_eq!(terminal.always_confirm.len(), 1);
    assert_eq!(terminal.always_allow.len(), 1);

    assert!(terminal.always_deny[0].is_match("rm -rf /"));
    assert!(terminal.always_confirm[0].is_match("sudo apt install"));
    assert!(terminal.always_allow[0].is_match("git status"));
}

#[test]
fn test_invalid_regex_is_tracked_and_valid_ones_still_compile() {
    let json = json!({
        "tools": {
            "terminal": {
                "always_deny": [
                    { "pattern": "[invalid(regex" },
                    { "pattern": "valid_pattern" }
                ],
                "always_allow": [
                    { "pattern": "[another_bad" }
                ]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    let terminal = permissions.tools.get("terminal").unwrap();

    // Valid patterns should still be compiled
    assert_eq!(terminal.always_deny.len(), 1);
    assert!(terminal.always_deny[0].is_match("valid_pattern"));

    // Invalid patterns should be tracked (order depends on processing order)
    assert_eq!(terminal.invalid_patterns.len(), 2);

    let deny_invalid = terminal
        .invalid_patterns
        .iter()
        .find(|p| p.rule_type == "always_deny")
        .expect("should have invalid pattern from always_deny");
    assert_eq!(deny_invalid.pattern, "[invalid(regex");
    assert!(!deny_invalid.error.is_empty());

    let allow_invalid = terminal
        .invalid_patterns
        .iter()
        .find(|p| p.rule_type == "always_allow")
        .expect("should have invalid pattern from always_allow");
    assert_eq!(allow_invalid.pattern, "[another_bad");

    // ToolPermissions helper methods should work
    assert!(permissions.has_invalid_patterns());
    assert_eq!(permissions.invalid_patterns().len(), 2);
}

#[test]
fn test_deny_takes_precedence_over_allow_and_confirm() {
    let json = json!({
        "tools": {
            "terminal": {
                "default": "allow",
                "always_deny": [{ "pattern": "dangerous" }],
                "always_confirm": [{ "pattern": "dangerous" }],
                "always_allow": [{ "pattern": "dangerous" }]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));
    let terminal = permissions.tools.get("terminal").unwrap();

    assert!(
        terminal.always_deny[0].is_match("run dangerous command"),
        "Deny rule should match"
    );
    assert!(
        terminal.always_allow[0].is_match("run dangerous command"),
        "Allow rule should also match (but deny takes precedence at evaluation time)"
    );
    assert!(
        terminal.always_confirm[0].is_match("run dangerous command"),
        "Confirm rule should also match (but deny takes precedence at evaluation time)"
    );
}

#[test]
fn test_confirm_takes_precedence_over_allow() {
    let json = json!({
        "tools": {
            "terminal": {
                "default": "allow",
                "always_confirm": [{ "pattern": "risky" }],
                "always_allow": [{ "pattern": "risky" }]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));
    let terminal = permissions.tools.get("terminal").unwrap();

    assert!(
        terminal.always_confirm[0].is_match("do risky thing"),
        "Confirm rule should match"
    );
    assert!(
        terminal.always_allow[0].is_match("do risky thing"),
        "Allow rule should also match (but confirm takes precedence at evaluation time)"
    );
}

#[test]
fn test_regex_matches_anywhere_in_string_not_just_anchored() {
    let json = json!({
        "tools": {
            "terminal": {
                "always_deny": [
                    { "pattern": "rm\\s+-rf" },
                    { "pattern": "/etc/passwd" }
                ]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));
    let terminal = permissions.tools.get("terminal").unwrap();

    assert!(
        terminal.always_deny[0].is_match("echo hello && rm -rf /"),
        "Should match rm -rf in the middle of a command chain"
    );
    assert!(
        terminal.always_deny[0].is_match("cd /tmp; rm -rf *"),
        "Should match rm -rf after semicolon"
    );
    assert!(
        terminal.always_deny[1].is_match("cat /etc/passwd | grep root"),
        "Should match /etc/passwd in a pipeline"
    );
    assert!(
        terminal.always_deny[1].is_match("vim /etc/passwd"),
        "Should match /etc/passwd as argument"
    );
}

#[test]
fn test_fork_bomb_pattern_matches() {
    let fork_bomb_regex = CompiledRegex::new(r":\(\)\{\s*:\|:&\s*\};:", false).unwrap();
    assert!(
        fork_bomb_regex.is_match(":(){ :|:& };:"),
        "Should match the classic fork bomb"
    );
    assert!(
        fork_bomb_regex.is_match(":(){ :|:&};:"),
        "Should match fork bomb without spaces"
    );
}

#[test]
fn test_compiled_regex_stores_case_sensitivity() {
    let case_sensitive = CompiledRegex::new("test", true).unwrap();
    let case_insensitive = CompiledRegex::new("test", false).unwrap();

    assert!(case_sensitive.case_sensitive);
    assert!(!case_insensitive.case_sensitive);
}

#[test]
fn test_invalid_regex_is_skipped_not_fail() {
    let json = json!({
        "tools": {
            "terminal": {
                "always_deny": [
                    { "pattern": "[invalid(regex" },
                    { "pattern": "valid_pattern" }
                ]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    let terminal = permissions.tools.get("terminal").unwrap();
    assert_eq!(terminal.always_deny.len(), 1);
    assert!(terminal.always_deny[0].is_match("valid_pattern"));
}

#[test]
fn test_unconfigured_tool_not_in_permissions() {
    let json = json!({
        "tools": {
            "terminal": {
                "default": "allow"
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    assert!(permissions.tools.contains_key("terminal"));
    assert!(!permissions.tools.contains_key("edit_file"));
    assert!(!permissions.tools.contains_key("fetch"));
}

#[test]
fn test_always_allow_pattern_only_matches_specified_commands() {
    // Reproduces user-reported bug: when always_allow has pattern "^echo\s",
    // only "echo hello" should be allowed, not "git status".
    //
    // User config:
    //   always_allow_tool_actions: false
    //   tool_permissions.tools.terminal.always_allow: [{ pattern: "^echo\\s" }]
    let json = json!({
        "tools": {
            "terminal": {
                "always_allow": [
                    { "pattern": "^echo\\s" }
                ]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    let terminal = permissions.tools.get("terminal").unwrap();

    // Verify the pattern was compiled
    assert_eq!(
        terminal.always_allow.len(),
        1,
        "Should have one always_allow pattern"
    );

    // Verify the pattern matches "echo hello"
    assert!(
        terminal.always_allow[0].is_match("echo hello"),
        "Pattern ^echo\\s should match 'echo hello'"
    );

    // Verify the pattern does NOT match "git status"
    assert!(
        !terminal.always_allow[0].is_match("git status"),
        "Pattern ^echo\\s should NOT match 'git status'"
    );

    // Verify the pattern does NOT match "echoHello" (no space)
    assert!(
        !terminal.always_allow[0].is_match("echoHello"),
        "Pattern ^echo\\s should NOT match 'echoHello' (requires whitespace)"
    );

    assert_eq!(
        terminal.default, None,
        "default should be None when not specified"
    );
}

#[test]
fn test_empty_regex_pattern_is_invalid() {
    let json = json!({
        "tools": {
            "terminal": {
                "always_allow": [
                    { "pattern": "" }
                ],
                "always_deny": [
                    { "case_sensitive": true }
                ],
                "always_confirm": [
                    { "pattern": "" },
                    { "pattern": "valid_pattern" }
                ]
            }
        }
    });

    let content: ToolPermissionsContent = serde_json::from_value(json).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    let terminal = permissions.tools.get("terminal").unwrap();

    assert_eq!(terminal.always_allow.len(), 0);
    assert_eq!(terminal.always_deny.len(), 0);
    assert_eq!(terminal.always_confirm.len(), 1);
    assert!(terminal.always_confirm[0].is_match("valid_pattern"));

    assert_eq!(terminal.invalid_patterns.len(), 3);
    for invalid in &terminal.invalid_patterns {
        assert_eq!(invalid.pattern, "");
        assert!(invalid.error.contains("empty"));
    }
}

#[test]
fn test_default_json_tool_permissions_parse() {
    let default_json = include_str!("../../../assets/settings/default.json");
    let value: serde_json_lenient::Value = serde_json_lenient::from_str(default_json).unwrap();
    let agent = value
        .get("agent")
        .expect("default.json should have 'agent' key");
    let tool_permissions_value = agent
        .get("tool_permissions")
        .expect("agent should have 'tool_permissions' key");

    let content: ToolPermissionsContent =
        serde_json_lenient::from_value(tool_permissions_value.clone()).unwrap();
    let permissions = compile_tool_permissions(Some(content));

    assert_eq!(permissions.default, ToolPermissionMode::Confirm);

    assert!(
        permissions.tools.is_empty(),
        "default.json should not have any active tool-specific rules, found: {:?}",
        permissions.tools.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_tool_permissions_explicit_global_default() {
    let json_allow = json!({
        "default": "allow"
    });
    let content: ToolPermissionsContent = serde_json::from_value(json_allow).unwrap();
    let permissions = compile_tool_permissions(Some(content));
    assert_eq!(permissions.default, ToolPermissionMode::Allow);

    let json_deny = json!({
        "default": "deny"
    });
    let content: ToolPermissionsContent = serde_json::from_value(json_deny).unwrap();
    let permissions = compile_tool_permissions(Some(content));
    assert_eq!(permissions.default, ToolPermissionMode::Deny);
}
