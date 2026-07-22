use super::*;

#[test]
fn test_skill_summary_from_skill() {
    let skill = Skill {
        name: "test-skill".to_string(),
        description: "A test description".to_string(),
        source: SkillSource::Global,
        directory_path: PathBuf::from("/skills/test-skill"),
        skill_file_path: PathBuf::from("/skills/test-skill/SKILL.md"),
        load_warnings: Vec::new(),
        disable_model_invocation: false,
        embedded_body: None,
    };

    let summary = SkillSummary::from(&skill);
    assert_eq!(summary.name, "test-skill");
    assert_eq!(summary.description, "A test description");
    assert_eq!(summary.location, "/skills/test-skill/SKILL.md");
}
