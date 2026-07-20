use super::*;
use pretty_assertions::assert_eq;

#[test]
fn test_permission_options_terminal_with_pattern() {
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["cargo build --release".to_string()],
    )
    .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    assert_eq!(choices.len(), 3);
    let labels: Vec<&str> = choices
        .iter()
        .map(|choice| choice.allow.name.as_ref())
        .collect();
    assert!(labels.contains(&"Always for terminal"));
    assert!(labels.contains(&"Always for `cargo build` commands"));
    assert!(labels.contains(&"Only this time"));
}

#[test]
fn test_permission_options_terminal_command_with_flag_second_token() {
    let permission_options =
        ToolPermissionContext::new(TerminalTool::NAME, vec!["ls -la".to_string()])
            .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    assert_eq!(choices.len(), 3);
    let labels: Vec<&str> = choices
        .iter()
        .map(|choice| choice.allow.name.as_ref())
        .collect();
    assert!(labels.contains(&"Always for terminal"));
    assert!(labels.contains(&"Always for `ls` commands"));
    assert!(labels.contains(&"Only this time"));
}

#[test]
fn test_permission_options_terminal_single_word_command() {
    let permission_options =
        ToolPermissionContext::new(TerminalTool::NAME, vec!["whoami".to_string()])
            .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    assert_eq!(choices.len(), 3);
    let labels: Vec<&str> = choices
        .iter()
        .map(|choice| choice.allow.name.as_ref())
        .collect();
    assert!(labels.contains(&"Always for terminal"));
    assert!(labels.contains(&"Always for `whoami` commands"));
    assert!(labels.contains(&"Only this time"));
}

#[test]
fn test_permission_options_edit_file_with_path_pattern() {
    let permission_options =
        ToolPermissionContext::new(EditFileTool::NAME, vec!["src/main.rs".to_string()])
            .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    let labels: Vec<&str> = choices
        .iter()
        .map(|choice| choice.allow.name.as_ref())
        .collect();
    assert!(labels.contains(&"Always for edit file"));
    assert!(labels.contains(&"Always for `src/`"));
}

#[test]
fn test_permission_options_fetch_with_domain_pattern() {
    let permission_options =
        ToolPermissionContext::new(FetchTool::NAME, vec!["https://docs.rs/gpui".to_string()])
            .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    let labels: Vec<&str> = choices
        .iter()
        .map(|choice| choice.allow.name.as_ref())
        .collect();
    assert!(labels.contains(&"Always for fetch"));
    assert!(labels.contains(&"Always for `docs.rs`"));
}

#[test]
fn test_permission_options_without_pattern() {
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["./deploy.sh --production".to_string()],
    )
    .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    assert_eq!(choices.len(), 2);
    let labels: Vec<&str> = choices
        .iter()
        .map(|choice| choice.allow.name.as_ref())
        .collect();
    assert!(labels.contains(&"Always for terminal"));
    assert!(labels.contains(&"Only this time"));
    assert!(!labels.iter().any(|label| label.contains("commands")));
}

#[test]
fn test_permission_options_symlink_target_are_flat_once_only() {
    let permission_options =
        ToolPermissionContext::symlink_target(EditFileTool::NAME, vec!["/outside/file.txt".into()])
            .build_permission_options();

    let PermissionOptions::Flat(options) = permission_options else {
        panic!("Expected flat permission options for symlink target authorization");
    };

    assert_eq!(options.len(), 2);
    assert!(options.iter().any(|option| {
        option.option_id.0.as_ref() == "allow"
            && option.kind == acp::PermissionOptionKind::AllowOnce
    }));
    assert!(options.iter().any(|option| {
        option.option_id.0.as_ref() == "deny"
            && option.kind == acp::PermissionOptionKind::RejectOnce
    }));
}

#[test]
fn test_permission_option_ids_for_terminal() {
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["cargo build --release".to_string()],
    )
    .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    // Expect 3 choices: always-tool, always-pattern, once
    assert_eq!(choices.len(), 3);

    // First two choices both use the tool-level option IDs
    assert_eq!(
        choices[0].allow.option_id.0.as_ref(),
        "always_allow:terminal"
    );
    assert_eq!(choices[0].deny.option_id.0.as_ref(), "always_deny:terminal");
    assert!(choices[0].sub_patterns.is_empty());

    assert_eq!(
        choices[1].allow.option_id.0.as_ref(),
        "always_allow:terminal"
    );
    assert_eq!(choices[1].deny.option_id.0.as_ref(), "always_deny:terminal");
    assert_eq!(choices[1].sub_patterns, vec!["^cargo\\s+build(\\s|$)"]);

    // Third choice is the one-time allow/deny
    assert_eq!(choices[2].allow.option_id.0.as_ref(), "allow");
    assert_eq!(choices[2].deny.option_id.0.as_ref(), "deny");
    assert!(choices[2].sub_patterns.is_empty());
}

#[test]
fn test_permission_options_terminal_pipeline_produces_dropdown_with_patterns() {
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["cargo test 2>&1 | tail".to_string()],
    )
    .build_permission_options();

    let PermissionOptions::DropdownWithPatterns {
        choices,
        patterns,
        tool_name,
    } = permission_options
    else {
        panic!("Expected DropdownWithPatterns permission options for pipeline command");
    };

    assert_eq!(tool_name, TerminalTool::NAME);

    // Should have "Always for terminal" and "Only this time" choices
    assert_eq!(choices.len(), 2);
    let labels: Vec<&str> = choices
        .iter()
        .map(|choice| choice.allow.name.as_ref())
        .collect();
    assert!(labels.contains(&"Always for terminal"));
    assert!(labels.contains(&"Only this time"));

    // Should have per-command patterns for "cargo test" and "tail"
    assert_eq!(patterns.len(), 2);
    let pattern_names: Vec<&str> = patterns.iter().map(|cp| cp.display_name.as_str()).collect();
    assert!(pattern_names.contains(&"cargo test"));
    assert!(pattern_names.contains(&"tail"));

    // Verify patterns are valid regex patterns
    let regex_patterns: Vec<&str> = patterns.iter().map(|cp| cp.pattern.as_str()).collect();
    assert!(regex_patterns.contains(&"^cargo\\s+test(\\s|$)"));
    assert!(regex_patterns.contains(&"^tail\\b"));
}

#[test]
fn test_permission_options_terminal_pipeline_with_chaining() {
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["npm install && npm test | tail".to_string()],
    )
    .build_permission_options();

    let PermissionOptions::DropdownWithPatterns { patterns, .. } = permission_options else {
        panic!("Expected DropdownWithPatterns for chained pipeline command");
    };

    // With subcommand-aware patterns, "npm install" and "npm test" are distinct
    assert_eq!(patterns.len(), 3);
    let pattern_names: Vec<&str> = patterns.iter().map(|cp| cp.display_name.as_str()).collect();
    assert!(pattern_names.contains(&"npm install"));
    assert!(pattern_names.contains(&"npm test"));
    assert!(pattern_names.contains(&"tail"));
}
