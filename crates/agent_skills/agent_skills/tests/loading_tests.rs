use super::*;

#[test]
fn test_parse_description_too_long_loads_with_warning() {
    let long_desc = "a".repeat(MAX_SKILL_DESCRIPTION_LEN + 1);
    let content = format!(
        r#"---
name: test
description: {long_desc}
---

Content.
"#
    );

    let skill = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        &content,
        SkillSource::Global,
    )
    .expect("long descriptions should load with a warning");

    assert_eq!(skill.description, long_desc);
    assert_eq!(skill.load_warnings.len(), 1);
    assert_eq!(
        skill.load_warnings[0],
        SkillLoadWarning::DescriptionTooLong {
            actual_len: MAX_SKILL_DESCRIPTION_LEN + 1,
            max_len: MAX_SKILL_DESCRIPTION_LEN,
        }
    );
}

#[test]
fn test_parse_skill_file_content_rejects_description_too_long() {
    let long_desc = "a".repeat(MAX_SKILL_DESCRIPTION_LEN + 1);
    let content = format!(
        r#"---
name: test
description: {long_desc}
---

Content.
"#
    );

    let result = parse_skill_file_content(&content);
    assert!(result.is_err());
    let expected = format!("at most {MAX_SKILL_DESCRIPTION_LEN} bytes");
    assert!(result.unwrap_err().to_string().contains(&expected));
}

#[test]
fn test_parse_empty_description() {
    let content = r#"---
name: test
description: ""
---

Content.
"#;

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot be empty"));
}

#[test]
fn test_parse_file_too_large() {
    let large_content = format!(
        r#"---
name: test
description: Test skill
---

{}"#,
        "x".repeat(MAX_SKILL_FILE_SIZE + 1)
    );

    let result = parse_skill_frontmatter(
        Path::new("/skills/test/SKILL.md"),
        &large_content,
        SkillSource::Global,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
}

#[test]
fn test_parse_empty_body_after_frontmatter() {
    let content = r#"---
name: minimal-skill
description: A skill with no body content
---
"#;

    let result = parse_skill_frontmatter(
        Path::new("/skills/minimal/SKILL.md"),
        content,
        SkillSource::Global,
    );

    let skill = result.expect("Empty body should be allowed");
    assert_eq!(skill.name, "minimal-skill");
    assert_eq!(skill.description, "A skill with no body content");
}

#[test]
fn test_parse_whitespace_only_body() {
    let content = "---\nname: whitespace-skill\ndescription: Test\n---\n\n   \n\n   \n";

    let result = parse_skill_frontmatter(
        Path::new("/skills/ws/SKILL.md"),
        content,
        SkillSource::Global,
    );

    let skill = result.expect("Whitespace-only body should be allowed");
    assert_eq!(skill.name, "whitespace-skill");
}

#[test]
fn test_parse_skill_with_crlf_line_endings() {
    let content = "---\r\nname: crlf-skill\r\ndescription: A skill with CRLF line endings\r\n---\r\n\r\n# CRLF Skill\r\n\r\nDo the thing.\r\n";

    let result = parse_skill_frontmatter(
        Path::new("/skills/crlf-skill/SKILL.md"),
        content,
        SkillSource::Global,
    );
    let skill = result.expect("CRLF document should parse successfully");

    assert_eq!(skill.name, "crlf-skill");
    assert_eq!(skill.description, "A skill with CRLF line endings");
}

#[test]
fn test_parse_skill_with_mixed_line_endings() {
    let content = "---\r\nname: mixed-skill\r\ndescription: Frontmatter uses CRLF, body uses LF\r\n---\r\n\n# Mixed Skill\n\nBody uses LF only.\n";

    let result = parse_skill_frontmatter(
        Path::new("/skills/mixed-skill/SKILL.md"),
        content,
        SkillSource::Global,
    );
    let skill = result.expect("Mixed line endings should parse successfully");

    assert_eq!(skill.name, "mixed-skill");
    assert_eq!(skill.description, "Frontmatter uses CRLF, body uses LF");
}

#[test]
fn test_parse_rejects_closing_delimiter_with_trailing_chars() {
    // The only `---` after the opener has trailing junk on the same line,
    // so it isn't a valid closing delimiter and parsing must error.
    let content = "---\nname: foo\ndescription: bar\n---trailing-junk\nbody content\n";

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
fn test_parse_accepts_only_truly_terminated_closing_delimiter() {
    // The first `---trailing` appears inside a quoted YAML string and is
    // NOT alone on its line, so it must not be treated as the closer.
    // The real closer comes later as `\n---\n`.
    let content = "---\nname: skill-name\ndescription: A real description\nsummary: \"---trailing\"\n---\nbody content\n";

    let skill = parse_skill_frontmatter(
        Path::new("/skills/skill-name/SKILL.md"),
        content,
        SkillSource::Global,
    )
    .expect("Should pick the truly-terminated closing delimiter");

    assert_eq!(skill.name, "skill-name");
    assert_eq!(skill.description, "A real description");
}

#[test]
fn test_parse_accepts_four_dashes_as_invalid_closer() {
    // A line of four dashes is NOT a valid closing delimiter; with no
    // valid closer following, parsing must error.
    let content = "---\nname: foo\ndescription: bar\n----\nbody content\n";

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

#[gpui::test]
async fn test_load_skills_from_empty_directory(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/skills", serde_json::json!({})).await;

    let results = load_skills_from_directory(
        &(fs as Arc<dyn Fs>),
        Path::new("/skills"),
        SkillSource::Global,
    )
    .await;
    assert!(results.is_empty());
}

#[gpui::test]
async fn test_load_single_skill(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
            "/skills",
            serde_json::json!({
                "my-skill": {
                    "SKILL.md": "---\nname: my-skill\ndescription: Test skill\n---\n\n# Instructions\nDo stuff."
                }
            }),
        )
        .await;

    let results = load_skills_from_directory(
        &(fs as Arc<dyn Fs>),
        Path::new("/skills"),
        SkillSource::Global,
    )
    .await;

    assert_eq!(results.len(), 1);
    let skill = results[0].as_ref().expect("Should load successfully");
    assert_eq!(skill.name, "my-skill");
    assert_eq!(skill.description, "Test skill");
}

#[gpui::test]
async fn test_load_symlinked_skill_directory(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/external/my-skill",
        serde_json::json!({
            "SKILL.md": "---\nname: my-skill\ndescription: Symlinked skill\n---\n\n# Instructions"
        }),
    )
    .await;
    fs.create_dir(Path::new("/skills")).await.unwrap();
    fs.create_symlink(
        Path::new("/skills/my-skill"),
        PathBuf::from("/external/my-skill"),
    )
    .await
    .unwrap();

    let results = load_skills_from_directory(
        &(fs as Arc<dyn Fs>),
        Path::new("/skills"),
        SkillSource::Global,
    )
    .await;

    assert_eq!(results.len(), 1);
    let skill = results[0].as_ref().expect("Should load successfully");
    assert_eq!(skill.name, "my-skill");
    assert_eq!(skill.description, "Symlinked skill");
    assert_eq!(
        skill.skill_file_path,
        Path::new("/skills/my-skill/SKILL.md")
    );
}

#[gpui::test]
async fn test_load_nested_skills(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/skills",
        serde_json::json!({
            "skill-one": {
                "SKILL.md": "---\nname: skill-one\ndescription: First skill\n---\n\nContent one"
            },
            "skill-two": {
                "SKILL.md": "---\nname: skill-two\ndescription: Second skill\n---\n\nContent two"
            }
        }),
    )
    .await;

    let results = load_skills_from_directory(
        &(fs as Arc<dyn Fs>),
        Path::new("/skills"),
        SkillSource::Global,
    )
    .await;

    assert_eq!(results.len(), 2);
    let names: Vec<&str> = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|s| s.name.as_str())
        .collect();
    assert!(names.contains(&"skill-one"));
    assert!(names.contains(&"skill-two"));
}

