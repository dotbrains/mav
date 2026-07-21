/// Regression test for the case where a skill is added (e.g. by the
/// SKILL.md file watcher) AFTER a session is registered. The system
/// prompt and slash-command list both read live state, so they pick
/// up the new skill automatically. The `SkillTool` registered on the
/// thread used to hold a stale snapshot of `state.skills` taken at
/// thread-construction time, which meant the model would see the new
/// skill in `<available_skills>` but get "not found" when it tried to
/// invoke it. The fix wires the tool to a dynamic resolver closure
/// that re-reads `state.skills` for the project on every invocation.
#[gpui::test]
async fn test_skills_added_after_session_visible_to_skill_tool(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let skills_dir = global_skills_dir();

    // No skills directory exists at startup; the watcher should
    // create one and pick up SKILL.md when it's added later.
    let project = Project::test(fs.clone(), [], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    // First scan trigger: nothing on disk yet.
    cx.update(|cx| {
        agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
    });

    let connection = NativeAgentConnection(agent.clone());
    let _acp_thread = cx
        .update(|cx| {
            Rc::new(connection).new_session(
                project.clone(),
                PathList::new(&[Path::new("/")]),
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let project_id = project.entity_id();
    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project_id).unwrap();
        assert!(
            user_skills(&state.skills).is_empty(),
            "expected no user skills before the global skills dir exists, got {:?}",
            state.skills
        );
    });

    // Build the same resolver closure that `register_session` uses.
    // This is the production resolver factored into a helper so the
    // test can verify resolution behavior directly without setting
    // up the full tool-call plumbing (`ToolInput`,
    // `ToolCallEventStream`, authorization channel, ...).
    let resolve =
        cx.update(|_cx| super::skills_resolver_for_project(agent.downgrade(), project_id));

    // Sanity check: before any skills exist, the resolver returns an
    // empty list — NOT the snapshot that `Thread::new` would have
    // captured.
    cx.update(|cx| {
        let all = resolve(cx);
        let user: Vec<_> = all
            .iter()
            .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
            .collect();
        assert!(user.is_empty());
    });

    // Now create a SKILL.md AFTER the session was registered. With
    // the old code this would be invisible to the `SkillTool`
    // because the tool held an `Arc<Vec<Skill>>` snapshot taken at
    // thread construction time.
    let new_skill_dir = skills_dir.join("my-skill");
    fs.create_dir(&new_skill_dir).await.unwrap();
    fs.insert_file(
        &new_skill_dir.join("SKILL.md"),
        b"---\nname: my-skill\ndescription: Created after session\n---\n\nbody".to_vec(),
    )
    .await;

    // Second scan trigger: now the directory exists, so the scan
    // starts the watch and refreshes project context.
    cx.update(|cx| {
        agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
    });
    cx.run_until_parked();

    // `state.skills` reflects the new skill (the watcher ran).
    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let user = user_skills(&state.skills);
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, "my-skill");
    });

    // The resolver the `SkillTool` uses must see it too. This is the
    // crux of the regression test: the tool's view of skills is
    // resolved at invocation time, not at thread-construction time.
    cx.update(|cx| {
        let all = resolve(cx);
        let snapshot: Vec<_> = all
            .iter()
            .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
            .collect();
        assert_eq!(
            snapshot.len(),
            1,
            "dynamic resolver should see the new skill"
        );
        assert_eq!(snapshot[0].name, "my-skill");
        assert_eq!(snapshot[0].description, "Created after session");
    });

    // And rendering the envelope through the same path the tool uses
    // produces a `<skill_content name="my-skill">` block, confirming
    // the model would see the new skill if it invoked the tool.
    let skill_for_render = cx.update(|cx| {
        let snapshot = resolve(cx);
        snapshot
            .iter()
            .find(|s| s.name == "my-skill" && !s.disable_model_invocation)
            .cloned()
            .expect("my-skill should be model-invocable")
    });
    let body = agent_skills::read_skill_body(fs.as_ref(), &skill_for_render.skill_file_path)
        .await
        .expect("skill body should load");
    let rendered = render_skill_envelope(&skill_for_render, &body);
    assert!(
        rendered.contains("<skill_content name=\"my-skill\">"),
        "rendered envelope missing skill_content tag: {rendered}"
    );
}

