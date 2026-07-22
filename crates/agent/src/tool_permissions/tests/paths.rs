use super::*;

#[test]
fn normalize_path_relative_no_change() {
    assert_eq!(normalize_path("foo/bar"), "foo/bar");
}

#[test]
fn normalize_path_relative_with_curdir() {
    assert_eq!(normalize_path("foo/./bar"), "foo/bar");
}

#[test]
fn normalize_path_relative_with_parent() {
    assert_eq!(normalize_path("foo/bar/../baz"), "foo/baz");
}

#[test]
fn normalize_path_absolute_preserved() {
    assert_eq!(normalize_path("/etc/passwd"), "/etc/passwd");
}

#[test]
fn normalize_path_absolute_with_traversal() {
    assert_eq!(normalize_path("/tmp/../etc/passwd"), "/etc/passwd");
}

#[test]
fn normalize_path_root() {
    assert_eq!(normalize_path("/"), "/");
}

#[test]
fn normalize_path_parent_beyond_root_clamped() {
    assert_eq!(normalize_path("/../../../etc/passwd"), "/etc/passwd");
}

#[test]
fn normalize_path_curdir_only() {
    assert_eq!(normalize_path("."), "");
}

#[test]
fn normalize_path_empty() {
    assert_eq!(normalize_path(""), "");
}

#[test]
fn normalize_path_relative_traversal_above_start() {
    assert_eq!(normalize_path("../../../etc/passwd"), "../../../etc/passwd");
}

#[test]
fn normalize_path_relative_traversal_with_curdir() {
    assert_eq!(normalize_path("../../."), "../..");
}

#[test]
fn normalize_path_relative_partial_traversal_above_start() {
    assert_eq!(normalize_path("foo/../../bar"), "../bar");
}

#[test]
fn most_restrictive_deny_vs_allow() {
    assert!(matches!(
        most_restrictive(
            ToolPermissionDecision::Deny("x".into()),
            ToolPermissionDecision::Allow
        ),
        ToolPermissionDecision::Deny(_)
    ));
}

#[test]
fn most_restrictive_allow_vs_deny() {
    assert!(matches!(
        most_restrictive(
            ToolPermissionDecision::Allow,
            ToolPermissionDecision::Deny("x".into())
        ),
        ToolPermissionDecision::Deny(_)
    ));
}

#[test]
fn most_restrictive_deny_vs_confirm() {
    assert!(matches!(
        most_restrictive(
            ToolPermissionDecision::Deny("x".into()),
            ToolPermissionDecision::Confirm
        ),
        ToolPermissionDecision::Deny(_)
    ));
}

#[test]
fn most_restrictive_confirm_vs_deny() {
    assert!(matches!(
        most_restrictive(
            ToolPermissionDecision::Confirm,
            ToolPermissionDecision::Deny("x".into())
        ),
        ToolPermissionDecision::Deny(_)
    ));
}

#[test]
fn most_restrictive_deny_vs_deny() {
    assert!(matches!(
        most_restrictive(
            ToolPermissionDecision::Deny("a".into()),
            ToolPermissionDecision::Deny("b".into())
        ),
        ToolPermissionDecision::Deny(_)
    ));
}

#[test]
fn most_restrictive_confirm_vs_allow() {
    assert_eq!(
        most_restrictive(
            ToolPermissionDecision::Confirm,
            ToolPermissionDecision::Allow
        ),
        ToolPermissionDecision::Confirm
    );
}

#[test]
fn most_restrictive_allow_vs_confirm() {
    assert_eq!(
        most_restrictive(
            ToolPermissionDecision::Allow,
            ToolPermissionDecision::Confirm
        ),
        ToolPermissionDecision::Confirm
    );
}

#[test]
fn most_restrictive_allow_vs_allow() {
    assert_eq!(
        most_restrictive(ToolPermissionDecision::Allow, ToolPermissionDecision::Allow),
        ToolPermissionDecision::Allow
    );
}

#[test]
fn decide_permission_for_path_no_dots_early_return() {
    // When the path has no `.` or `..`, normalize_path returns the same string,
    // so decide_permission_for_path returns the raw decision directly.
    let settings = test_agent_settings(ToolPermissions {
        default: ToolPermissionMode::Confirm,
        tools: Default::default(),
    });
    let decision = decide_permission_for_path(EditFileTool::NAME, "src/main.rs", &settings);
    assert_eq!(decision, ToolPermissionDecision::Confirm);
}

#[test]
fn decide_permission_for_path_traversal_triggers_deny() {
    let deny_regex = CompiledRegex::new("/etc/passwd", false).unwrap();
    let mut tools = collections::HashMap::default();
    tools.insert(
        Arc::from(EditFileTool::NAME),
        ToolRules {
            default: Some(ToolPermissionMode::Allow),
            always_allow: vec![],
            always_deny: vec![deny_regex],
            always_confirm: vec![],
            invalid_patterns: vec![],
        },
    );
    let settings = test_agent_settings(ToolPermissions {
        default: ToolPermissionMode::Confirm,
        tools,
    });

    let decision = decide_permission_for_path(EditFileTool::NAME, "/tmp/../etc/passwd", &settings);
    assert!(
        matches!(decision, ToolPermissionDecision::Deny(_)),
        "expected Deny for traversal to /etc/passwd, got {:?}",
        decision
    );
}

