fn make_project_skill(name: &str, description: &str, worktree: &str) -> Skill {
    Skill {
        name: name.to_string(),
        description: description.to_string(),
        source: SkillSource::ProjectLocal {
            worktree_id: SkillScopeId(1),
            worktree_root_name: worktree.into(),
        },
        directory_path: PathBuf::from(format!("/{worktree}/.agents/skills/{name}")),
        skill_file_path: PathBuf::from(format!("/{worktree}/.agents/skills/{name}/SKILL.md")),
        load_warnings: Vec::new(),
        disable_model_invocation: false,
        embedded_body: None,
    }
}

fn make_builtin_skill(name: &str, description: &str) -> Skill {
    Skill {
        name: name.to_string(),
        description: description.to_string(),
        source: SkillSource::BuiltIn,
        directory_path: PathBuf::from(format!("/builtin/{name}")),
        skill_file_path: PathBuf::from(format!("/builtin/{name}/SKILL.md")),
        load_warnings: Vec::new(),
        disable_model_invocation: false,
        embedded_body: Some("built-in body"),
    }
}

#[test]
fn test_combine_skills_keeps_every_entry_for_autocomplete() {
    // The autocomplete popup needs both same-named entries so the
    // source label can disambiguate them. `combine_skills` must not
    // drop the global when a project-local shares its name.
    let global = make_global_skill("review", "Global review");
    let project = make_project_skill("review", "Project review", "project");

    let (skills, errors) = combine_skills(vec![Ok(global)], vec![Ok(project)].into_iter());

    assert!(errors.is_empty());
    let user = user_skills(&skills);
    assert_eq!(user.len(), 2);
    assert!(matches!(user[0].source, SkillSource::Global));
    assert!(matches!(user[1].source, SkillSource::ProjectLocal { .. }));
}

#[test]
fn test_apply_skill_overrides_project_wins_over_global() {
    // The model-facing projection collapses the same name to a
    // single entry, with the project-local winning. This is what
    // `select_catalog_skills`, `SkillTool`, and the slash-command
    // resolver all see.
    let global = make_global_skill("review", "Global review");
    let project = make_project_skill("review", "Project review", "project");

    let resolved = apply_skill_overrides(&[global, project]);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].description, "Project review");
    assert!(matches!(
        resolved[0].source,
        SkillSource::ProjectLocal { .. }
    ));
}

#[test]
fn test_apply_skill_overrides_same_source_collision_keeps_first() {
    // Two globals (or two project-locals from different worktrees)
    // colliding don't have a clear winner; preserve the historical
    // "first one wins" behavior.
    let first = make_global_skill("review", "First");
    let second = make_global_skill("review", "Second");

    let resolved = apply_skill_overrides(&[first, second]);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].description, "First");
}

#[test]
fn test_apply_skill_overrides_global_wins_over_builtin() {
    // A global skill with the same name as a built-in must shadow
    // the built-in in the model-facing projection, regardless of
    // iteration order.
    let built_in = make_builtin_skill("create-skill", "Built-in version");
    let global = make_global_skill("create-skill", "User override");

    let resolved = apply_skill_overrides(&[built_in, global]);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].description, "User override");
    assert!(matches!(resolved[0].source, SkillSource::Global));
}

#[test]
fn test_apply_skill_overrides_project_wins_over_builtin() {
    let built_in = make_builtin_skill("create-skill", "Built-in version");
    let project = make_project_skill("create-skill", "Project override", "my-project");

    let resolved = apply_skill_overrides(&[built_in, project]);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].description, "Project override");
    assert!(matches!(
        resolved[0].source,
        SkillSource::ProjectLocal { .. }
    ));
}

#[test]
fn test_apply_skill_overrides_project_wins_over_builtin_and_global() {
    // All three sources present — the project-local must win and
    // both lower-precedence entries must be dropped from the
    // model-facing projection.
    let built_in = make_builtin_skill("create-skill", "Built-in");
    let global = make_global_skill("create-skill", "Global");
    let project = make_project_skill("create-skill", "Project", "my-project");

    let resolved = apply_skill_overrides(&[built_in, global, project]);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].description, "Project");
}

