use super::*;

fn test_parse_valid_skill() {
    let content = r#"---
name: my-skill
description: A test skill for testing purposes
---

# My Skill

## Instructions
Do the thing.
"#;

    let result = parse_skill_frontmatter(
        Path::new("/skills/my-skill/SKILL.md"),
        content,
        SkillSource::Global,
    );
    let skill = result.expect("Should parse successfully");

    assert_eq!(skill.name, "my-skill");
    assert_eq!(skill.description, "A test skill for testing purposes");
    assert_eq!(skill.directory_path, Path::new("/skills/my-skill"));
    // Default: skill is invocable by both model and user.
    assert!(!skill.disable_model_invocation);
}

#[test]
fn test_parse_skill_file_content_returns_body() {
    let content = r#"---
name: my-skill
description: A test skill for testing purposes
---

# My Skill

Do the thing.
"#;

    let (metadata, body) =
        parse_skill_file_content(content).expect("valid skill content should parse successfully");

    assert_eq!(metadata.name, "my-skill");
    assert_eq!(metadata.description, "A test skill for testing purposes");
    assert_eq!(body.trim(), "# My Skill\n\nDo the thing.");
}

#[test]
fn test_parse_disable_model_invocation_true() {
    let content = r#"---
name: deploy
description: Deploy the application to production.
disable-model-invocation: true
---

Steps to deploy.
"#;

    let skill = parse_skill_frontmatter(
        Path::new("/skills/deploy/SKILL.md"),
        content,
        SkillSource::Global,
    )
    .expect("should parse");
    assert!(skill.disable_model_invocation);
}

#[test]
fn test_parse_disable_model_invocation_explicit_false() {
    let content = r#"---
name: helper
description: A helper skill.
disable-model-invocation: false
---

Help.
"#;

    let skill = parse_skill_frontmatter(
        Path::new("/skills/helper/SKILL.md"),
        content,
        SkillSource::Global,
    )
    .expect("should parse");
    assert!(!skill.disable_model_invocation);
}

#[test]
fn test_parse_missing_frontmatter() {
    let content = "# My Skill\n\nNo frontmatter here.";

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("must start with YAML frontmatter")
    );
}

#[test]
fn test_parse_missing_closing_delimiter() {
    let content = r#"---
name: test
description: Test
# No closing delimiter
"#;

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("missing closing frontmatter delimiter")
    );
}

#[test]
fn test_parse_empty_frontmatter_closing_on_next_line() {
    // An empty frontmatter (closer immediately after the opener) is a real
    // authoring case. Parsing should ultimately fail because the empty YAML
    // doc lacks `name` and `description`, but the error must be the proper
    // YAML/missing-field error rather than "missing closing frontmatter
    // delimiter" — the closer is right there.
    let content = "---\n---\nbody\n";

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_chain = format!("{:?}", err);
    assert!(
        !err_chain.contains("missing closing frontmatter delimiter"),
        "Error should NOT be the missing-closer error since the closer is present: {}",
        err_chain
    );
    assert!(
        err_chain.contains("missing field")
            || err_chain.contains("name")
            || err_chain.contains("description")
            || err_chain.contains("Invalid YAML"),
        "Error should mention missing name/description field or invalid YAML: {}",
        err_chain
    );
}

#[test]
fn test_parse_missing_name() {
    let content = r#"---
description: A test skill
---

Content here.
"#;

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_chain = format!("{:?}", err);
    assert!(
        err_chain.contains("missing field")
            || err_chain.contains("name")
            || err_chain.contains("Invalid YAML"),
        "Error should mention missing name field or invalid YAML: {}",
        err_chain
    );
}

#[test]
fn test_parse_missing_description() {
    let content = r#"---
name: test-skill
---

Content here.
"#;

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_chain = format!("{:?}", err);
    assert!(
        err_chain.contains("missing field")
            || err_chain.contains("description")
            || err_chain.contains("Invalid YAML"),
        "Error should mention missing description field or invalid YAML: {}",
        err_chain
    );
}

#[test]
fn test_parse_name_too_long() {
    let long_name = "a".repeat(65);
    let content = format!(
        r#"---
name: {long_name}
description: Test
---

Content.
"#
    );

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        &content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    let expected = format!("at most {MAX_SKILL_NAME_LEN} characters");
    assert!(result.unwrap_err().to_string().contains(&expected));
}

#[test]
fn test_parse_name_invalid_chars() {
    let content = r#"---
name: My_Skill
description: Test
---

Content.
"#;

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("lowercase letters, numbers, and hyphens")
    );
}
