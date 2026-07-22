use super::*;

fn test_scalar_env_var_prefix_included_in_extracted_command() {
    let commands = extract_commands("PAGER=blah git status").expect("parse failed");
    assert_eq!(commands, vec!["PAGER=blah git status"]);
}

#[test]
fn test_multiple_scalar_assignments_preserved_in_order() {
    let commands = extract_commands("A=1 B=2 git log").expect("parse failed");
    assert_eq!(commands, vec!["A=1 B=2 git log"]);
}

#[test]
fn test_assignment_quoting_dropped_when_safe() {
    let commands = extract_commands("PAGER='curl' git log").expect("parse failed");
    assert_eq!(commands, vec!["PAGER=curl git log"]);
}

#[test]
fn test_assignment_quoting_preserved_for_whitespace() {
    let commands = extract_commands("PAGER='less -R' git log").expect("parse failed");
    assert_eq!(commands, vec!["PAGER='less -R' git log"]);
}

#[test]
fn test_assignment_quoting_preserved_for_semicolon() {
    let commands = extract_commands("PAGER='a;b' git log").expect("parse failed");
    assert_eq!(commands, vec!["PAGER='a;b' git log"]);
}

#[test]
fn test_array_assignments_ignored_for_prefix_matching_output() {
    let commands = extract_commands("FOO=(a b) git status").expect("parse failed");
    assert_eq!(commands, vec!["git status"]);
}

#[test]
fn test_extract_terminal_command_prefix_includes_env_var_prefix_and_subcommand() {
    let prefix = extract_terminal_command_prefix("PAGER=blah git log --oneline")
        .expect("expected terminal command prefix");

    assert_eq!(
        prefix,
        TerminalCommandPrefix {
            normalized: "PAGER=blah git log".to_string(),
            display: "PAGER=blah git log".to_string(),
            tokens: vec![
                "PAGER=blah".to_string(),
                "git".to_string(),
                "log".to_string(),
            ],
            command: "git".to_string(),
            subcommand: Some("log".to_string()),
        }
    );
}

#[test]
fn test_extract_terminal_command_prefix_preserves_required_assignment_quotes_in_display_and_normalized()
 {
    let prefix = extract_terminal_command_prefix("PAGER='less -R' git log")
        .expect("expected terminal command prefix");

    assert_eq!(
        prefix,
        TerminalCommandPrefix {
            normalized: "PAGER='less -R' git log".to_string(),
            display: "PAGER='less -R' git log".to_string(),
            tokens: vec![
                "PAGER='less -R'".to_string(),
                "git".to_string(),
                "log".to_string(),
            ],
            command: "git".to_string(),
            subcommand: Some("log".to_string()),
        }
    );
}

#[test]
fn test_extract_terminal_command_prefix_skips_redirects_before_subcommand() {
    let prefix = extract_terminal_command_prefix("git 2>/dev/null log --oneline")
        .expect("expected terminal command prefix");

    assert_eq!(
        prefix,
        TerminalCommandPrefix {
            normalized: "git log".to_string(),
            display: "git 2>/dev/null log".to_string(),
            tokens: vec!["git".to_string(), "log".to_string()],
            command: "git".to_string(),
            subcommand: Some("log".to_string()),
        }
    );
}
