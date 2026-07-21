use super::*;
use agent::{TerminalTool, ToolPermissionContext};

#[gpui::test]
async fn test_option_id_transformation_for_allow() {
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["cargo build --release".to_string()],
    )
    .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    let allow_ids: Vec<String> = choices
        .iter()
        .map(|choice| choice.allow.option_id.0.to_string())
        .collect();

    assert!(allow_ids.contains(&"allow".to_string()));
    assert_eq!(
        allow_ids
            .iter()
            .filter(|id| *id == "always_allow:terminal")
            .count(),
        2,
        "Expected two always_allow:terminal IDs (one whole-tool, one pattern with sub_patterns)"
    );
}

#[gpui::test]
async fn test_option_id_transformation_for_deny() {
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["cargo build --release".to_string()],
    )
    .build_permission_options();

    let PermissionOptions::Dropdown(choices) = permission_options else {
        panic!("Expected dropdown permission options");
    };

    let deny_ids: Vec<String> = choices
        .iter()
        .map(|choice| choice.deny.option_id.0.to_string())
        .collect();

    assert!(deny_ids.contains(&"deny".to_string()));
    assert_eq!(
        deny_ids
            .iter()
            .filter(|id| *id == "always_deny:terminal")
            .count(),
        2,
        "Expected two always_deny:terminal IDs (one whole-tool, one pattern with sub_patterns)"
    );
}

fn flat_allow_deny_options() -> PermissionOptions {
    PermissionOptions::Flat(vec![
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("allow"),
            "Yes",
            acp::PermissionOptionKind::AllowOnce,
        ),
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("deny"),
            "No",
            acp::PermissionOptionKind::RejectOnce,
        ),
    ])
}

fn sandbox_permission_options() -> PermissionOptions {
    PermissionOptions::Flat(vec![
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("allow"),
            "Allow once",
            acp::PermissionOptionKind::AllowOnce,
        ),
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("allow_thread"),
            "Allow for this thread",
            acp::PermissionOptionKind::AllowAlways,
        ),
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("allow_always"),
            "Allow always",
            acp::PermissionOptionKind::AllowAlways,
        ),
        acp::PermissionOption::new(
            acp::PermissionOptionId::new("deny"),
            "Deny",
            acp::PermissionOptionKind::RejectOnce,
        ),
    ])
}

#[test]
fn permission_option_for_action_prefers_explicit_sandbox_allow_always() {
    let options = sandbox_permission_options();

    let option =
        permission_option_for_action(&options, acp::PermissionOptionKind::AllowAlways).unwrap();

    assert_eq!(option.option_id.0.as_ref(), "allow_always");
}

#[test]
fn resolve_outcome_from_selection_flat_allow_picks_allow_once() {
    let options = flat_allow_deny_options();

    let outcome = resolve_outcome_from_selection(&options, None, true).unwrap();

    assert_eq!(outcome.option_id.0.as_ref(), "allow");
    assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
}

#[test]
fn resolve_outcome_from_selection_flat_deny_picks_reject_once() {
    let options = flat_allow_deny_options();

    let outcome = resolve_outcome_from_selection(&options, None, false).unwrap();

    assert_eq!(outcome.option_id.0.as_ref(), "deny");
    assert_eq!(outcome.option_kind, acp::PermissionOptionKind::RejectOnce);
}

#[test]
fn resolve_outcome_from_selection_flat_ignores_selection() {
    let options = flat_allow_deny_options();
    let selection = thread_view::PermissionSelection::Choice(42);

    let outcome = resolve_outcome_from_selection(&options, Some(&selection), true).unwrap();

    assert_eq!(outcome.option_id.0.as_ref(), "allow");
}

#[test]
fn resolve_outcome_from_selection_dropdown_defaults_to_last_choice_when_no_selection() {
    let options = ToolPermissionContext::new(TerminalTool::NAME, vec!["cargo build".to_string()])
        .build_permission_options();

    let outcome = resolve_outcome_from_selection(&options, None, true).unwrap();

    assert_eq!(outcome.option_id.0.as_ref(), "allow");
    assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
}

#[test]
fn resolve_outcome_from_selection_dropdown_uses_selected_choice() {
    let options = ToolPermissionContext::new(TerminalTool::NAME, vec!["cargo build".to_string()])
        .build_permission_options();
    let selection = thread_view::PermissionSelection::Choice(0);

    let outcome = resolve_outcome_from_selection(&options, Some(&selection), true).unwrap();

    assert!(outcome.option_id.0.contains("always_allow:terminal"));
    assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowAlways);
}

#[test]
fn resolve_outcome_from_selection_dropdown_out_of_range_falls_back_to_last() {
    let options = ToolPermissionContext::new(TerminalTool::NAME, vec!["cargo build".to_string()])
        .build_permission_options();
    let selection = thread_view::PermissionSelection::Choice(999);

    let outcome = resolve_outcome_from_selection(&options, Some(&selection), true).unwrap();

    assert_eq!(outcome.option_id.0.as_ref(), "allow");
}

#[test]
fn resolve_outcome_from_selection_pattern_mode_with_empty_checked_falls_back_to_last_choice() {
    let options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["cargo test 2>&1 | tail".to_string()],
    )
    .build_permission_options();
    assert!(matches!(
        options,
        PermissionOptions::DropdownWithPatterns { .. }
    ));
    let selection = thread_view::PermissionSelection::SelectedPatterns(vec![]);

    let outcome = resolve_outcome_from_selection(&options, Some(&selection), true).unwrap();

    assert_eq!(outcome.option_id.0.as_ref(), "allow");
    assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
}

#[test]
fn resolve_outcome_from_selection_pattern_mode_with_checked_uses_always_with_params() {
    let options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["cargo test 2>&1 | tail".to_string()],
    )
    .build_permission_options();
    assert!(matches!(
        options,
        PermissionOptions::DropdownWithPatterns { .. }
    ));
    let selection = thread_view::PermissionSelection::SelectedPatterns(vec![0]);

    let outcome = resolve_outcome_from_selection(&options, Some(&selection), true).unwrap();

    assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowAlways);
    assert!(
        outcome.params.is_some(),
        "checked patterns should attach terminal params"
    );
}
