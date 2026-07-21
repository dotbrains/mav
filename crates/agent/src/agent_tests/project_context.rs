#[gpui::test]
async fn test_maintaining_project_context(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/",
        json!({
            "a": {}
        }),
    )
    .await;
    let project = Project::test(fs.clone(), [], cx).await;
    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let agent =
        cx.update(|cx| NativeAgent::new(thread_store, Templates::new(), fs.clone(), cx));

    // Creating a session registers the project and triggers context building.
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

    let thread = agent.read_with(cx, |agent, _cx| {
        agent.sessions.values().next().unwrap().thread.clone()
    });

    agent.read_with(cx, |agent, cx| {
        let project_id = project.entity_id();
        let state = agent.projects.get(&project_id).unwrap();
        assert_eq!(state.project_context.read(cx).worktrees, vec![]);
        assert_eq!(thread.read(cx).project_context().read(cx).worktrees, vec![]);
    });

    let worktree = project
        .update(cx, |project, cx| project.create_worktree("/a", true, cx))
        .await
        .unwrap();
    cx.run_until_parked();
    agent.read_with(cx, |agent, cx| {
        let project_id = project.entity_id();
        let state = agent.projects.get(&project_id).unwrap();
        let expected_worktrees = vec![WorktreeContext {
            root_name: "a".into(),
            abs_path: Path::new("/a").into(),
            rules_file: None,
        }];
        assert_eq!(state.project_context.read(cx).worktrees, expected_worktrees);
        assert_eq!(
            thread.read(cx).project_context().read(cx).worktrees,
            expected_worktrees
        );
    });

    // Creating `/a/.rules` updates the project context.
    fs.insert_file("/a/.rules", Vec::new()).await;
    cx.run_until_parked();
    agent.read_with(cx, |agent, cx| {
        let project_id = project.entity_id();
        let state = agent.projects.get(&project_id).unwrap();
        let rules_entry = worktree
            .read(cx)
            .entry_for_path(rel_path(".rules"))
            .unwrap();
        let expected_worktrees = vec![WorktreeContext {
            root_name: "a".into(),
            abs_path: Path::new("/a").into(),
            rules_file: Some(RulesFileContext {
                path_in_worktree: rel_path(".rules").into(),
                text: "".into(),
                project_entry_id: rules_entry.id.to_usize(),
            }),
        }];
        assert_eq!(state.project_context.read(cx).worktrees, expected_worktrees);
        assert_eq!(
            thread.read(cx).project_context().read(cx).worktrees,
            expected_worktrees
        );
    });
}

