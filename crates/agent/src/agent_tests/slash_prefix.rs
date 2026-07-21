#[test]
fn test_strip_slash_command_prefix_keeps_inline_args() {
    // The bug being guarded against: skill slash invocation used to
    // discard the entire first text block, which threw away anything
    // the user typed on the same line as the command.
    assert_eq!(
        strip_slash_command_prefix("/fix-review #1, #2, #3"),
        "#1, #2, #3",
    );
}

#[test]
fn test_strip_slash_command_prefix_preserves_newlines() {
    // Continuations across newlines are common when users compose
    // structured prompts; the first newline is the command terminator,
    // but everything after it must reach the model verbatim.
    assert_eq!(
        strip_slash_command_prefix("/fix-review\nline 1\nline 2"),
        "line 1\nline 2",
    );
}

#[test]
fn test_strip_slash_command_prefix_command_only_is_empty() {
    assert_eq!(strip_slash_command_prefix("/fix-review"), "");
    assert_eq!(strip_slash_command_prefix("/fix-review "), "");
}

#[test]
fn test_strip_slash_command_prefix_ignores_leading_whitespace() {
    assert_eq!(strip_slash_command_prefix("   /fix-review hello"), "hello",);
}

#[test]
fn test_strip_slash_command_prefix_passes_through_non_command_text() {
    // Defense in depth: if somehow we're called with a non-slash-prefixed
    // block, the safe behavior is to return it unchanged rather than
    // silently mangling unrelated user text.
    assert_eq!(strip_slash_command_prefix("hello world"), "hello world",);
}
