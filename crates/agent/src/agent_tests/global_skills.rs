#[gpui::test]
async fn test_global_skills_load_and_reload(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let skills_dir = global_skills_dir();
    let initial_skill_dir = skills_dir.join("my-skill");
    let initial_skill_path = initial_skill_dir.join("SKILL.md");
    fs.create_dir(&initial_skill_dir).await.unwrap();
    fs.insert_file(
        &initial_skill_path,
        b"---\nname: my-skill\ndescription: First version\n---\n\nbody-v1".to_vec(),
    )
    .await;

    let project = Project::test(fs.clone(), [], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    // Simulate the user-interaction trigger that the agent panel
    // fires (input focus, slash autocomplete, or submit). In tests
    // we call it directly because there's no panel.
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

    // The pre-existing skill should be loaded into the project state.
    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project.entity_id()).unwrap();
        let user = user_skills(&state.skills);
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, "my-skill");
        assert_eq!(user[0].description, "First version");
    });

    // Modify the SKILL.md and verify the project context refreshes.
    fs.write(
        &initial_skill_path,
        b"---\nname: my-skill\ndescription: Second version\n---\n\nbody-v2",
    )
    .await
    .unwrap();
    cx.run_until_parked();

    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project.entity_id()).unwrap();
        let user = user_skills(&state.skills);
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].description, "Second version");
    });
}

#[gpui::test]
async fn test_global_skill_with_long_description_loads_with_warning(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let skills_dir = global_skills_dir();
    let skill_dir = skills_dir.join("long-description");
    let skill_path = skill_dir.join("SKILL.md");
    let long_description = "a".repeat(agent_skills::MAX_SKILL_DESCRIPTION_LEN + 1);
    fs.create_dir(&skill_dir).await.unwrap();
    fs.insert_file(
        &skill_path,
        format!("---\nname: long-description\ndescription: {long_description}\n---\n\nbody")
            .into_bytes(),
    )
    .await;

    let project = Project::test(fs.clone(), [], cx).await;
    let project_id = project.entity_id();
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    cx.update(|cx| {
        agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
    });

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

    let loaded_skill = agent.read_with(cx, |agent, cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let user = user_skills(&state.skills);
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, "long-description");
        assert_eq!(user[0].description, long_description);

        let catalog_names: Vec<&str> = state
            .project_context
            .read(cx)
            .skills()
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert!(
            catalog_names.contains(&"long-description"),
            "long-description skill should remain in the model catalog: {catalog_names:?}"
        );

        assert!(
            state.skill_loading_issues.iter().any(|issue| {
                issue.kind == SkillLoadingIssueKind::DescriptionTooLong
                    && issue.path == skill_path
                    && issue.message.to_string().contains("1024-byte limit")
            }),
            "expected a description-length warning issue, got {:?}",
            state.skill_loading_issues
        );

        (*user[0]).clone()
    });

    let session_id = acp_thread.read_with(cx, |thread, _cx| thread.session_id().clone());
    cx.update(|cx| {
        let available_skills = connection.available_skills(&session_id, cx);
        let available_skill = available_skills
            .iter()
            .find(|skill| skill.name == "long-description")
            .expect("long-description should appear in available skills");
        assert_eq!(available_skill.description, long_description);
        assert!(
            available_skill
                .warning
                .as_ref()
                .is_some_and(|warning| warning.contains("1024-byte limit")),
            "available skill should expose warning text, got {:?}",
            available_skill.warning
        );
    });

    let body = agent_skills::read_skill_body(fs.as_ref(), &loaded_skill.skill_file_path)
        .await
        .expect("body should load despite description-length warning");
    assert_eq!(body, "body");
}

