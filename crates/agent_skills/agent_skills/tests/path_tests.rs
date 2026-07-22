use super::*;

#[test]
fn is_agents_skills_path_simple_positive() {
    assert!(is_agents_skills_path(Path::new(
        "foo/.agents/skills/my-skill/SKILL.md"
    )));
}

#[test]
fn is_agents_skills_path_simple_negative() {
    assert!(!is_agents_skills_path(Path::new("foo/bar/baz")));
}

#[test]
fn is_agents_skills_path_double_agents() {
    // `foo/.agents/.agents/skills` contains a `.agents/skills` pair at
    // depths 2-3. Any-depth matching catches it; this is intentional, so
    // a `.agents/skills` directory the user wasn't expecting to be
    // touched still prompts for confirmation.
    assert!(is_agents_skills_path(Path::new(
        "foo/.agents/.agents/skills"
    )));
}

#[test]
fn is_agents_skills_path_agents_without_skills() {
    assert!(!is_agents_skills_path(Path::new("foo/.agents/other")));
}

#[test]
fn is_agents_skills_path_at_start() {
    assert!(is_agents_skills_path(Path::new(".agents/skills")));
}

#[test]
fn is_agents_skills_path_trailing_agents() {
    assert!(!is_agents_skills_path(Path::new("foo/.agents")));
}

#[test]
fn is_agents_skills_path_deep_match() {
    // Any-depth matching: nested `.agents/skills` directories — e.g.
    // inside vendored sources — are flagged too. We prefer the extra
    // prompt over silently letting the agent edit something named
    // `.agents/skills`.
    assert!(is_agents_skills_path(Path::new("a/b/.agents/skills/x.txt")));
    assert!(is_agents_skills_path(Path::new(
        "some/random/place/.agents/skills/foo"
    )));
}

#[test]
fn is_agents_skills_path_absolute() {
    // Absolute paths into a project-local `.agents/skills/` are caught
    // by the same consecutive-component match.
    assert!(is_agents_skills_path(Path::new(
        "/Users/foo/project/.agents/skills/my-skill/SKILL.md"
    )));
    assert!(!is_agents_skills_path(Path::new("/etc/hosts")));
}

#[test]
fn is_agents_skills_path_case_insensitive() {
    // Filesystems on macOS/Windows are case-insensitive by default; the
    // classifier must agree.
    assert!(is_agents_skills_path(Path::new(".AGENTS/skills/foo")));
    assert!(is_agents_skills_path(Path::new(".agents/SKILLS/foo")));
    assert!(is_agents_skills_path(Path::new(
        "project/.AGENTS/SKILLS/foo"
    )));
}

#[test]
fn validate_name_accepts_valid_names() {
    assert!(validate_name("draft-pr").is_ok());
    assert!(validate_name("a").is_ok());
    assert!(validate_name("skill1").is_ok());
    assert!(validate_name(&"a".repeat(MAX_SKILL_NAME_LEN)).is_ok());
}

#[test]
fn validate_name_rejects_empty() {
    assert!(validate_name("").is_err());
}

#[test]
fn validate_name_rejects_uppercase() {
    assert!(validate_name("Draft-PR").is_err());
}

#[test]
fn validate_name_rejects_leading_and_trailing_hyphens() {
    assert!(validate_name("-draft").is_err());
    assert!(validate_name("draft-").is_err());
}

#[test]
fn validate_name_rejects_invalid_chars() {
    assert!(validate_name("draft_pr").is_err());
    assert!(validate_name("draft pr").is_err());
    assert!(validate_name("draft.pr").is_err());
}

#[test]
fn validate_name_rejects_too_long() {
    assert!(validate_name(&"a".repeat(MAX_SKILL_NAME_LEN + 1)).is_err());
}

#[test]
fn validate_description_accepts_valid() {
    assert!(validate_description("A useful skill").is_ok());
}

#[test]
fn validate_description_rejects_empty_and_whitespace_only() {
    assert!(validate_description("").is_err());
    assert!(validate_description("   ").is_err());
    assert!(validate_description("\t\n ").is_err());
}

#[test]
fn validate_description_rejects_too_long() {
    assert!(validate_description(&"a".repeat(MAX_SKILL_DESCRIPTION_LEN + 1)).is_err());
}

#[test]
fn validate_description_length_is_measured_in_bytes() {
    // "é" is 2 bytes in UTF-8. A string of MAX/2 + 1 "é" characters has
    // only ~MAX/2 + 1 chars but exceeds MAX bytes, so it must be
    // rejected by a byte-based validator (and accepted by a char-based
    // one). This regression-tests the byte semantics that strict
    // validation and load-time warnings both rely on.
    let chars = MAX_SKILL_DESCRIPTION_LEN / 2 + 1;
    let description = "é".repeat(chars);
    assert!(description.chars().count() <= MAX_SKILL_DESCRIPTION_LEN);
    assert!(description.len() > MAX_SKILL_DESCRIPTION_LEN);
    assert!(validate_description(&description).is_err());
}