#[test]
fn test_apply_skill_overrides_preserves_unique_skills() {
    let global_a = make_global_skill("alpha", "a");
    let global_b = make_global_skill("beta", "b");
    let project_c = make_project_skill("gamma", "c", "project");

    let resolved = apply_skill_overrides(&[global_a, global_b, project_c]);

    assert_eq!(resolved.len(), 3);
    let names: Vec<&str> = resolved.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn test_skill_source_scope_prefix_and_matches_scope() {
    // The popup inserts `/<prefix>:<name>` using `scope_prefix`,
    // and the resolver routes via `matches_scope`. This test pins
    // the contract that the two stay in sync.
    let global = SkillSource::Global;
    // Globals use an empty prefix, so the popup inserts `/:<name>`.
    assert_eq!(global.scope_prefix(), "");
    assert!(global.matches_scope(""));
    // Hand-typed `/global:<name>` is not aliased to the global
    // source; it looks for a worktree literally named `global`.
    assert!(!global.matches_scope("global"));
    assert!(!global.matches_scope("mav"));

    let project = SkillSource::ProjectLocal {
        worktree_id: SkillScopeId(1),
        worktree_root_name: "mav".into(),
    };
    // Project-local skills are scoped by their worktree root name
    // so multiple open worktrees with same-named skills can each
    // be addressed unambiguously.
    assert_eq!(project.scope_prefix(), "mav");
    assert!(project.matches_scope("mav"));
    // The empty scope is reserved for globals.
    assert!(!project.matches_scope(""));
    // An unrelated worktree name (or MCP server name) must not
    // match a project skill from a different worktree.
    assert!(!project.matches_scope("extensions"));

    // A worktree literally named `global` is no longer ambiguous
    // with the global source: its skills are invoked as
    // `/global:<name>` while globals are invoked as `/:<name>`.
    let project_named_global = SkillSource::ProjectLocal {
        worktree_id: SkillScopeId(2),
        worktree_root_name: "global".into(),
    };
    assert_eq!(project_named_global.scope_prefix(), "global");
    assert!(project_named_global.matches_scope("global"));
    assert!(!project_named_global.matches_scope(""));
}

#[test]
fn test_select_catalog_skills_emits_issue_for_dropped_skills() {
    // Each skill's name + description occupies ~10KB. With a 50KB
    // budget, only the first ~5 visible skills fit; the rest must
    // appear as loading issues so the UI can surface them.
    let description = "x".repeat(10 * 1024);
    let mut skills = Vec::new();
    let total = 10;
    for i in 0..total {
        let name = format!("skill-{i:02}");
        skills.push(Skill {
            name: name.clone(),
            description: description.clone(),
            source: SkillSource::Global,
            directory_path: PathBuf::from(format!("/skills/{name}")),
            skill_file_path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
            load_warnings: Vec::new(),
            disable_model_invocation: false,
            embedded_body: None,
        });
    }

    let (kept, issues) = select_catalog_skills(&skills);

    assert!(
        kept.len() < skills.len(),
        "some skills should be dropped due to the budget (kept {} of {})",
        kept.len(),
        skills.len(),
    );
    assert_eq!(
        issues.len(),
        1,
        "all dropped skills should be consolidated into a single issue, got {issues:?}",
    );

    let kept_size: usize = kept
        .iter()
        .map(|s| s.name.len() + s.description.len())
        .sum();
    assert!(
        kept_size <= MAX_SKILL_DESCRIPTIONS_SIZE,
        "kept skills must fit in the budget (got {kept_size} bytes)",
    );

    let issue = &issues[0];
    assert_eq!(issue.kind, SkillLoadingIssueKind::CatalogBudgetExceeded);
    assert!(
        issue.message.contains("50KB") && issue.message.contains("budget"),
        "issue message {:?} should describe the budget",
        issue.message,
    );
    assert_eq!(
        issue.path,
        skills[kept.len()].skill_file_path,
        "issue path should match the first dropped skill",
    );

    for dropped_skill in &skills[kept.len()..total] {
        let name = &dropped_skill.name;
        assert!(
            issue.message.contains(name.as_str()),
            "issue message {:?} should mention the dropped skill name {name:?}",
            issue.message,
        );
        let bullet_line = format!("- {name}");
        assert!(
            issue
                .message
                .lines()
                .any(|line| line.starts_with(&bullet_line)),
            "issue message {:?} should contain a bullet line starting with {bullet_line:?}",
            issue.message,
        );
    }
}

#[test]
fn test_select_catalog_skills_stops_packing_after_first_overflow() {
    // Once a model-invocable skill overflows the budget, no later
    // skills should be admitted, even if they're small enough to fit
    // in the remaining sliver. This keeps the cutoff deterministic by
    // sort order rather than dependent on individual skill sizes.
    let half_description = "a".repeat(MAX_SKILL_DESCRIPTIONS_SIZE / 2);
    let big_description = "b".repeat(MAX_SKILL_DESCRIPTIONS_SIZE);
    let small_description = "c".repeat(100);

    let first = Skill {
        name: "skill-01-first".to_string(),
        description: half_description,
        source: SkillSource::Global,
        directory_path: PathBuf::from("/skills/skill-01-first"),
        skill_file_path: PathBuf::from("/skills/skill-01-first/SKILL.md"),
        load_warnings: Vec::new(),
        disable_model_invocation: false,
        embedded_body: None,
    };
    let second = Skill {
        name: "skill-02-overflows".to_string(),
        description: big_description,
        source: SkillSource::Global,
        directory_path: PathBuf::from("/skills/skill-02-overflows"),
        skill_file_path: PathBuf::from("/skills/skill-02-overflows/SKILL.md"),
        load_warnings: Vec::new(),
        disable_model_invocation: false,
        embedded_body: None,
    };
    let third = Skill {
        name: "skill-03-would-fit".to_string(),
        description: small_description,
        source: SkillSource::Global,
        directory_path: PathBuf::from("/skills/skill-03-would-fit"),
        skill_file_path: PathBuf::from("/skills/skill-03-would-fit/SKILL.md"),
        load_warnings: Vec::new(),
        disable_model_invocation: false,
        embedded_body: None,
    };

    // Sanity-check the test setup: the third skill is small enough
    // that a greedy packer would have squeemav it in alongside the
    // first one.
    let leftover_after_first =
        MAX_SKILL_DESCRIPTIONS_SIZE - (first.name.len() + first.description.len());
    assert!(
        third.name.len() + third.description.len() <= leftover_after_first,
        "third skill must fit in the leftover sliver for this test to be meaningful",
    );

    let skills = vec![first.clone(), second.clone(), third.clone()];
    let (kept, issues) = select_catalog_skills(&skills);

    let kept_names: Vec<&str> = kept.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(kept_names, vec![first.name.as_str()]);

    assert_eq!(issues.len(), 1, "expected a single consolidated issue");
    assert_eq!(issues[0].kind, SkillLoadingIssueKind::CatalogBudgetExceeded);
    assert_eq!(issues[0].path, second.skill_file_path);
    assert!(
        issues[0].message.contains(second.name.as_str()),
        "issue message {:?} should mention {:?}",
        issues[0].message,
        second.name,
    );
    assert!(
        issues[0].message.contains(third.name.as_str()),
        "issue message {:?} should mention {:?}",
        issues[0].message,
        third.name,
    );
    assert!(
        issues[0].message.contains("- "),
        "issue message {:?} should use bullet form when multiple skills are dropped",
        issues[0].message,
    );
}

#[test]
fn test_select_catalog_skills_excludes_hidden_skills_from_catalog() {
    // Hidden skills (`disable_model_invocation: true`) are slash-only and
    // must not appear in the catalog returned by `select_catalog_skills`,
    // even when they would otherwise fit in the budget. They also don't
    // count against the budget, so a hidden skill larger than the entire
    // budget shouldn't generate a loading issue or prevent later visible
    // skills from fitting.
    let huge_description = "y".repeat(MAX_SKILL_DESCRIPTIONS_SIZE * 2);
    let hidden = Skill {
        name: "hidden-huge".to_string(),
        description: huge_description,
        source: SkillSource::Global,
        directory_path: PathBuf::from("/skills/hidden-huge"),
        skill_file_path: PathBuf::from("/skills/hidden-huge/SKILL.md"),
        load_warnings: Vec::new(),
        disable_model_invocation: true,
        embedded_body: None,
    };
    let visible = Skill {
        name: "visible".to_string(),
        description: "short".to_string(),
        source: SkillSource::Global,
        directory_path: PathBuf::from("/skills/visible"),
        skill_file_path: PathBuf::from("/skills/visible/SKILL.md"),
        load_warnings: Vec::new(),
        disable_model_invocation: false,
        embedded_body: None,
    };

    let (kept, issues) = select_catalog_skills(&[hidden, visible]);

    assert!(issues.is_empty(), "expected no issues, got: {issues:?}");
    let kept_names: Vec<&str> = kept.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(kept_names, vec!["visible"]);
}
