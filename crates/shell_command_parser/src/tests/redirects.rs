use super::*;

fn test_redirect_write_includes_target_path() {
    let commands = extract_commands("echo hello > /etc/passwd").expect("parse failed");
    assert_eq!(commands, vec!["echo hello", "> /etc/passwd"]);
}

#[test]
fn test_redirect_append_includes_target_path() {
    let commands = extract_commands("cat file >> /tmp/log").expect("parse failed");
    assert_eq!(commands, vec!["cat file", ">> /tmp/log"]);
}

#[test]
fn test_fd_redirect_handled_gracefully() {
    let commands = extract_commands("cmd 2>&1").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_input_redirect() {
    let commands = extract_commands("sort < /tmp/input").expect("parse failed");
    assert_eq!(commands, vec!["sort", "< /tmp/input"]);
}

#[test]
fn test_multiple_redirects() {
    let commands = extract_commands("cmd > /tmp/out 2> /tmp/err").expect("parse failed");
    assert_eq!(commands, vec!["cmd", "> /tmp/out", "2> /tmp/err"]);
}

#[test]
fn test_prefix_position_redirect() {
    let commands = extract_commands("> /tmp/out echo hello").expect("parse failed");
    assert_eq!(commands, vec!["echo hello", "> /tmp/out"]);
}

#[test]
fn test_redirect_with_variable_expansion() {
    let commands = extract_commands("echo > $HOME/file").expect("parse failed");
    assert_eq!(commands, vec!["echo", "> $HOME/file"]);
}

#[test]
fn test_output_and_error_redirect() {
    let commands = extract_commands("cmd &> /tmp/all").expect("parse failed");
    assert_eq!(commands, vec!["cmd", "&> /tmp/all"]);
}

#[test]
fn test_append_output_and_error_redirect() {
    let commands = extract_commands("cmd &>> /tmp/all").expect("parse failed");
    assert_eq!(commands, vec!["cmd", "&>> /tmp/all"]);
}

#[test]
fn test_redirect_in_chained_command() {
    let commands = extract_commands("echo hello > /tmp/out && cat /tmp/out").expect("parse failed");
    assert_eq!(commands, vec!["echo hello", "> /tmp/out", "cat /tmp/out"]);
}

#[test]
fn test_here_string_dropped_from_normalized_output() {
    let commands = extract_commands("cat <<< 'hello'").expect("parse failed");
    assert_eq!(commands, vec!["cat"]);
}

#[test]
fn test_brace_group_redirect() {
    let commands = extract_commands("{ echo hello; } > /etc/passwd").expect("parse failed");
    assert_eq!(commands, vec!["echo hello", "> /etc/passwd"]);
}

#[test]
fn test_subshell_redirect() {
    let commands = extract_commands("(cmd) > /etc/passwd").expect("parse failed");
    assert_eq!(commands, vec!["cmd", "> /etc/passwd"]);
}

#[test]
fn test_for_loop_redirect() {
    let commands =
        extract_commands("for f in *; do cat \"$f\"; done > /tmp/out").expect("parse failed");
    assert_eq!(commands, vec!["cat $f", "> /tmp/out"]);
}

#[test]
fn test_brace_group_multi_command_redirect() {
    let commands = extract_commands("{ echo hello; cat; } > /etc/passwd").expect("parse failed");
    assert_eq!(commands, vec!["echo hello", "cat", "> /etc/passwd"]);
}

#[test]
fn test_quoted_redirect_target_is_normalized() {
    let commands = extract_commands("echo hello > '/etc/passwd'").expect("parse failed");
    assert_eq!(commands, vec!["echo hello", "> /etc/passwd"]);
}

#[test]
fn test_redirect_without_space() {
    let commands = extract_commands("echo hello >/etc/passwd").expect("parse failed");
    assert_eq!(commands, vec!["echo hello", "> /etc/passwd"]);
}

#[test]
fn test_clobber_redirect() {
    let commands = extract_commands("cmd >| /tmp/file").expect("parse failed");
    assert_eq!(commands, vec!["cmd", ">| /tmp/file"]);
}

#[test]
fn test_fd_to_fd_redirect_skipped() {
    let commands = extract_commands("cmd 1>&2").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_bare_redirect_returns_none() {
    let result = extract_commands("> /etc/passwd");
    assert!(result.is_none());
}

#[test]
fn test_arithmetic_with_redirect_returns_none() {
    let result = extract_commands("(( x = 1 )) > /tmp/file");
    assert!(result.is_none());
}

#[test]
fn test_redirect_target_with_command_substitution() {
    let commands = extract_commands("echo > $(mktemp)").expect("parse failed");
    assert_eq!(commands, vec!["echo", "> $(mktemp)", "mktemp"]);
}

#[test]
fn test_nested_compound_redirects() {
    let commands = extract_commands("{ echo > /tmp/a; } > /tmp/b").expect("parse failed");
    assert_eq!(commands, vec!["echo", "> /tmp/a", "> /tmp/b"]);
}

#[test]
fn test_while_loop_redirect() {
    let commands =
        extract_commands("while true; do echo line; done > /tmp/log").expect("parse failed");
    assert_eq!(commands, vec!["true", "echo line", "> /tmp/log"]);
}

#[test]
fn test_if_clause_redirect() {
    let commands = extract_commands("if true; then echo yes; fi > /tmp/out").expect("parse failed");
    assert_eq!(commands, vec!["true", "echo yes", "> /tmp/out"]);
}

#[test]
fn test_pipe_with_redirect_on_last_command() {
    let commands = extract_commands("ls | grep foo > /tmp/out").expect("parse failed");
    assert_eq!(commands, vec!["ls", "grep foo", "> /tmp/out"]);
}

#[test]
fn test_pipe_with_stderr_redirect_on_first_command() {
    let commands = extract_commands("ls 2>/dev/null | grep foo").expect("parse failed");
    assert_eq!(commands, vec!["ls", "grep foo"]);
}

#[test]
fn test_function_definition_redirect() {
    let commands = extract_commands("f() { echo hi; } > /tmp/out").expect("parse failed");
    assert_eq!(commands, vec!["echo hi", "> /tmp/out"]);
}

#[test]
fn test_read_and_write_redirect() {
    let commands = extract_commands("cmd <> /dev/tty").expect("parse failed");
    assert_eq!(commands, vec!["cmd", "<> /dev/tty"]);
}

#[test]
fn test_case_clause_with_redirect() {
    let commands =
        extract_commands("case $x in a) echo hi;; esac > /tmp/out").expect("parse failed");
    assert_eq!(commands, vec!["echo hi", "> /tmp/out"]);
}

#[test]
fn test_until_loop_with_redirect() {
    let commands =
        extract_commands("until false; do echo line; done > /tmp/log").expect("parse failed");
    assert_eq!(commands, vec!["false", "echo line", "> /tmp/log"]);
}

#[test]
fn test_arithmetic_for_clause_with_redirect() {
    let commands = extract_commands("for ((i=0; i<10; i++)); do echo $i; done > /tmp/out")
        .expect("parse failed");
    assert_eq!(commands, vec!["echo $i", "> /tmp/out"]);
}

#[test]
fn test_if_elif_else_with_redirect() {
    let commands = extract_commands(
        "if true; then echo a; elif false; then echo b; else echo c; fi > /tmp/out",
    )
    .expect("parse failed");
    assert_eq!(
        commands,
        vec!["true", "echo a", "false", "echo b", "echo c", "> /tmp/out"]
    );
}

#[test]
fn test_multiple_redirects_on_compound_command() {
    let commands = extract_commands("{ cmd; } > /tmp/out 2> /tmp/err").expect("parse failed");
    assert_eq!(commands, vec!["cmd", "> /tmp/out", "2> /tmp/err"]);
}

#[test]
fn test_here_document_command_substitution_extracted() {
    let commands = extract_commands("cat <<EOF\n$(rm -rf /)\nEOF").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("cat")));
    assert!(commands.contains(&"rm -rf /".to_string()));
}

