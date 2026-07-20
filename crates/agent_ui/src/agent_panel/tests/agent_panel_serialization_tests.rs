use super::*;

#[gpui::test]
async fn test_active_thread_serialize_and_load_round_trip(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project_a", json!({ "file.txt": "" }))
        .await;
    let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
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

    let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    panel_a.update_in(cx, |panel, window, cx| {
        panel.open_external_thread_with_server(
            Rc::new(StubAgentServer::default_response()),
            window,
            cx,
        );
    });

    cx.run_until_parked();

    panel_a.read_with(cx, |panel, cx| {
        assert!(
            panel.active_agent_thread(cx).is_some(),
            "workspace A should have an active thread after connection"
        );
    });

    send_message(&panel_a, cx);

    let agent_type_a = panel_a.read_with(cx, |panel, _cx| panel.selected_agent.clone());

    let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    panel_b.update(cx, |panel, _cx| {
        panel.selected_agent = Agent::Custom {
            id: "claude-acp".into(),
        };
    });

    panel_a.update(cx, |panel, cx| panel.serialize(cx));
    panel_b.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();

    let workspace_a_id = workspace_a
        .read_with(cx, |workspace, _cx| workspace.database_id())
        .expect("workspace A should have a database id");
    let kvp = cx.update(|_window, cx| KeyValueStore::global(cx));
    let serialized_a: SerializedAgentPanel = cx
        .background_spawn(async move { read_serialized_panel(workspace_a_id, &kvp) })
        .await
        .expect("workspace A should serialize panel state");
    assert!(
        serialized_a.last_active_thread.is_some(),
        "active thread should be the thread restore target"
    );
    assert!(
        serialized_a.last_active_terminal_id.is_none(),
        "active thread serialization should not also include a terminal restore target"
    );

    cx.update(|_window, cx| {
        ThreadMetadataStore::init_global(cx);
    });

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
            panel.selected_agent, agent_type_a,
            "workspace A agent type should be restored"
        );
        assert!(
            panel.active_conversation_view().is_some(),
            "workspace A should have its active thread restored"
        );
    });

    loaded_b.read_with(cx, |panel, _cx| {
        assert_eq!(
            panel.selected_agent,
            Agent::Custom {
                id: "claude-acp".into()
            },
            "workspace B agent type should be restored"
        );
        assert!(
            panel.active_conversation_view().is_none(),
            "workspace B should have no active thread when it had no prior conversation"
        );
    });
}
