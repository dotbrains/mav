use super::*;

#[gpui::test]
async fn test_non_native_thread_without_metadata_is_not_restored(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

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
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.open_external_thread_with_server(
            Rc::new(StubAgentServer::default_response()),
            window,
            cx,
        );
    });

    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert!(
            panel.active_agent_thread(cx).is_some(),
            "should have an active thread after connection"
        );
    });

    panel.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();

    let async_cx = cx.update(|window, cx| window.to_async(cx));
    let loaded = AgentPanel::load(workspace.downgrade(), async_cx)
        .await
        .expect("panel load should succeed");
    cx.run_until_parked();

    loaded.read_with(cx, |panel, _cx| {
        assert!(
            panel.active_conversation_view().is_none(),
            "thread without metadata should not be restored; the panel should have no active thread"
        );
    });
}

#[gpui::test]
async fn test_serialize_preserves_session_id_in_load_error(cx: &mut TestAppContext) {
    use crate::conversation_view::tests::FlakyAgentServer;
    use crate::thread_metadata_store::ThreadMetadata;
    use project::AgentId as ProjectAgentId;

    init_test(cx);
    cx.update(|cx| {
        agent::ThreadStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
    });

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let multi_workspace =
        cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace
        .read_with(cx, |mw, _cx| mw.workspace().clone())
        .unwrap();
    workspace.update(cx, |workspace, _cx| {
        workspace.set_random_database_id();
    });
    let workspace_id = workspace
        .read_with(cx, |workspace, _cx| workspace.database_id())
        .expect("workspace should have a database id");

    let cx = &mut VisualTestContext::from_window(multi_workspace.into(), cx);
    let resume_session_id = acp::SessionId::new("persistent-session");
    cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(resume_session_id.clone()),
                    agent_id: ProjectAgentId::new("Flaky"),
                    title: Some("Persistent chat".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_folder_paths(&PathList::default()),
                    remote_connection: None,
                    archived: false,
                },
                cx,
            );
        });
    });

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        cx.new(|cx| AgentPanel::new(workspace, window, cx))
    });

    let (server, _fail) =
        FlakyAgentServer::new(StubAgentConnection::new().with_supports_load_session(true));
    panel.update_in(cx, |panel, window, cx| {
        panel.open_restored_thread_with_server(
            Rc::new(server),
            resume_session_id.clone(),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, cx| {
        assert!(
            panel.active_agent_thread(cx).is_none(),
            "active_agent_thread should be None while the flaky server is failing"
        );
        let conversation_view = panel
            .active_conversation_view()
            .expect("panel should still have an active ConversationView");
        assert_eq!(
            conversation_view.read(cx).root_session_id.as_ref(),
            Some(&resume_session_id),
            "ConversationView should still hold the restored session id"
        );
    });

    panel.update(cx, |panel, cx| panel.serialize(cx));
    cx.run_until_parked();

    let kvp = cx.update(|_window, cx| KeyValueStore::global(cx));
    let serialized: Option<SerializedAgentPanel> = cx
        .background_spawn(async move { read_serialized_panel(workspace_id, &kvp) })
        .await;
    let serialized_session_id = serialized
        .as_ref()
        .and_then(|p| p.last_active_thread.as_ref())
        .and_then(|t| t.session_id.clone());
    assert_eq!(
        serialized_session_id,
        Some(resume_session_id.0.to_string()),
        "serialize() must preserve the restored session id even while the \
         ConversationView is in LoadError; otherwise the bug survives a \
         restart because the KVP has been wiped"
    );
}