/// Subagents must inherit access to the same skills as their parent.
/// Production wires this up in `NativeThreadEnvironment::create_subagent_thread`,
/// which calls `agent.register_session(subagent, project_id, ...)` —
/// `register_session` is what installs the `SkillTool` on the thread
/// using a resolver closure keyed on `project_id`. Because the
/// subagent shares its parent's `project_id`, both threads end up
/// resolving skills against the same `state.skills`.
///
/// This test exercises that production path directly: it creates a
/// parent session via the agent connection, builds a subagent thread
/// the same way `create_subagent_thread` does, and runs it through
/// `register_session`. It then asserts that the `SkillTool` is
/// registered on the subagent thread and that resolving against the
/// same `project_id` produces the same skill set the parent sees.
#[gpui::test]
async fn test_subagent_skills_lookup_matches_parent(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let skills_dir = global_skills_dir();
    let skill_dir = skills_dir.join("shared-skill");
    fs.create_dir(&skill_dir).await.unwrap();
    fs.insert_file(
        &skill_dir.join("SKILL.md"),
        b"---\nname: shared-skill\ndescription: A shared skill\n---\n\nbody".to_vec(),
    )
    .await;

    let project = Project::test(fs.clone(), [], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    // Open a parent session through the connection, the same way
    // production does. This triggers project-context refresh which
    // populates `state.skills` for the project.
    let connection = NativeAgentConnection(agent.clone());
    let _parent_acp = cx
        .update(|cx| {
            Rc::new(connection).new_session(
                project.clone(),
                PathList::new(&[Path::new("/")]),
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let project_id = project.entity_id();

    // Sanity check: resolving against the parent's project sees the skill.
    let parent_resolve =
        cx.update(|_cx| super::skills_resolver_for_project(agent.downgrade(), project_id));
    cx.update(|cx| {
        let all = parent_resolve(cx);
        let parent_skills: Vec<_> = all
            .iter()
            .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
            .collect();
        assert_eq!(parent_skills.len(), 1);
        assert_eq!(parent_skills[0].name, "shared-skill");
    });

    // Grab the parent thread out of the agent's session map. This
    // mirrors what `create_subagent_thread` does internally — it
    // looks up the parent session by `parent_session_id` and reads
    // its `project_id` to forward to `register_session`.
    let (parent_thread, parent_project_id) = agent.read_with(cx, |agent, _cx| {
        let session = agent
            .sessions
            .values()
            .next()
            .expect("parent session should exist");
        (session.thread.clone(), session.project_id)
    });
    assert_eq!(parent_project_id, project_id);

    // Build the subagent thread the same way
    // `NativeThreadEnvironment::create_subagent_thread` does.
    let subagent_thread = cx.update(|cx| cx.new(|cx| Thread::new_subagent(&parent_thread, cx)));

    // Run the subagent through the production registration path.
    // This is what installs the `SkillTool` on the thread.
    let _subagent_acp = agent.update(cx, |agent, cx| {
        agent.register_session(subagent_thread.clone(), parent_project_id, 1, cx)
    });

    // Verify the subagent thread has the `SkillTool` installed —
    // without `register_session`, it would not.
    subagent_thread.read_with(cx, |thread, _cx| {
        assert!(thread.is_subagent());
        assert!(
            thread.has_registered_tool(SkillTool::NAME),
            "subagent should have SkillTool registered after register_session"
        );
    });

    // The subagent's `SkillTool` is wired to a resolver closure keyed
    // on the same `project_id` the parent used, so it sees the same
    // skill set. We check this by constructing an equivalent resolver
    // against the same project_id and asserting it matches.
    let subagent_resolve = cx
        .update(|_cx| super::skills_resolver_for_project(agent.downgrade(), parent_project_id));
    cx.update(|cx| {
        let all = subagent_resolve(cx);
        let subagent_skills: Vec<_> = all
            .iter()
            .filter(|s| !matches!(s.source, SkillSource::BuiltIn))
            .collect();
        assert_eq!(subagent_skills.len(), 1);
        assert_eq!(subagent_skills[0].name, "shared-skill");
    });
}

#[gpui::test]
async fn test_skills_appear_as_available_skills(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let skills_dir = global_skills_dir();

    // Two skills: one model-invocable (default), one slash-only via
    // `disable-model-invocation: true`. Both should still appear in
    // the slash menu as first-class skills.
    let visible_dir = skills_dir.join("visible-skill");
    fs.create_dir(&visible_dir).await.unwrap();
    fs.insert_file(
        &visible_dir.join("SKILL.md"),
        b"---\nname: visible-skill\ndescription: Visible skill\n---\n\nbody".to_vec(),
    )
    .await;

    let hidden_dir = skills_dir.join("deploy");
    fs.create_dir(&hidden_dir).await.unwrap();
    fs.insert_file(
        &hidden_dir.join("SKILL.md"),
        b"---\nname: deploy\ndescription: Deploy to prod\ndisable-model-invocation: true\n---\n\nbody"
            .to_vec(),
    )
    .await;

    let project = Project::test(fs.clone(), [], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    let connection = NativeAgentConnection(agent.clone());
    let acp_thread = cx
        .update(|cx| {
            Rc::new(connection.clone()).new_session(
                project.clone(),
                PathList::new(&[Path::new("/")]),
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let project_id = project.entity_id();
    let session_id = acp_thread.read_with(cx, |thread, _cx| thread.session_id().clone());

    agent.read_with(cx, |agent, cx| {
        let commands = NativeAgent::build_available_commands_for_project(
            agent.projects.get(&project_id),
            cx,
        );
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(
            !names.contains(&"visible-skill"),
            "skills should not be exposed as ACP slash commands: {names:?}"
        );
        assert!(
            !names.contains(&"deploy"),
            "slash-only skills should not be exposed as ACP slash commands: {names:?}"
        );
    });

    cx.update(|cx| {
        let skills = connection.available_skills(&session_id, cx);
        let names: Vec<&str> = skills.iter().map(|skill| skill.name.as_str()).collect();
        assert!(
            names.contains(&"visible-skill"),
            "visible skill missing from available skills: {names:?}"
        );
        assert!(
            names.contains(&"deploy"),
            "slash-only skill missing from available skills: {names:?}"
        );
    });

    // The model's catalog (ProjectContext.skills) should NOT include
    // `deploy` since it has disable_model_invocation set.
    agent.read_with(cx, |agent, cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let catalog: Vec<&str> = state
            .project_context
            .read(cx)
            .skills()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            catalog.contains(&"visible-skill"),
            "visible skill missing from catalog: {catalog:?}"
        );
        assert!(
            !catalog.contains(&"deploy"),
            "deploy should be excluded from catalog: {catalog:?}"
        );
    });
}

