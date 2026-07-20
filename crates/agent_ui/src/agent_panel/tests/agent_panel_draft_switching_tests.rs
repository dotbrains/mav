use super::*;

#[gpui::test]
async fn test_plus_with_parked_draft_active_focuses_ephemeral(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();
    workspace.update(cx, |workspace, _cx| workspace.set_random_database_id());
    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::Stub;
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();
    let parked_thread_id = crate::test_support::active_thread_id(&panel, cx);
    crate::test_support::type_draft_prompt(&panel, "parked draft prompt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    let ephemeral_thread_id = crate::test_support::active_thread_id(&panel, cx);
    let ephemeral_entity_id = panel.read_with(cx, |panel, _cx| {
        panel.draft_thread.as_ref().unwrap().entity_id()
    });
    assert_ne!(
        ephemeral_thread_id, parked_thread_id,
        "sanity: parking should have produced a fresh ephemeral draft"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.load_agent_thread(
            Agent::Stub,
            parked_thread_id,
            None,
            None,
            true,
            AgentThreadSource::Sidebar,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(
        crate::test_support::active_thread_id(&panel, cx),
        parked_thread_id,
        "sanity: parked draft should be the active view after load_agent_thread"
    );
    panel.read_with(cx, |panel, _cx| {
        assert_eq!(
            panel.draft_thread.as_ref().unwrap().entity_id(),
            ephemeral_entity_id,
            "ephemeral draft slot should still hold the fresh draft"
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert_eq!(
            panel.active_thread_id(cx),
            Some(ephemeral_thread_id),
            "`+` should have switched back to the existing ephemeral draft"
        );
        assert_eq!(
            panel.draft_thread.as_ref().unwrap().entity_id(),
            ephemeral_entity_id,
            "`+` should not have replaced the ephemeral draft"
        );
        assert!(
            panel.retained_threads.contains_key(&parked_thread_id),
            "parked draft should remain in `retained_threads`"
        );
    });
}

#[gpui::test]
async fn test_new_external_agent_replaces_mismatched_ephemeral_draft(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

    fs.insert_tree("/project", json!({ "file.txt": "" })).await;
    let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |multi_workspace, _cx| {
            multi_workspace.workspace().clone()
        })
        .unwrap();
    workspace.update(cx, |workspace, _cx| workspace.set_random_database_id());
    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::Stub;
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();
    let parked_thread_id = crate::test_support::active_thread_id(&panel, cx);
    crate::test_support::type_draft_prompt(&panel, "parked prompt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    let ephemeral_thread_id = crate::test_support::active_thread_id(&panel, cx);
    assert_ne!(ephemeral_thread_id, parked_thread_id);
    panel.read_with(cx, |panel, cx| {
        assert_eq!(
            panel.draft_thread.as_ref().unwrap().read(cx).agent_key(),
            &Agent::Stub,
            "ephemeral draft should be Stub agent"
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.load_agent_thread(
            Agent::Stub,
            parked_thread_id,
            None,
            None,
            true,
            AgentThreadSource::Sidebar,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(
        crate::test_support::active_thread_id(&panel, cx),
        parked_thread_id,
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::NativeAgent;
        panel.activate_new_thread(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        let draft = panel.draft_thread.as_ref().expect("draft should exist");
        assert_eq!(
            draft.read(cx).agent_key(),
            &Agent::NativeAgent,
            "ephemeral draft should be bound to NativeAgent, not Stub"
        );
        let active_id = panel.active_thread_id(cx).unwrap();
        assert_ne!(
            active_id, ephemeral_thread_id,
            "old Stub ephemeral draft should have been replaced"
        );
        assert!(
            panel.retained_threads.contains_key(&parked_thread_id),
            "parked draft should still be in retained_threads"
        );
    });
}

#[gpui::test]
async fn test_typed_draft_is_parked_when_switching_agents(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        <dyn fs::Fs>::set_global(fs.clone(), cx);
    });

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

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.open_draft_with_server(
            Rc::new(StubAgentServer::new(StubAgentConnection::new())),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    let initial_draft_id = panel.read_with(cx, |panel, _cx| {
        panel.draft_thread.as_ref().unwrap().entity_id()
    });
    let initial_thread_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());

    let thread_view = panel.read_with(cx, |panel, cx| panel.active_thread_view(cx).unwrap());
    let message_editor = thread_view.read_with(cx, |view, _cx| view.message_editor.clone());
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("saved prompt", window, cx);
    });

    cx.dispatch_action(NewExternalAgentThread {
        agent: Agent::Stub.id(),
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        let draft = panel.draft_thread.as_ref().expect("draft should exist");
        assert_ne!(
            draft.entity_id(),
            initial_draft_id,
            "a new draft should have been created for the new agent"
        );
        assert_eq!(
            *draft.read(cx).agent_key(),
            Agent::Stub,
            "new draft should use the new agent"
        );
        assert!(
            panel.retained_threads.contains_key(&initial_thread_id),
            "typed draft should have been parked into retained_threads"
        );
    });

    let parked_text = panel.read_with(cx, |panel, cx| panel.editor_text(initial_thread_id, cx));
    assert_eq!(
        parked_text.as_deref(),
        Some("saved prompt"),
        "parked draft should retain the user's prompt"
    );

    let active_thread_id = panel.read_with(cx, |panel, cx| panel.active_thread_id(cx).unwrap());
    let active_text = panel.read_with(cx, |panel, cx| panel.editor_text(active_thread_id, cx));
    assert_eq!(
        active_text, None,
        "new draft on the new agent should start empty, not carry the parked draft's prompt"
    );
}
