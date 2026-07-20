use super::*;

#[gpui::test]
async fn test_initialize_from_source_transfers_draft_to_fresh_panel(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project_a", json!({ "file.txt": "" }))
        .await;
    fs.insert_tree("/project_b", json!({ "file.txt": "" }))
        .await;
    let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
    let project_b = Project::test(fs.clone(), [Path::new("/project_b")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    let workspace_a = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    let workspace_b = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project_b.clone(), window, cx)
        })
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    panel_a.update_in(cx, |panel, window, cx| {
        panel.open_external_thread_with_server(
            Rc::new(StubAgentServer::default_response()),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    let thread_view_a = panel_a.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
    let editor_a = thread_view_a.read_with(cx, |view, _cx| view.message_editor.clone());
    editor_a.update_in(cx, |editor, window, cx| {
        editor.set_text("Draft from workspace A", window, cx);
    });

    let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    let transferred = panel_b.update_in(cx, |panel, window, cx| {
        panel.initialize_from_source_workspace_if_needed(workspace_a.downgrade(), window, cx)
    });
    assert!(
        transferred,
        "fresh destination panel should accept source content"
    );

    panel_b.read_with(cx, |panel, _cx| {
        assert!(
            panel.active_conversation_view().is_some(),
            "panel_b should have a conversation view after initialization"
        );
        assert!(
            panel.draft_thread.is_some(),
            "panel_b should have a draft_thread set after initialization"
        );
    });
}

#[gpui::test]
async fn test_initialize_from_source_inherits_agent_without_draft_content(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project_a", json!({ "file.txt": "" }))
        .await;
    fs.insert_tree("/project_b", json!({ "file.txt": "" }))
        .await;
    let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
    let project_b = Project::test(fs.clone(), [Path::new("/project_b")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    let workspace_a = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    let workspace_b = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project_b.clone(), window, cx)
        })
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel_a.update(cx, |panel, _cx| {
        panel.selected_agent = Agent::Stub;
    });

    let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    let initialized = panel_b.update_in(cx, |panel, window, cx| {
        panel.initialize_from_source_workspace_if_needed(workspace_a.downgrade(), window, cx)
    });
    assert!(
        initialized,
        "fresh destination panel should inherit the source agent"
    );

    panel_b.read_with(cx, |panel, _cx| {
        assert_eq!(
            panel.selected_agent,
            Agent::Stub,
            "destination panel should inherit the source panel's selected agent"
        );
        assert!(
            panel.active_conversation_view().is_none(),
            "agent-only initialization should not create a draft thread"
        );
    });
}

#[gpui::test]
async fn test_initialize_from_source_retargets_empty_destination_draft_agent(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    fs.insert_tree("/project_a", json!({ "file.txt": "" }))
        .await;
    fs.insert_tree("/project_b", json!({ "file.txt": "" }))
        .await;
    let project_a = Project::test(fs.clone(), [Path::new("/project_a")], cx).await;
    let project_b = Project::test(fs.clone(), [Path::new("/project_b")], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    let workspace_a = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();

    let workspace_b = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project_b.clone(), window, cx)
        })
        .unwrap();

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);

    let panel_a = workspace_a.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel_a.update(cx, |panel, _cx| {
        panel.selected_agent = Agent::Stub;
    });

    let panel_b = workspace_b.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    panel_b.update_in(cx, |panel, window, cx| {
        panel.activate_new_thread(false, AgentThreadSource::AgentPanel, window, cx);
    });

    let original_draft = panel_b.read_with(cx, |panel, cx| {
        let draft = panel.draft_thread.as_ref().expect("draft should exist");
        assert_eq!(
            *draft.read(cx).agent_key(),
            Agent::NativeAgent,
            "destination draft should start on the default agent"
        );
        draft.entity_id()
    });

    let initialized = panel_b.update_in(cx, |panel, window, cx| {
        panel.initialize_from_source_workspace_if_needed(workspace_a.downgrade(), window, cx)
    });
    assert!(
        initialized,
        "fresh destination draft should inherit the source agent"
    );

    panel_b.read_with(cx, |panel, cx| {
        let draft = panel.draft_thread.as_ref().expect("draft should exist");
        assert_ne!(
            draft.entity_id(),
            original_draft,
            "empty destination draft should be replaced when the inherited agent differs"
        );
        assert_eq!(
            *draft.read(cx).agent_key(),
            Agent::Stub,
            "empty destination draft should be rebound to the inherited agent"
        );
    });
}
