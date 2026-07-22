use crate::CompiledRegex;

#[test]
fn test_compiled_regex_case_insensitive() {
    let regex = CompiledRegex::new("rm\\s+-rf", false).unwrap();
    assert!(regex.is_match("rm -rf /"));
    assert!(regex.is_match("RM -RF /"));
    assert!(regex.is_match("Rm -Rf /"));
}

#[test]
fn test_compiled_regex_case_sensitive() {
    let regex = CompiledRegex::new("DROP\\s+TABLE", true).unwrap();
    assert!(regex.is_match("DROP TABLE users"));
    assert!(!regex.is_match("drop table users"));
}

#[test]
fn test_invalid_regex_returns_none() {
    let result = CompiledRegex::new("[invalid(regex", false);
    assert!(result.is_none());
}
