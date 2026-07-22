use super::*;

#[test]
fn test_skill_source_precedence_is_total_and_ordered() {
    // Pin the hierarchy: project-local > global > built-in. Every
    // override and conflict-resolution site routes through this,
    // so the rest of the codebase relies on it being correct.
    let built_in = SkillSource::BuiltIn.precedence();
    let global = SkillSource::Global.precedence();
    let project = SkillSource::ProjectLocal {
        worktree_id: SkillScopeId(1),
        worktree_root_name: "my-project".into(),
    }
    .precedence();

    assert!(built_in < global, "global must shadow built-in");
    assert!(global < project, "project-local must shadow global");

    // Two project-local skills from different worktrees tie. The
    // "first wins" convention is enforced by the callers, but the
    // precedence itself must be equal so neither silently shadows
    // the other.
    let other_project = SkillSource::ProjectLocal {
        worktree_id: SkillScopeId(2),
        worktree_root_name: "other-project".into(),
    }
    .precedence();
    assert_eq!(project, other_project);
}
