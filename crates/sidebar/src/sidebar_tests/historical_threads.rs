use super::*;

#[gpui::test]
async fn test_confirm_on_historical_thread_activates_workspace(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.create_test_workspace(window, cx).detach();
    });
    cx.run_until_parked();

    let (workspace_0, workspace_1) = multi_workspace.read_with(cx, |mw, _| {
        (
            mw.workspaces().next().unwrap().clone(),
            mw.workspaces().nth(1).unwrap().clone(),
        )
    });

    save_thread_metadata(
        acp::SessionId::new(Arc::from("hist-1")),
        Some("Historical Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 6, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Historical Thread",
        ]
    );

    // Switch to workspace 1 so we can verify the confirm switches back.
    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().nth(1).unwrap().clone();
        mw.activate(workspace, None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_1
    );

    // Confirm on the historical (non-live) thread at index 1.
    // Before a previous fix, the workspace field was Option<usize> and
    // historical threads had None, so activate_thread early-returned
    // without switching the workspace.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.selection = Some(1);
        sidebar.confirm(&Confirm, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_0
    );
}

#[gpui::test]
async fn test_confirm_on_historical_thread_preserves_historical_timestamp_and_order(
    cx: &mut TestAppContext,
) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let newer_session_id = acp::SessionId::new(Arc::from("newer-historical-thread"));
    let newer_timestamp = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 6, 2, 0, 0, 0).unwrap();
    save_thread_metadata(
        newer_session_id,
        Some("Newer Historical Thread".into()),
        newer_timestamp,
        Some(newer_timestamp),
        None,
        &project,
        cx,
    );

    let older_session_id = acp::SessionId::new(Arc::from("older-historical-thread"));
    let older_timestamp = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 6, 1, 0, 0, 0).unwrap();
    save_thread_metadata(
        older_session_id.clone(),
        Some("Older Historical Thread".into()),
        older_timestamp,
        Some(older_timestamp),
        None,
        &project,
        cx,
    );

    cx.run_until_parked();
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    let historical_entries_before: Vec<_> = visible_entries_as_strings(&sidebar, cx)
        .into_iter()
        .filter(|entry| entry.contains("Historical Thread"))
        .collect();
    assert_eq!(
        historical_entries_before,
        vec![
            "  Newer Historical Thread".to_string(),
            "  Older Historical Thread".to_string(),
        ],
        "expected the sidebar to sort historical threads by their saved timestamp before activation"
    );

    let older_entry_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| {
                matches!(entry, ListEntry::Thread(thread)
                    if thread.metadata.session_id.as_ref() == Some(&older_session_id))
            })
            .expect("expected Older Historical Thread to appear in the sidebar")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.selection = Some(older_entry_index);
        sidebar.confirm(&Confirm, window, cx);
    });
    cx.run_until_parked();

    let older_metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&older_session_id)
            .cloned()
            .expect("expected metadata for Older Historical Thread after activation")
    });
    assert_eq!(
        older_metadata.created_at,
        Some(older_timestamp),
        "activating a historical thread should not rewrite its saved created_at timestamp"
    );

    let historical_entries_after: Vec<_> = visible_entries_as_strings(&sidebar, cx)
        .into_iter()
        .filter(|entry| entry.contains("Historical Thread"))
        .collect();
    assert_eq!(
        historical_entries_after,
        vec![
            "  Newer Historical Thread".to_string(),
            "  Older Historical Thread  <== selected".to_string(),
        ],
        "activating an older historical thread should not reorder it ahead of a newer historical thread"
    );
}

#[gpui::test]
async fn test_confirm_on_historical_thread_in_new_project_group_opens_real_thread(
    cx: &mut TestAppContext,
) {
    use workspace::ProjectGroup;

    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let project_b_key = project_b.read_with(cx, |project, cx| project.project_group_key(cx));
    multi_workspace.update(cx, |mw, _cx| {
        mw.test_add_project_group(ProjectGroup {
            key: project_b_key.clone(),
            workspaces: Vec::new(),
            expanded: true,
        });
    });

    let session_id = acp::SessionId::new(Arc::from("historical-new-project-group"));
    save_thread_metadata(
        session_id.clone(),
        Some("Historical Thread in New Group".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 6, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project_b,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    let entries_before = visible_entries_as_strings(&sidebar, cx);
    assert_eq!(
        entries_before,
        vec![
            "v [project-a]",
            "v [project-b]",
            "  Historical Thread in New Group",
        ],
        "expected the closed project group to show the historical thread before first open"
    );

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "should start without an open workspace for the new project group"
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.selection = Some(2);
        sidebar.confirm(&Confirm, window, cx);
    });

    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "confirming the historical thread should open a workspace for the new project group"
    );

    let workspace_b = multi_workspace.read_with(cx, |mw, cx| {
        mw.workspaces()
            .find(|workspace| {
                PathList::new(&workspace.read(cx).root_paths(cx))
                    == project_b_key.path_list().clone()
            })
            .cloned()
            .expect("expected workspace for project-b after opening the historical thread")
    });

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_b,
        "opening the historical thread should activate the new project's workspace"
    );

    let panel = workspace_b.read_with(cx, |workspace, cx| {
        workspace
            .panel::<AgentPanel>(cx)
            .expect("expected first-open activation to bootstrap the agent panel")
    });

    let expected_thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("metadata should still map session id to thread id")
    });

    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.active_thread_id(cx)),
        Some(expected_thread_id),
        "expected the agent panel to activate the real historical thread rather than a draft"
    );

    let entries_after = visible_entries_as_strings(&sidebar, cx);
    let matching_rows: Vec<_> = entries_after
        .iter()
        .filter(|entry| entry.contains("Historical Thread in New Group") || entry.contains("Draft"))
        .cloned()
        .collect();
    assert_eq!(
        matching_rows.len(),
        1,
        "expected only one matching row after first open into a new project group, got entries: {entries_after:?}"
    );
    assert!(
        matching_rows[0].contains("Historical Thread in New Group"),
        "expected the surviving row to be the real historical thread, got entries: {entries_after:?}"
    );
    assert!(
        !matching_rows[0].contains("Draft"),
        "expected no draft row after first open into a new project group, got entries: {entries_after:?}"
    );
}
