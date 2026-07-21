#[gpui::test]
async fn test_project_skills_require_worktree_trust(cx: &mut TestAppContext) {
    use collections::{HashMap, HashSet};
    use project::trusted_worktrees::{self, PathTrust, TrustedWorktrees};

    init_test(cx);
    cx.update(|cx| {
        // The trust global isn't created by `init_test`. We need it
        // for `Project::test_with_worktree_trust` to actually wire up
        // trust tracking and for our subscription in
        // `register_project_with_initial_context` to fire.
        trusted_worktrees::init(HashMap::default(), cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".agents": {
                "skills": {
                    "my-skill": {
                        "SKILL.md": "---\nname: my-skill\ndescription: A project skill\n---\n\nbody"
                    }
                }
            }
        }),
    )
    .await;

    // `test_with_worktree_trust` initializes the trust system and
    // starts every worktree as restricted, mirroring production
    // behavior on a freshly opened folder.
    let project =
        Project::test_with_worktree_trust(fs.clone(), [Path::new("/project")], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    let connection = NativeAgentConnection(agent.clone());
    let acp_thread = cx
        .update(|cx| {
            Rc::new(connection.clone()).new_session(
                project.clone(),
                PathList::new(&[Path::new("/project")]),
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let project_id = project.entity_id();
    let session_id = acp_thread.read_with(cx, |thread, _cx| thread.session_id().clone());
    let worktree_id = project.read_with(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    // Untrusted: project skills are excluded from the loaded list and
    // never make it into the catalog or slash commands.
    agent.read_with(cx, |agent, cx| {
        let state = agent.projects.get(&project_id).unwrap();
        assert!(
            user_skills(&state.skills).is_empty(),
            "untrusted worktree skills should not load: {:?}",
            state
                .skills
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
        );
        let commands = NativeAgent::build_available_commands_for_project(Some(state), cx);
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(
            !names.contains(&"my-skill"),
            "untrusted skill leaked into slash commands: {names:?}"
        );
    });

    // Granting trust should trigger a context refresh; the skill then
    // appears in both the catalog and the slash-command list.
    cx.update(|cx| {
        let trusted_worktrees = TrustedWorktrees::try_get_global(cx)
            .expect("trusted worktrees global initialized by test_with_worktree_trust");
        trusted_worktrees.update(cx, |trusted_worktrees, cx| {
            trusted_worktrees.trust(
                &project.read(cx).worktree_store(),
                HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                cx,
            );
        });
    });
    cx.run_until_parked();

    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let user = user_skills(&state.skills);
        let names: Vec<&str> = user.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["my-skill"]);
    });

    cx.update(|cx| {
        let skills = connection.available_skills(&session_id, cx);
        let skill_names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert!(
            skill_names.contains(&"my-skill"),
            "trusted skill should appear in available skills: {skill_names:?}"
        );
    });
}

/// Open a session against a freshly created project and trust its only
/// worktree, so project-local skills load. Returns the agent, the
/// project, and the worktree id of the project root.
async fn open_trusted_project_skills(
    cx: &mut TestAppContext,
    fs: Arc<FakeFs>,
    root: &str,
) -> (Entity<NativeAgent>, Entity<Project>, WorktreeId) {
    use collections::{HashMap, HashSet};
    use project::trusted_worktrees::{self, PathTrust, TrustedWorktrees};

    cx.update(|cx| {
        trusted_worktrees::init(HashMap::default(), cx);
    });

    let project = Project::test_with_worktree_trust(fs.clone(), [Path::new(root)], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    let connection = NativeAgentConnection(agent.clone());
    let _acp_thread = cx
        .update(|cx| {
            Rc::new(connection).new_session(
                project.clone(),
                PathList::new(&[Path::new(root)]),
                cx,
            )
        })
        .await
        .unwrap();
    cx.run_until_parked();

    let worktree_id = project.read_with(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });
    cx.update(|cx| {
        let trusted_worktrees = TrustedWorktrees::try_get_global(cx)
            .expect("trusted worktrees global initialized by test_with_worktree_trust");
        trusted_worktrees.update(cx, |trusted_worktrees, cx| {
            trusted_worktrees.trust(
                &project.read(cx).worktree_store(),
                HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
                cx,
            );
        });
    });
    cx.run_until_parked();

    (agent, project, worktree_id)
}

/// The body resolver for a project-local skill must read the file
/// through a project buffer rather than the local filesystem. This is
/// what makes project skills resolvable in remote workspaces, where
/// the `fs` the agent holds is the client's filesystem and not where
/// the project files actually live. We prove the buffer path is used
/// by editing the buffer in memory (without saving) and asserting the
/// resolver returns the edited body, not the on-disk body.
#[gpui::test]
async fn test_project_skill_body_resolves_through_buffer(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".agents": {
                "skills": {
                    "my-skill": {
                        "SKILL.md": "---\nname: my-skill\ndescription: A project skill\n---\n\ndisk body"
                    }
                }
            }
        }),
    )
    .await;

    let (agent, project, worktree_id) =
        open_trusted_project_skills(cx, fs.clone(), "/project").await;
    let project_id = project.entity_id();

    let skill = agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project_id).unwrap();
        user_skills(&state.skills)
            .into_iter()
            .find(|s| s.name == "my-skill")
            .cloned()
            .expect("project skill should be loaded")
    });
    assert!(matches!(skill.source, SkillSource::ProjectLocal { .. }));

    let resolver =
        cx.update(|_cx| super::skill_body_resolver_for_project(project.clone(), fs.clone()));

    let body = cx
        .update(|cx| resolver(skill.clone(), &mut cx.to_async()))
        .await
        .unwrap();
    assert_eq!(body, "disk body");

    // Edit the buffer in memory without writing to disk.
    let relative_path: Arc<RelPath> = rel_path(".agents/skills/my-skill/SKILL.md").into();
    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, relative_path), cx)
        })
        .await
        .unwrap();
    buffer.update(cx, |buffer, cx| {
        buffer.set_text(
            "---\nname: my-skill\ndescription: A project skill\n---\n\nedited body",
            cx,
        );
    });

    let body = cx
        .update(|cx| resolver(skill.clone(), &mut cx.to_async()))
        .await
        .unwrap();
    assert_eq!(
        body, "edited body",
        "resolver must read the in-memory buffer, not the on-disk file"
    );
}

