use super::*;

#[test]
fn test_session_capabilities_keep_commands_and_skills_separate() {
    let skill_file_path = PathBuf::from("/tmp/SKILL.md");
    let skill = AvailableSkill {
        name: "deploy".into(),
        description: "Deploy the app".into(),
        source: "".into(),
        skill_file_path: skill_file_path.clone(),
        warning: None,
    };
    let session_capabilities = SessionCapabilities::new(
        acp::PromptCapabilities::default(),
        vec![acp::AvailableCommand::new("help", "Get help")],
        vec![skill],
    );

    assert_eq!(session_capabilities.completion_commands().len(), 1);
    let skills = session_capabilities.completion_skills();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name.as_ref(), "deploy");
    assert_eq!(skills[0].skill_file_path, skill_file_path);
}

#[test]
fn test_completion_commands_derive_category_from_meta() {
    let session_capabilities = SessionCapabilities::new(
        acp::PromptCapabilities::default(),
        vec![
            acp::AvailableCommand::new("compact", "Built-in").meta(
                acp_thread::meta_with_command_category(acp_thread::CommandCategory::Native),
            ),
            acp::AvailableCommand::new("deploy", "MCP").meta(
                acp_thread::meta_with_command_category(acp_thread::CommandCategory::Mcp),
            ),
            // No category meta: this is how external ACP agents' commands
            // arrive, and they should group on their own.
            acp::AvailableCommand::new("help", "External"),
        ],
        Vec::new(),
    );

    let commands = session_capabilities.completion_commands();
    let category = |name: &str| {
        commands
            .iter()
            .find(|command| command.name.as_ref() == name)
            .unwrap()
            .category
    };
    assert_eq!(
        category("compact"),
        Some(acp_thread::CommandCategory::Native)
    );
    assert_eq!(category("deploy"), Some(acp_thread::CommandCategory::Mcp));
    assert_eq!(category("help"), None);
}

#[test]
fn test_validate_slash_commands_accepts_scope_qualified_skill() {
    let agent_id = AgentId::from("Mav");
    let make_skill = |name: &str, source: &str| AvailableSkill {
        name: name.into(),
        description: "desc".into(),
        source: source.into(),
        skill_file_path: PathBuf::from(format!("/tmp/{source}-{name}/SKILL.md")),
        warning: None,
    };

    // Global skills carry an empty scope (so the popup inserts
    // `/:<name>`); project-local skills carry their worktree root
    // name. The empty-scope encoding means a worktree literally
    // named `global` no longer collides with the global source.
    let commands = vec![acp::AvailableCommand::new("help", "Get help")];
    let skills = vec![make_skill("deploy", ""), make_skill("deploy", "mav")];
    let no_skills = Vec::new();

    // Bare name still works (current behavior — the resolver
    // applies project-overrides-global for unqualified commands).
    MessageEditor::validate_slash_commands("/deploy", &commands, &skills, &agent_id)
        .expect("bare /deploy should validate when a skill named `deploy` exists");
    MessageEditor::validate_slash_commands("/mav:deploy", &commands, &no_skills, &agent_id)
        .expect_err("scope-qualified skills should require a first-class available skill");

    // Scope-qualified forms both validate, each pointing at the
    // matching source. `/:<name>` is the qualified form for a
    // global skill; `/<worktree>:<name>` is the qualified form
    // for a project-local skill.
    MessageEditor::validate_slash_commands("/:deploy", &commands, &skills, &agent_id)
        .expect("/:deploy should validate when a global skill named `deploy` exists");
    MessageEditor::validate_slash_commands("/mav:deploy", &commands, &skills, &agent_id).expect(
        "/mav:deploy should validate when a project skill named `deploy` exists in the `mav` worktree",
    );

    // Hand-typed `/global:<name>` is NOT an alias for `/:<name>`.
    // It looks for a project-local skill from a worktree named
    // `global`, and fails when no such worktree skill exists.
    MessageEditor::validate_slash_commands("/global:deploy", &commands, &skills, &agent_id)
        .expect_err(
            "/global:deploy should fail when no worktree named `global` has a `deploy` skill",
        );

    // The `:` separator is what distinguishes a skill scope from
    // an MCP server prefix — the dotted form `/mav.deploy` is an
    // MCP-style lookup, which doesn't match here.
    MessageEditor::validate_slash_commands("/mav.deploy", &commands, &skills, &agent_id)
        .expect_err("/mav.deploy (dotted) should be treated as an MCP-style prefix and fail");

    // Wrong scope is rejected so the resolver doesn't silently
    // fall through when the user meant a skill. `mav:help` looks
    // like a skill scope qualifier but no skill named `help`
    // exists in the `mav` worktree (it's an MCP command).
    let err = MessageEditor::validate_slash_commands("/mav:help", &commands, &skills, &agent_id)
        .expect_err("/mav:help should fail — `help` is an MCP command, not a worktree skill");
    let err_message = err.to_string();
    assert!(
        err_message.contains("/mav:help"),
        "error should mention the typed command: {err_message}"
    );
    // Error listing shows qualified forms for skills so users see
    // the exact text the popup would have inserted. Globals
    // render with an empty scope as `/:<name>`.
    assert!(
        err_message.contains("/:deploy"),
        "error listing should show qualified global form: {err_message}"
    );
    assert!(
        err_message.contains("/mav:deploy"),
        "error listing should show qualified worktree form: {err_message}"
    );
    assert!(
        err_message.contains("/help"),
        "error listing should still show bare MCP commands: {err_message}"
    );

    // Slashes that appear mid-text (paths, URLs, pasted logs)
    // should NOT be validated as commands.
    MessageEditor::validate_slash_commands("check /docs for info", &commands, &skills, &agent_id)
        .expect("mid-text /docs should not be treated as a slash command");

    MessageEditor::validate_slash_commands("see /usr/local/bin/foo", &commands, &skills, &agent_id)
        .expect("file paths containing slashes should not trigger validation");
}
