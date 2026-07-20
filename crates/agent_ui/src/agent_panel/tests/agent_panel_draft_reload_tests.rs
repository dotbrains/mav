use super::*;

#[gpui::test]
async fn test_new_draft_survives_reload_when_real_thread_is_active(cx: &mut TestAppContext) {
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
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();
    workspace.update(cx, |workspace, _cx| workspace.set_random_database_id());

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    let stub_connection =
        crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
    stub_connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("ok".into()),
    )]);

    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::Stub;
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();
    crate::test_support::send_message(&panel, cx);
    let real_thread_id = crate::test_support::active_thread_id(&panel, cx);
    let real_session_id = crate::test_support::active_session_id(&panel, cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();
    let retained_draft_id = crate::test_support::active_thread_id(&panel, cx);
    crate::test_support::type_draft_prompt(&panel, "retained draft text", cx);

    panel.update_in(cx, |panel, window, cx| {
        panel.new_thread(&NewThread, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert!(panel.retained_threads.contains_key(&retained_draft_id));
        assert_ne!(panel.active_thread_id(cx), Some(retained_draft_id));
    });

    let draft_thread_id = crate::test_support::active_thread_id(&panel, cx);
    crate::test_support::type_draft_prompt(&panel, "in-flight draft text", cx);

    let (ephemeral_kvp, retained_kvp) = cx.update(|_, cx| {
        (
            crate::draft_prompt_store::read(draft_thread_id, cx),
            crate::draft_prompt_store::read(retained_draft_id, cx),
        )
    });
    assert!(ephemeral_kvp.is_some());
    assert!(retained_kvp.is_some());

    panel.update_in(cx, |panel, window, cx| {
        panel.load_agent_thread(
            Agent::Stub,
            real_thread_id,
            None,
            None,
            false,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    panel.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();
    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let loaded_panel = AgentPanel::load(workspace.downgrade(), async_cx)
        .await
        .expect("panel load should succeed");
    cx.run_until_parked();

    loaded_panel.read_with(cx, |panel, cx| {
        assert_eq!(panel.active_thread_id(cx), Some(real_thread_id));
        assert!(!panel.active_thread_is_draft(cx));
        assert!(panel.draft_thread.is_none());
    });

    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert!(store.entry(draft_thread_id).unwrap().is_draft());
        assert!(store.entry(retained_draft_id).unwrap().is_draft());
        let real_row = store.entry(real_thread_id).unwrap();
        assert_eq!(real_row.session_id.as_ref(), Some(&real_session_id));
    });

    loaded_panel.update_in(cx, |panel, window, cx| {
        panel.load_agent_thread(
            Agent::Stub,
            draft_thread_id,
            None,
            None,
            false,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let restored_ephemeral_text =
        loaded_panel.read_with(cx, |panel, cx| panel.editor_text(draft_thread_id, cx));
    assert_eq!(
        restored_ephemeral_text.as_deref(),
        Some("in-flight draft text")
    );

    loaded_panel.update_in(cx, |panel, window, cx| {
        panel.load_agent_thread(
            Agent::Stub,
            retained_draft_id,
            None,
            None,
            false,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let restored_retained_text =
        loaded_panel.read_with(cx, |panel, cx| panel.editor_text(retained_draft_id, cx));
    assert_eq!(
        restored_retained_text.as_deref(),
        Some("retained draft text")
    );
}

#[gpui::test]
async fn test_reloaded_ephemeral_draft_preserves_original_agent(cx: &mut TestAppContext) {
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
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();
    workspace.update(cx, |workspace, _cx| workspace.set_random_database_id());

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });

    let _stub_connection =
        crate::test_support::set_stub_agent_connection(StubAgentConnection::new());
    panel.update_in(cx, |panel, window, cx| {
        panel.selected_agent = Agent::Stub;
        panel.activate_draft(true, AgentThreadSource::AgentPanel, window, cx);
    });
    cx.run_until_parked();

    let draft_thread_id = crate::test_support::active_thread_id(&panel, cx);
    crate::test_support::type_draft_prompt(&panel, "pinned to stub", cx);
    panel.update(cx, |panel, _cx| {
        panel.selected_agent = Agent::Custom {
            id: "other-agent".into(),
        };
    });
    panel.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();

    cx.update(|_, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let row = store.entry(draft_thread_id).unwrap();
        assert_eq!(row.agent_id.as_ref(), "stub");
    });

    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let reloaded_panel = AgentPanel::load(workspace.downgrade(), async_cx)
        .await
        .expect("panel load should succeed");
    cx.run_until_parked();

    reloaded_panel.read_with(cx, |panel, cx| {
        let draft_view = panel.draft_thread.as_ref().unwrap();
        assert_eq!(draft_view.read(cx).thread_id, draft_thread_id);
        assert_eq!(draft_view.read(cx).agent_key(), &Agent::Stub);
    });
}
