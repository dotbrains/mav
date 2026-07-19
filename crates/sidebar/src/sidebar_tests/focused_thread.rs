use super::*;

#[gpui::test]
async fn test_focused_thread_tracks_user_intent(cx: &mut TestAppContext) {
    let project_a = init_test_project_with_agent_panel("/project-a", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let (sidebar, panel_a) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Save a thread so it appears in the list.
    let connection_a = StubAgentConnection::new();
    connection_a.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel_a, connection_a, cx);
    send_message(&panel_a, cx);
    let session_id_a = active_session_id(&panel_a, cx);
    save_test_thread_metadata(&session_id_a, &project_a, cx).await;

    // Add a second workspace with its own agent panel.
    let fs = cx.update(|_, cx| <dyn fs::Fs>::global(cx));
    fs.as_fake()
        .insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    let project_b = project::Project::test(fs, ["/project-b".as_ref()], cx).await;
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    let workspace_a =
        multi_workspace.read_with(cx, |mw, _cx| mw.workspaces().next().unwrap().clone());

    // ── 1. Initial state: focused thread derived from active panel ─────
    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id_a,
            "The active panel's thread should be focused on startup",
        );
    });

    let thread_metadata_a = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&session_id_a)
            .cloned()
            .expect("session_id_a should exist in metadata store")
    });
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.activate_thread(thread_metadata_a, &workspace_a, false, window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id_a,
            "After clicking a thread, it should be the focused thread",
        );
        assert!(
            has_thread_entry(sidebar, &session_id_a),
            "The clicked thread should be present in the entries"
        );
    });

    workspace_a.read_with(cx, |workspace, cx| {
        assert!(
            workspace.panel::<AgentPanel>(cx).is_some(),
            "Agent panel should exist"
        );
        let dock = workspace.left_dock().read(cx);
        assert!(
            dock.is_open(),
            "Clicking a thread should open the agent panel dock"
        );
    });

    let connection_b = StubAgentConnection::new();
    connection_b.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Thread B".into()),
    )]);
    open_thread_with_connection(&panel_b, connection_b, cx);
    send_message(&panel_b, cx);
    let session_id_b = active_session_id(&panel_b, cx);
    save_test_thread_metadata(&session_id_b, &project_b, cx).await;
    cx.run_until_parked();

    // Workspace A is currently active. Click a thread in workspace B,
    // which also triggers a workspace switch.
    let thread_metadata_b = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&session_id_b)
            .cloned()
            .expect("session_id_b should exist in metadata store")
    });
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.activate_thread(thread_metadata_b, &workspace_b, false, window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id_b,
            "Clicking a thread in another workspace should focus that thread",
        );
        assert!(
            has_thread_entry(sidebar, &session_id_b),
            "The cross-workspace thread should be present in the entries"
        );
    });

    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().next().unwrap().clone();
        mw.activate(workspace, None, window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id_a,
            "Switching workspace should seed focused_thread from the new active panel",
        );
        assert!(
            has_thread_entry(sidebar, &session_id_a),
            "The seeded thread should be present in the entries"
        );
    });

    let connection_b2 = StubAgentConnection::new();
    connection_b2.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new(DEFAULT_THREAD_TITLE.into()),
    )]);
    open_thread_with_connection(&panel_b, connection_b2, cx);
    send_message(&panel_b, cx);
    let session_id_b2 = active_session_id(&panel_b, cx);
    save_test_thread_metadata(&session_id_b2, &project_b, cx).await;
    cx.run_until_parked();

    // Panel B is not the active workspace's panel (workspace A is
    // active), so opening a thread there should not change focused_thread.
    // This prevents running threads in background workspaces from causing
    // the selection highlight to jump around.
    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id_a,
            "Opening a thread in a non-active panel should not change focused_thread",
        );
    });

    workspace_b.update_in(cx, |workspace, window, cx| {
        workspace.focus_handle(cx).focus(window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id_a,
            "Defocusing the sidebar should not change focused_thread",
        );
    });

    // Switching workspaces via the multi_workspace (simulates clicking
    // a workspace header) should clear focused_thread.
    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().find(|w| *w == &workspace_b).cloned();
        if let Some(workspace) = workspace {
            mw.activate(workspace, None, window, cx);
        }
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id_b2,
            "Switching workspace should seed focused_thread from the new active panel",
        );
        assert!(
            has_thread_entry(sidebar, &session_id_b2),
            "The seeded thread should be present in the entries"
        );
    });

    // ── 8. Focusing the agent panel thread keeps focused_thread ────
    // Workspace B still has session_id_b2 loaded in the agent panel.
    // Clicking into the thread (simulated by focusing its view) should
    // keep focused_thread since it was already seeded on workspace switch.
    panel_b.update_in(cx, |panel, window, cx| {
        if let Some(thread_view) = panel.active_conversation_view() {
            thread_view.read(cx).focus_handle(cx).focus(window, cx);
        }
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id_b2,
            "Focusing the agent panel thread should set focused_thread",
        );
        assert!(
            has_thread_entry(sidebar, &session_id_b2),
            "The focused thread should be present in the entries"
        );
    });
}
