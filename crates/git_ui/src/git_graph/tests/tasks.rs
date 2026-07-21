use super::*;

#[gpui::test]
async fn test_global_git_command_task_runs_from_context_menu(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;

    let commit_sha = Oid::try_from("abcdef1234567890abcdef1234567890abcdef12")
        .expect("commit SHA should be valid");
    fs.set_graph_commits(
        Path::new("/project/.git"),
        vec![Arc::new(InitialGraphCommitData {
            sha: commit_sha,
            parents: SmallVec::new(),
            ref_names: Vec::new(),
        })],
    );

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("project should have an active repository")
    });
    let task_inventory = project.read_with(cx, |project, cx| {
        project
            .task_store()
            .read(cx)
            .task_inventory()
            .cloned()
            .expect("project should have a task inventory")
    });

    task_inventory.update(cx, |inventory, _| {
        inventory
            .update_file_based_tasks(
                TaskSettingsLocation::Global(Path::new("/tasks.json")),
                Some(
                    &serde_json::to_string(&json!([
                        // Tagged global task that should be scheduled from the Git graph context menu.
                        {
                            "label": "Git Show $MAV_GIT_SHA_SHORT",
                            "command": "git",
                            "args": ["show", "$MAV_GIT_SHA"],
                            "cwd": "$MAV_GIT_REPOSITORY_PATH",
                            "env": {
                                "REPOSITORY": "$MAV_GIT_REPOSITORY_NAME",
                            },
                            "tags": [GIT_COMMAND_TASK_TAG],
                        },
                        // Untagged task that should not appear in the Git graph context menu.
                        {
                            "label": "Git Status",
                            "command": "git",
                            "args": ["status"],
                        },
                        // Tagged task that still should not appear because Git graph task contexts
                        // do not provide editor-specific variables.
                        {
                            "label": "Print File $MAV_FILE",
                            "command": "echo",
                            "args": ["$MAV_FILE"],
                            "tags": [GIT_COMMAND_TASK_TAG],
                        },
                    ]))
                    .expect("tasks JSON should serialize"),
                ),
            )
            .expect("tasks should parse");
    });

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        workspace::MultiWorkspace::test_new(project.clone(), window, cx)
    });
    let workspace = multi_workspace.read_with(&*cx, |multi_workspace, _| {
        multi_workspace.workspace().clone()
    });
    let workspace_weak = workspace.downgrade();

    let git_graph = cx.new_window_entity(|window, cx| {
        GitGraph::new(
            repository.read(cx).id,
            project.read(cx).git_store().clone(),
            workspace_weak,
            None,
            window,
            cx,
        )
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(git_graph.clone()), None, true, window, cx);
    });
    cx.run_until_parked();

    git_graph.update_in(cx, |git_graph, window, cx| {
        assert_eq!(git_graph.graph_data.commits.len(), 1);
        git_graph.deploy_entry_context_menu(point(px(20.), px(20.)), 0, None, window, cx);
    });
    cx.run_until_parked();

    let context_menu = git_graph.read_with(&*cx, |git_graph, _| {
        git_graph
            .context_menu
            .as_ref()
            .expect("context menu should be open")
            .menu
            .clone()
    });
    context_menu.update_in(cx, |context_menu, window, cx| {
        context_menu
            .select_last(window, cx)
            .expect("custom Git task should be selectable");
        context_menu.confirm(&menu::Confirm, window, cx);
    });
    cx.run_until_parked();

    let (task_source_kind, resolved_task) = task_inventory.read_with(&*cx, |inventory, _| {
        inventory
            .last_scheduled_task(None)
            .expect("custom Git task should be scheduled")
    });

    assert!(
        matches!(task_source_kind, TaskSourceKind::AbsPath { .. }),
        "scheduled task should come from global tasks"
    );
    assert_eq!(resolved_task.resolved_label, "Git Show abcdef1");
    assert_eq!(resolved_task.resolved.command, Some("git".to_string()));
    assert_eq!(
        resolved_task.resolved.args,
        vec![
            "show".to_string(),
            "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        ]
    );
    assert_eq!(
        resolved_task.resolved.cwd,
        Some(Path::new("/project").to_path_buf())
    );
    assert_eq!(
        resolved_task.resolved.env.get("REPOSITORY"),
        Some(&"project".to_string())
    );
}

#[gpui::test]
async fn test_global_git_command_task_runs_from_ref_context_menu(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        Path::new("/project"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;

    let commit_sha = Oid::try_from("abcdef1234567890abcdef1234567890abcdef12")
        .expect("commit SHA should be valid");
    fs.set_graph_commits(
        Path::new("/project/.git"),
        vec![Arc::new(InitialGraphCommitData {
            sha: commit_sha,
            parents: SmallVec::new(),
            ref_names: vec!["HEAD -> feature-x".into()],
        })],
    );

    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    cx.run_until_parked();

    let repository = project.read_with(cx, |project, cx| {
        project
            .active_repository(cx)
            .expect("project should have an active repository")
    });
    let task_inventory = project.read_with(cx, |project, cx| {
        project
            .task_store()
            .read(cx)
            .task_inventory()
            .cloned()
            .expect("project should have a task inventory")
    });

    task_inventory.update(cx, |inventory, _| {
        inventory
            .update_file_based_tasks(
                TaskSettingsLocation::Global(Path::new("/tasks.json")),
                Some(
                    &serde_json::to_string(&json!([
                        {
                            "label": "Check out $MAV_GIT_REF",
                            "command": "git",
                            "args": ["checkout", "$MAV_GIT_REF"],
                            "cwd": "$MAV_GIT_REPOSITORY_PATH",
                            "tags": [GIT_COMMAND_TASK_TAG],
                        },
                    ]))
                    .expect("tasks JSON should serialize"),
                ),
            )
            .expect("tasks should parse");
    });

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        workspace::MultiWorkspace::test_new(project.clone(), window, cx)
    });
    let workspace = multi_workspace.read_with(&*cx, |multi_workspace, _| {
        multi_workspace.workspace().clone()
    });
    let workspace_weak = workspace.downgrade();

    let git_graph = cx.new_window_entity(|window, cx| {
        GitGraph::new(
            repository.read(cx).id,
            project.read(cx).git_store().clone(),
            workspace_weak,
            None,
            window,
            cx,
        )
    });
    workspace.update_in(cx, |workspace, window, cx| {
        workspace.add_item_to_active_pane(Box::new(git_graph.clone()), None, true, window, cx);
    });
    cx.run_until_parked();

    git_graph.update_in(cx, |git_graph, window, cx| {
        assert_eq!(git_graph.graph_data.commits.len(), 1);
        git_graph.deploy_entry_context_menu(
            point(px(20.), px(20.)),
            0,
            Some("feature-x".into()),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    let context_menu = git_graph.read_with(&*cx, |git_graph, _| {
        git_graph
            .context_menu
            .as_ref()
            .expect("context menu should be open")
            .menu
            .clone()
    });
    context_menu.update_in(cx, |context_menu, window, cx| {
        context_menu
            .select_last(window, cx)
            .expect("custom Git task should be selectable");
        context_menu.confirm(&menu::Confirm, window, cx);
    });
    cx.run_until_parked();

    let (_task_source_kind, resolved_task) = task_inventory.read_with(&*cx, |inventory, _| {
        inventory
            .last_scheduled_task(None)
            .expect("custom Git task should be scheduled")
    });

    assert_eq!(resolved_task.resolved_label, "Check out feature-x");
    assert_eq!(
        resolved_task.resolved.args,
        vec!["checkout".to_string(), "feature-x".to_string()]
    );
}