#[gpui::test]
async fn test_load_skills_returns_results_sorted_by_path(cx: &mut TestAppContext) {
    // `apply_skill_overrides` resolves same-source name collisions
    // by keeping the first entry in iteration order. Without a
    // stable sort here, the result depends on `fs.read_dir`, which
    // is OS/filesystem-dependent. Assert the contract: results
    // come back sorted by skill file path regardless of insertion
    // order.
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/skills",
        serde_json::json!({
            "charlie": {
                "SKILL.md": "---\nname: charlie\ndescription: C\n---\n\nC"
            },
            "alpha": {
                "SKILL.md": "---\nname: alpha\ndescription: A\n---\n\nA"
            },
            "bravo": {
                "SKILL.md": "---\nname: bravo\ndescription: B\n---\n\nB"
            },
            "delta": {
                "SKILL.md": "No frontmatter, will fail"
            },
        }),
    )
    .await;

    let results = load_skills_from_directory(
        &(fs as Arc<dyn Fs>),
        Path::new("/skills"),
        SkillSource::Global,
    )
    .await;

    assert_eq!(results.len(), 4);

    let paths: Vec<PathBuf> = results
        .iter()
        .map(|r| match r {
            Ok(skill) => skill.skill_file_path.clone(),
            Err(error) => error.path.clone(),
        })
        .collect();

    let mut expected = paths.clone();
    expected.sort();
    assert_eq!(paths, expected);
}

#[gpui::test]
async fn test_load_ignores_non_skill_files(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/skills",
        serde_json::json!({
            "my-skill": {
                "SKILL.md": "---\nname: my-skill\ndescription: Test\n---\n\nContent"
            },
            "not-a-skill.txt": "This is not a skill",
            "some-dir": {
                "other-file.md": "Not a SKILL.md"
            }
        }),
    )
    .await;

    let results = load_skills_from_directory(
        &(fs as Arc<dyn Fs>),
        Path::new("/skills"),
        SkillSource::Global,
    )
    .await;

    assert_eq!(results.len(), 1);
    let skill = results[0].as_ref().expect("Should load successfully");
    assert_eq!(skill.name, "my-skill");
}

#[gpui::test]
async fn test_load_returns_errors_for_invalid_skills(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/skills",
        serde_json::json!({
            "valid-skill": {
                "SKILL.md": "---\nname: valid-skill\ndescription: Valid\n---\n\nContent"
            },
            "invalid-skill": {
                "SKILL.md": "No frontmatter here"
            }
        }),
    )
    .await;

    let results = load_skills_from_directory(
        &(fs as Arc<dyn Fs>),
        Path::new("/skills"),
        SkillSource::Global,
    )
    .await;

    assert_eq!(results.len(), 2);

    let (successes, errors): (Vec<_>, Vec<_>) = results.iter().partition(|r| r.is_ok());

    assert_eq!(successes.len(), 1);
    assert_eq!(errors.len(), 1);

    let error = errors[0].as_ref().unwrap_err();
    assert!(error.path.to_string_lossy().contains("invalid-skill"));
}

#[gpui::test]
async fn test_load_from_nonexistent_directory(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());

    let results = load_skills_from_directory(
        &(fs as Arc<dyn Fs>),
        Path::new("/nonexistent"),
        SkillSource::Global,
    )
    .await;

    assert!(results.is_empty());
}