/// A project SKILL.md whose on-disk size exceeds the cap must be
/// rejected with a size-limit error and excluded from the loaded
/// skills, exercising the size guard in `load_project_skills`.
#[gpui::test]
async fn test_oversized_project_skill_reports_error(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    let oversized = format!(
        "---\nname: huge-skill\ndescription: Too big\n---\n\n{}",
        "a".repeat(MAX_SKILL_FILE_SIZE + 1)
    );
    fs.insert_tree(
        "/project",
        json!({
            ".agents": { "skills": { "huge-skill": { "SKILL.md": oversized } } }
        }),
    )
    .await;

    let (agent, project, _worktree_id) =
        open_trusted_project_skills(cx, fs.clone(), "/project").await;
    let project_id = project.entity_id();

    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project_id).unwrap();
        assert!(
            user_skills(&state.skills).is_empty(),
            "oversized skill must not load: {:?}",
            user_skills(&state.skills)
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>()
        );
        assert!(
            state
                .skill_loading_issues
                .iter()
                .any(|issue| issue.kind == SkillLoadingIssueKind::LoadFailed
                    && issue.message.to_string().contains("maximum size")),
            "expected a size-limit error, got {:?}",
            state.skill_loading_issues
        );
    });
}

/// A malformed project SKILL.md must surface a per-skill load error
/// without preventing sibling skills in the same worktree from
/// loading.
#[gpui::test]
async fn test_malformed_project_skill_reports_error(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".agents": {
                "skills": {
                    "good": {
                        "SKILL.md": "---\nname: good\ndescription: Fine\n---\n\nbody"
                    },
                    "bad": {
                        "SKILL.md": "this file has no frontmatter"
                    }
                }
            }
        }),
    )
    .await;

    let (agent, project, _worktree_id) =
        open_trusted_project_skills(cx, fs.clone(), "/project").await;
    let project_id = project.entity_id();

    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let names: Vec<&str> = user_skills(&state.skills)
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, vec!["good"], "only the valid skill should load");
        assert!(
            state
                .skill_loading_issues
                .iter()
                .any(|issue| issue.kind == SkillLoadingIssueKind::LoadFailed
                    && issue.path.ends_with("bad/SKILL.md")),
            "expected an error for the malformed skill, got {:?}",
            state.skill_loading_issues
        );
    });
}

/// The skill catalog (metadata) is also loaded through project
/// buffers, and the broadened `.agents` refresh trigger must rebuild
/// it when files under `.agents` change. We edit the SKILL.md buffer
/// in memory, then touch an unrelated file directly under `.agents`
/// (not under `.agents/skills`) and assert the catalog reflects the
/// in-memory edit. Under the previous `.agents/skills`-only trigger
/// this refresh would not have fired.
#[gpui::test]
async fn test_project_skill_metadata_refreshes_from_buffer(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        json!({
            ".agents": {
                "skills": {
                    "my-skill": {
                        "SKILL.md": "---\nname: my-skill\ndescription: Original\n---\n\nbody"
                    }
                }
            }
        }),
    )
    .await;

    let (agent, project, worktree_id) =
        open_trusted_project_skills(cx, fs.clone(), "/project").await;
    let project_id = project.entity_id();

    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let skill = user_skills(&state.skills)
            .into_iter()
            .find(|s| s.name == "my-skill")
            .expect("skill should be loaded");
        assert_eq!(skill.description, "Original");
    });

    let relative_path: Arc<RelPath> = rel_path(".agents/skills/my-skill/SKILL.md").into();
    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, relative_path), cx)
        })
        .await
        .unwrap();
    buffer.update(cx, |buffer, cx| {
        buffer.set_text(
            "---\nname: my-skill\ndescription: Edited in buffer\n---\n\nbody",
            cx,
        );
    });

    // Touch a file directly under `.agents` (not under
    // `.agents/skills`) to trigger the broadened refresh path.
    fs.insert_file("/project/.agents/marker.txt", b"hello".to_vec())
        .await;
    cx.run_until_parked();

    agent.read_with(cx, |agent, _cx| {
        let state = agent.projects.get(&project_id).unwrap();
        let skill = user_skills(&state.skills)
            .into_iter()
            .find(|s| s.name == "my-skill")
            .expect("skill should still be loaded");
        assert_eq!(
            skill.description, "Edited in buffer",
            "catalog must reflect the in-memory buffer after a refresh"
        );
    });
}

