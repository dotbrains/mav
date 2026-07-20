use super::*;

#[gpui::test]
async fn test_new_workspace_inherits_global_last_used_agent(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        cx.set_global(db::AppDatabase::test_new());
    });

    let custom_agent = Agent::Custom {
        id: "my-preferred-agent".into(),
    };

    let kvp = cx.update(|cx| KeyValueStore::global(cx));
    write_global_last_used_agent(kvp, custom_agent.clone()).await;

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs.clone(), [], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();

    workspace.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let panel = AgentPanel::load(workspace.downgrade(), async_cx)
        .await
        .expect("panel load should succeed");
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert_eq!(
            panel.selected_agent, custom_agent,
            "new workspace should inherit the global last-used agent"
        );
    });
}

#[gpui::test]
async fn test_workspaces_maintain_independent_agent_selection(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let project_a = Project::test(fs.clone(), [], cx).await;
    let project_b = Project::test(fs, [], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    let workspace_a = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();

    let workspace_b = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project_b.clone(), window, cx)
        })
        .unwrap();

    workspace_a.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });
    workspace_b.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let agent_a = Agent::Custom {
        id: "agent-alpha".into(),
    };
    let agent_b = Agent::Custom {
        id: "agent-beta".into(),
    };

    let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });
    panel_a.update(cx, |panel, _cx| {
        panel.selected_agent = agent_a.clone();
    });

    let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });
    panel_b.update(cx, |panel, _cx| {
        panel.selected_agent = agent_b.clone();
    });

    panel_a.update(cx, |panel, cx| panel.serialize(cx));
    panel_b.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();

    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let loaded_a = AgentPanel::load(workspace_a.downgrade(), async_cx)
        .await
        .expect("panel A load should succeed");
    cx.run_until_parked();

    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let loaded_b = AgentPanel::load(workspace_b.downgrade(), async_cx)
        .await
        .expect("panel B load should succeed");
    cx.run_until_parked();

    loaded_a.read_with(cx, |panel, _cx| {
        assert_eq!(
            panel.selected_agent, agent_a,
            "workspace A should restore agent-alpha, not agent-beta"
        );
    });

    loaded_b.read_with(cx, |panel, _cx| {
        assert_eq!(
            panel.selected_agent, agent_b,
            "workspace B should restore agent-beta, not agent-alpha"
        );
    });
}

#[gpui::test]
async fn test_new_thread_uses_workspace_selected_agent(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();

    workspace.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let custom_agent = Agent::Custom {
        id: "my-custom-agent".into(),
    };

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel.update(cx, |panel, _cx| {
        panel.selected_agent = custom_agent.clone();
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });

    panel.read_with(cx, |panel, _cx| {
        assert_eq!(
            panel.selected_agent, custom_agent,
            "selected_agent should remain the custom agent after new_thread"
        );
        assert!(
            panel.active_conversation_view().is_some(),
            "a thread should have been created"
        );
    });
}
