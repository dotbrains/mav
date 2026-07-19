use super::*;
use agent_skills::{Skill, SkillSource};
use std::path::PathBuf;

#[test]
fn test_project_context_does_not_filter_by_budget() {
    // The budget is enforced upstream in `agent.rs::select_catalog_skills`
    // so that dropped skills can surface as load errors. ProjectContext
    // should accept whatever summaries it's given.
    let huge_description = "x".repeat(60 * 1024);
    let skill = Skill {
        name: "oversized".to_string(),
        description: huge_description.clone(),
        source: SkillSource::Global,
        directory_path: PathBuf::from("/skills/oversized"),
        skill_file_path: PathBuf::from("/skills/oversized/SKILL.md"),
        load_warnings: Vec::new(),
        disable_model_invocation: false,
        embedded_body: None,
    };
    let summary = SkillSummary::from(&skill);

    let context = ProjectContext::new(vec![]).with_skills(vec![summary]);
    assert_eq!(context.skills.len(), 1);
    assert_eq!(context.skills[0].description, huge_description);
}

#[test]
fn test_empty_skills_sets_has_skills_false() {
    let context = ProjectContext::new(vec![]);
    assert!(!context.has_skills);
    assert!(context.skills.is_empty());
}

// Hidden-skill filtering used to live here, but it's now the
// responsibility of `select_catalog_skills` in `agent.rs`, which is the
// single source of truth for which skills enter the catalog.
// `ProjectContext::new` simply converts whatever skills it receives
// into summaries, so there's no behavior left to test at this layer.