#[test]
fn normalize_path_collapses_dot_segments() {
    assert_eq!(
        normalize_path("src/../.mav/settings.json"),
        ".mav/settings.json"
    );
    assert_eq!(normalize_path("a/b/../c"), "a/c");
    assert_eq!(normalize_path("a/./b/c"), "a/b/c");
    assert_eq!(normalize_path("a/b/./c/../d"), "a/b/d");
    assert_eq!(normalize_path(".mav/settings.json"), ".mav/settings.json");
    assert_eq!(normalize_path("a/b/c"), "a/b/c");
}

#[test]
fn normalize_path_handles_multiple_parent_dirs() {
    assert_eq!(normalize_path("a/b/c/../../d"), "a/d");
    assert_eq!(normalize_path("a/b/c/../../../d"), "d");
}

fn path_perm(
    tool: &str,
    input: &str,
    deny: &[&str],
    allow: &[&str],
    confirm: &[&str],
) -> ToolPermissionDecision {
    let mut tools = collections::HashMap::default();
    tools.insert(
        Arc::from(tool),
        ToolRules {
            default: None,
            always_allow: allow
                .iter()
                .map(|p| {
                    CompiledRegex::new(p, false).unwrap_or_else(|| panic!("invalid regex: {p:?}"))
                })
                .collect(),
            always_deny: deny
                .iter()
                .map(|p| {
                    CompiledRegex::new(p, false).unwrap_or_else(|| panic!("invalid regex: {p:?}"))
                })
                .collect(),
            always_confirm: confirm
                .iter()
                .map(|p| {
                    CompiledRegex::new(p, false).unwrap_or_else(|| panic!("invalid regex: {p:?}"))
                })
                .collect(),
            invalid_patterns: vec![],
        },
    );
    let permissions = ToolPermissions {
        default: ToolPermissionMode::Confirm,
        tools,
    };
    let raw_decision = ToolPermissionDecision::from_input(
        tool,
        &[input.to_string()],
        &permissions,
        ShellKind::Posix,
    );

    let simplified = normalize_path(input);
    if simplified == input {
        return raw_decision;
    }

    let simplified_decision =
        ToolPermissionDecision::from_input(tool, &[simplified], &permissions, ShellKind::Posix);

    most_restrictive(raw_decision, simplified_decision)
}

#[test]
fn decide_permission_for_path_denies_traversal_to_denied_dir() {
    let decision = path_perm(
        "copy_path",
        "src/../.mav/settings.json",
        &["^\\.mav/"],
        &[],
        &[],
    );
    assert!(matches!(decision, ToolPermissionDecision::Deny(_)));
}

#[test]
fn decide_permission_for_path_confirms_traversal_to_confirmed_dir() {
    let decision = path_perm(
        "copy_path",
        "src/../.mav/settings.json",
        &[],
        &[],
        &["^\\.mav/"],
    );
    assert!(matches!(decision, ToolPermissionDecision::Confirm));
}

#[test]
fn decide_permission_for_path_allows_when_no_traversal_issue() {
    let decision = path_perm("copy_path", "src/main.rs", &[], &["^src/"], &[]);
    assert!(matches!(decision, ToolPermissionDecision::Allow));
}

#[test]
fn decide_permission_for_path_most_restrictive_wins() {
    let decision = path_perm(
        "copy_path",
        "allowed/../.mav/settings.json",
        &["^\\.mav/"],
        &["^allowed/"],
        &[],
    );
    assert!(matches!(decision, ToolPermissionDecision::Deny(_)));
}

#[test]
fn decide_permission_for_path_dot_segment_only() {
    let decision = path_perm(
        "delete_path",
        "./.mav/settings.json",
        &["^\\.mav/"],
        &[],
        &[],
    );
    assert!(matches!(decision, ToolPermissionDecision::Deny(_)));
}

#[test]
fn decide_permission_for_path_no_change_when_already_simple() {
    // When path has no `.` or `..` segments, behavior matches decide_permission_from_settings
    let decision = path_perm("copy_path", ".mav/settings.json", &["^\\.mav/"], &[], &[]);
    assert!(matches!(decision, ToolPermissionDecision::Deny(_)));
}

#[test]
fn decide_permission_for_path_raw_deny_still_works() {
    // Even without traversal, if the raw path itself matches deny, it's denied
    let decision = path_perm("copy_path", "secret/file.txt", &["^secret/"], &[], &[]);
    assert!(matches!(decision, ToolPermissionDecision::Deny(_)));
}

#[test]
fn decide_permission_for_path_denies_edit_file_traversal_to_dotenv() {
    let decision = path_perm(EditFileTool::NAME, "src/../.env", &["^\\.env"], &[], &[]);
    assert!(matches!(decision, ToolPermissionDecision::Deny(_)));
}