#[gpui::test]
async fn test_symlinked_global_skills_load_and_reload(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let skills_dir = global_skills_dir();
    let external_skill_dir = PathBuf::from(path!("/external/my-skill"));
    let skill_link_dir = skills_dir.join("my-skill");
    let skill_link_path = skill_link_dir.join("SKILL.md");

    fs.insert_tree(
        &external_skill_dir,
        json!({
            "SKILL.md": "---\nname: my-skill\ndescription: First symlinked version\n---\n\nbody-v1"
        }),
    )
    .await;
    fs.create_dir(&skills_dir).await.unwrap();
    fs.create_symlink(&skill_link_dir, external_skill_dir)
        .await
        .unwrap();

    let project = Project::test(fs.clone(), [], cx).await;
    let project_id = project.entity_id();
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

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

    let loaded_skill = agent.read_with(cx, |agent, cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let user = user_skills(&state.skills);
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, "my-skill");
        assert_eq!(user[0].description, "First symlinked version");
        assert_eq!(user[0].source, SkillSource::Global);
        assert_eq!(user[0].skill_file_path, skill_link_path);

        let catalog_skills = state.project_context.read(cx).skills();
        let catalog_skill = catalog_skills
            .iter()
            .find(|skill| skill.name == "my-skill")
            .expect("symlinked skill should be included in the model-facing catalog");
        assert_eq!(catalog_skill.description, "First symlinked version");
        assert_eq!(
            catalog_skill.location,
            skill_link_path.to_string_lossy().as_ref()
        );

        (*user[0]).clone()
    });
    let body = agent_skills::read_skill_body(fs.as_ref(), &loaded_skill.skill_file_path)
        .await
        .unwrap();
    assert_eq!(body, "body-v1");

    fs.write(
        &skill_link_path,
        b"---\nname: my-skill\ndescription: Second symlinked version\n---\n\nbody-v2",
    )
    .await
    .unwrap();
    cx.run_until_parked();

    let reloaded_skill = agent.read_with(cx, |agent, cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let user = user_skills(&state.skills);
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, "my-skill");
        assert_eq!(user[0].description, "Second symlinked version");
        assert_eq!(user[0].source, SkillSource::Global);
        assert_eq!(user[0].skill_file_path, skill_link_path);

        let catalog_skills = state.project_context.read(cx).skills();
        let catalog_skill = catalog_skills
            .iter()
            .find(|skill| skill.name == "my-skill")
            .expect("reloaded symlinked skill should be included in the model-facing catalog");
        assert_eq!(catalog_skill.description, "Second symlinked version");
        assert_eq!(
            catalog_skill.location,
            skill_link_path.to_string_lossy().as_ref()
        );

        (*user[0]).clone()
    });
    let body = agent_skills::read_skill_body(fs.as_ref(), &reloaded_skill.skill_file_path)
        .await
        .unwrap();
    assert_eq!(body, "body-v2");
}

#[gpui::test]
async fn test_global_skills_dir_created_after_startup(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let skills_dir = global_skills_dir();

    // Intentionally do NOT pre-create `skills_dir`. The first scan
    // trigger should find no directory and leave the watch state
    // idle; a later trigger after the directory is created should
    // attach to the deepest existing ancestor and react when the
    // directory is created later.

    let project = Project::test(fs.clone(), [], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    // First scan trigger: nothing on disk yet, state stays idle.
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

    // No skills directory exists yet, so no skills should be loaded.
    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project.entity_id()).unwrap();
        assert!(
            user_skills(&state.skills).is_empty(),
            "expected no user skills before the global skills dir exists, got {:?}",
            state.skills
        );
    });

    // Create the global skills directory and a skill within it.
    let new_skill_dir = skills_dir.join("late-skill");
    fs.create_dir(&new_skill_dir).await.unwrap();
    fs.insert_file(
        &new_skill_dir.join("SKILL.md"),
        b"---\nname: late-skill\ndescription: Created after startup\n---\n\nbody".to_vec(),
    )
    .await;

    // Fire the trigger again, simulating the user interacting with
    // the agent panel after creating the skills directory. The
    // second scan should find the directory and start the watch,
    // which refreshes project context.
    cx.update(|cx| {
        agent.update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
    });
    cx.run_until_parked();

    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project.entity_id()).unwrap();
        let user = user_skills(&state.skills);
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, "late-skill");
        assert_eq!(user[0].description, "Created after startup");
    });
}