#[test]
fn test_here_document_quoted_delimiter_no_extraction() {
    let commands = extract_commands("cat <<'EOF'\n$(rm -rf /)\nEOF").expect("parse failed");
    assert_eq!(commands, vec!["cat"]);
}

#[test]
fn test_here_document_backtick_substitution_extracted() {
    let commands = extract_commands("cat <<EOF\n`whoami`\nEOF").expect("parse failed");
    assert!(commands.iter().any(|c| c.contains("cat")));
    assert!(commands.contains(&"whoami".to_string()));
}

#[test]
fn test_brace_group_redirect_with_command_substitution() {
    let commands = extract_commands("{ echo hello; } > $(mktemp)").expect("parse failed");
    assert!(commands.contains(&"echo hello".to_string()));
    assert!(commands.contains(&"mktemp".to_string()));
}

#[test]
fn test_function_definition_redirect_with_command_substitution() {
    let commands = extract_commands("f() { echo hi; } > $(mktemp)").expect("parse failed");
    assert!(commands.contains(&"echo hi".to_string()));
    assert!(commands.contains(&"mktemp".to_string()));
}

#[test]
fn test_brace_group_redirect_with_process_substitution() {
    let commands = extract_commands("{ cat; } > >(tee /tmp/log)").expect("parse failed");
    assert!(commands.contains(&"cat".to_string()));
    assert!(commands.contains(&"tee /tmp/log".to_string()));
}

#[test]
fn test_redirect_to_dev_null_skipped() {
    let commands = extract_commands("cmd > /dev/null").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_stderr_redirect_to_dev_null_skipped() {
    let commands = extract_commands("cmd 2>/dev/null").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_stderr_redirect_to_dev_null_with_space_skipped() {
    let commands = extract_commands("cmd 2> /dev/null").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_append_redirect_to_dev_null_skipped() {
    let commands = extract_commands("cmd >> /dev/null").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_output_and_error_redirect_to_dev_null_skipped() {
    let commands = extract_commands("cmd &>/dev/null").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_append_output_and_error_redirect_to_dev_null_skipped() {
    let commands = extract_commands("cmd &>>/dev/null").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_quoted_dev_null_redirect_skipped() {
    let commands = extract_commands("cmd 2>'/dev/null'").expect("parse failed");
    assert_eq!(commands, vec!["cmd"]);
}

#[test]
fn test_redirect_to_real_file_still_included() {
    let commands = extract_commands("echo hello > /etc/passwd").expect("parse failed");
    assert_eq!(commands, vec!["echo hello", "> /etc/passwd"]);
}

#[test]
fn test_dev_null_redirect_in_chained_command() {
    let commands = extract_commands("git log 2>/dev/null || echo fallback").expect("parse failed");
    assert_eq!(commands, vec!["git log", "echo fallback"]);
}

#[test]
fn test_mixed_safe_and_unsafe_redirects() {
    let commands = extract_commands("cmd > /tmp/out 2>/dev/null").expect("parse failed");
    assert_eq!(commands, vec!["cmd", "> /tmp/out"]);
}
