use super::*;

#[gpui::test]
async fn test_nested_skill_md_inside_skill_resources_is_not_loaded(cx: &mut TestAppContext) {
    // We only look at immediate children of the skills root, so a
    // `SKILL.md` nested inside a skill's resources directory cannot
    // accidentally be picked up as a separate skill.
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/skills",
        serde_json::json!({
            "outer": {
                "SKILL.md": "---\nname: outer\ndescription: Outer skill\n---\n\nBody",
                "references": {
                    "SKILL.md": "---\nname: bogus-inner\ndescription: Should not load\n---\n\nBody"
                },
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

    let names: Vec<&str> = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(names, vec!["outer"]);
}

#[gpui::test]
async fn test_load_oversized_skill_file_short_circuits(cx: &mut TestAppContext) {
    // A `SKILL.md` whose size exceeds `MAX_SKILL_FILE_SIZE` must be
    // rejected via metadata before we read its contents into memory.
    // Otherwise a stray multi-GB file dropped into a skill directory
    // would OOM the application before `parse_skill`'s size check fires.
    let fs = FakeFs::new(cx.executor());
    let oversized_body = "x".repeat(MAX_SKILL_FILE_SIZE + 1);
    let oversized_content = format!(
        "---\nname: huge\ndescription: Too big\n---\n\n{}",
        oversized_body
    );
    fs.insert_tree(
        "/skills",
        serde_json::json!({
            "huge": {
                "SKILL.md": oversized_content,
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
    let err = results[0].as_ref().expect_err("Oversized file must error");
    assert!(
        err.message.contains("exceeds maximum size"),
        "unexpected error message: {}",
        err.message
    );
}

#[gpui::test]
async fn test_load_skill_frontmatter_parses_metadata_without_body(cx: &mut TestAppContext) {
    // `load_skill_frontmatter` should read just enough of the file to
    // parse the frontmatter and return a `Skill` with name/description/
    // disable_model_invocation populated. The body is intentionally not
    // surfaced; callers go through `read_skill_body` for that.
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
            "/skills",
            serde_json::json!({
                "my-skill": {
                    "SKILL.md": "---\nname: my-skill\ndescription: A skill for tests\ndisable-model-invocation: true\n---\n\n# Body\n\nLots of body text here.\n"
                }
            }),
        )
        .await;

    let skill = load_skill_frontmatter(
        fs as Arc<dyn Fs>,
        PathBuf::from("/skills/my-skill/SKILL.md"),
        SkillSource::Global,
    )
    .await
    .expect("frontmatter should parse");

    assert_eq!(skill.name, "my-skill");
    assert_eq!(skill.description, "A skill for tests");
    assert!(skill.disable_model_invocation);
    assert_eq!(
        skill.skill_file_path,
        PathBuf::from("/skills/my-skill/SKILL.md")
    );
    assert_eq!(skill.directory_path, PathBuf::from("/skills/my-skill"));
}

#[gpui::test]
async fn test_read_skill_body_returns_trimmed_body(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
            "/skills",
            serde_json::json!({
                "my-skill": {
                    "SKILL.md": "---\nname: my-skill\ndescription: Test skill\n---\n\n# Instructions\n\nDo the thing.\n\n"
                }
            }),
        )
        .await;

    let body = read_skill_body(fs.as_ref(), Path::new("/skills/my-skill/SKILL.md"))
        .await
        .expect("body should load");

    // Trimmed: no leading blank line after the closing `---`, and no
    // trailing whitespace.
    assert_eq!(body, "# Instructions\n\nDo the thing.");
}

#[gpui::test]
async fn test_read_skill_body_accepts_description_too_long(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    let long_desc = "a".repeat(MAX_SKILL_DESCRIPTION_LEN + 1);
    fs.insert_tree(
            "/skills",
            serde_json::json!({
                "long-description": {
                    "SKILL.md": format!("---\nname: long-description\ndescription: {long_desc}\n---\n\nBody")
                }
            }),
        )
        .await;

    let body = read_skill_body(fs.as_ref(), Path::new("/skills/long-description/SKILL.md"))
        .await
        .expect("body should load despite description-length warning");

    assert_eq!(body, "Body");
}

#[gpui::test]
async fn test_read_skill_body_for_skill_without_body(cx: &mut TestAppContext) {
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/skills",
        serde_json::json!({
            "empty": {
                "SKILL.md": "---\nname: empty\ndescription: No body\n---\n"
            }
        }),
    )
    .await;

    let body = read_skill_body(fs.as_ref(), Path::new("/skills/empty/SKILL.md"))
        .await
        .expect("body should load");

    assert!(body.is_empty(), "expected empty body, got: {body:?}");
}
