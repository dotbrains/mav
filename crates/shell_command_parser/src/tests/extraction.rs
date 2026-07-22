use super::*;

#[test]
fn test_simple_command() {
    let commands = extract_commands("ls").expect("parse failed");
    assert_eq!(commands, vec!["ls"]);
}

#[test]
fn test_command_with_args() {
    let commands = extract_commands("ls -la /tmp").expect("parse failed");
    assert_eq!(commands, vec!["ls -la /tmp"]);
}

#[test]
fn test_single_quoted_argument_is_normalized() {
    let commands = extract_commands("rm -rf '/'").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf /"]);
}

#[test]
fn test_single_quoted_command_name_is_normalized() {
    let commands = extract_commands("'rm' -rf /").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf /"]);
}

#[test]
fn test_double_quoted_argument_is_normalized() {
    let commands = extract_commands("rm -rf \"/\"").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf /"]);
}

#[test]
fn test_double_quoted_command_name_is_normalized() {
    let commands = extract_commands("\"rm\" -rf /").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf /"]);
}

#[test]
fn test_escaped_argument_is_normalized() {
    let commands = extract_commands("rm -rf \\/").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf /"]);
}

#[test]
fn test_partial_quoting_command_name_is_normalized() {
    let commands = extract_commands("r'm' -rf /").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf /"]);
}

#[test]
fn test_partial_quoting_flag_is_normalized() {
    let commands = extract_commands("rm -r'f' /").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf /"]);
}

#[test]
fn test_quoted_bypass_in_chained_command() {
    let commands = extract_commands("ls && 'rm' -rf '/'").expect("parse failed");
    assert_eq!(commands, vec!["ls", "rm -rf /"]);
}

#[test]
fn test_tilde_preserved_after_normalization() {
    let commands = extract_commands("rm -rf ~").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf ~"]);
}

#[test]
fn test_quoted_tilde_normalized() {
    let commands = extract_commands("rm -rf '~'").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf ~"]);
}

#[test]
fn test_parameter_expansion_preserved() {
    let commands = extract_commands("rm -rf $HOME").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf $HOME"]);
}

#[test]
fn test_braced_parameter_expansion_preserved() {
    let commands = extract_commands("rm -rf ${HOME}").expect("parse failed");
    assert_eq!(commands, vec!["rm -rf ${HOME}"]);
}

#[test]
fn test_and_operator() {
    let commands = extract_commands("ls && rm -rf /").expect("parse failed");
    assert_eq!(commands, vec!["ls", "rm -rf /"]);
}

#[test]
fn test_or_operator() {
    let commands = extract_commands("ls || rm -rf /").expect("parse failed");
    assert_eq!(commands, vec!["ls", "rm -rf /"]);
}

#[test]
fn test_semicolon() {
    let commands = extract_commands("ls; rm -rf /").expect("parse failed");
    assert_eq!(commands, vec!["ls", "rm -rf /"]);
}

#[test]
fn test_pipe() {
    let commands = extract_commands("ls | xargs rm -rf").expect("parse failed");
    assert_eq!(commands, vec!["ls", "xargs rm -rf"]);
}

#[test]
fn test_background() {
    let commands = extract_commands("ls & rm -rf /").expect("parse failed");
    assert_eq!(commands, vec!["ls", "rm -rf /"]);
}

#[test]
fn test_command_substitution_dollar() {
    let commands = extract_commands("echo $(whoami)").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.contains(&"whoami".to_string()));
}

#[test]
fn test_command_substitution_backticks() {
    let commands = extract_commands("echo `whoami`").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.contains(&"whoami".to_string()));
}

#[test]
fn test_process_substitution_input() {
    let commands = extract_commands("cat <(ls)").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("cat")));
    assert!(commands.contains(&"ls".to_string()));
}

#[test]
fn test_process_substitution_output() {
    let commands = extract_commands("ls >(cat)").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("ls")));
    assert!(commands.contains(&"cat".to_string()));
}

#[test]
fn test_newline_separator() {
    let commands = extract_commands("ls\nrm -rf /").expect("parse failed");
    assert_eq!(commands, vec!["ls", "rm -rf /"]);
}

#[test]
fn test_subshell() {
    let commands = extract_commands("(ls && rm -rf /)").expect("parse failed");
    assert_eq!(commands, vec!["ls", "rm -rf /"]);
}

#[test]
fn test_mixed_operators() {
    let commands = extract_commands("ls; echo hello && rm -rf /").expect("parse failed");
    assert_eq!(commands, vec!["ls", "echo hello", "rm -rf /"]);
}

#[test]
fn test_no_spaces_around_operators() {
    let commands = extract_commands("ls&&rm").expect("parse failed");
    assert_eq!(commands, vec!["ls", "rm"]);
}

#[test]
fn test_nested_command_substitution() {
    let commands = extract_commands("echo $(cat $(whoami).txt)").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("cat")));
    assert!(commands.contains(&"whoami".to_string()));
}

#[test]
fn test_empty_command() {
    let commands = extract_commands("").expect("parse failed");
    assert!(commands.is_empty());
}

#[test]
fn test_invalid_syntax_returns_none() {
    let result = extract_commands("ls &&");
    assert!(result.is_none());
}

#[test]
fn test_unparsable_nested_substitution_returns_none() {
    let result = extract_commands("echo $(ls &&)");
    assert!(result.is_none());
}

#[test]
fn test_unparsable_nested_backtick_substitution_returns_none() {
    let result = extract_commands("echo `ls &&`");
    assert!(result.is_none());
}
