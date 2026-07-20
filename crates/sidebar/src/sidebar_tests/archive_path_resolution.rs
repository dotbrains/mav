use super::*;

#[gpui::test]
async fn test_activate_archived_thread_with_saved_paths_activates_matching_workspace(
    cx: &mut TestAppContext,
) {
    // Thread has saved metadata in ThreadStore. A matching workspace is
    // already open. Expected: activates the matching workspace.
    init_test(cx);
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

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let workspace_a =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());

    // Save a thread with path_list pointing to project-b.
    let session_id = acp::SessionId::new(Arc::from("archived-1"));
    save_test_thread_metadata(&session_id, &project_b, cx).await;

    // Ensure workspace A is active.
    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().next().unwrap().clone();
        mw.activate(workspace, None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_a
    );

    // Call activate_archived_thread – should resolve saved paths and
    // switch to the workspace for project-b.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(
            ThreadMetadata {
                thread_id: ThreadId::new(),
                session_id: Some(session_id.clone()),
                agent_id: agent::MAV_AGENT_ID.clone(),
                title: Some("Archived Thread".into()),
                title_override: None,
                updated_at: Utc::now(),
                created_at: None,
                interacted_at: None,
                worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
                    "/project-b",
                )])),
                archived: false,
                remote_connection: None,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_b,
        "should have switched to the workspace matching the saved paths"
    );
}

#[gpui::test]
async fn test_activate_archived_thread_cwd_fallback_with_matching_workspace(
    cx: &mut TestAppContext,
) {
    // Thread has no saved metadata but session_info has cwd. A matching
    // workspace is open. Expected: uses cwd to find and activate it.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b, window, cx)
    });
    let workspace_a =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());

    // Start with workspace A active.
    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().next().unwrap().clone();
        mw.activate(workspace, None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_a
    );

    // No thread saved to the store – cwd is the only path hint.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(
            ThreadMetadata {
                thread_id: ThreadId::new(),
                session_id: Some(acp::SessionId::new(Arc::from("unknown-session"))),
                agent_id: agent::MAV_AGENT_ID.clone(),
                title: Some("CWD Thread".into()),
                title_override: None,
                updated_at: Utc::now(),
                created_at: None,
                interacted_at: None,
                worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[
                    std::path::PathBuf::from("/project-b"),
                ])),
                archived: false,
                remote_connection: None,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_b,
        "should have activated the workspace matching the cwd"
    );
}

#[gpui::test]
async fn test_activate_archived_thread_no_paths_no_cwd_uses_active_workspace(
    cx: &mut TestAppContext,
) {
    // Thread has no saved metadata and no cwd. Expected: falls back to
    // the currently active workspace.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b, window, cx)
    });

    // Activate workspace B (index 1) to make it the active one.
    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().nth(1).unwrap().clone();
        mw.activate(workspace, None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_b
    );

    // No saved thread, no cwd – should fall back to the active workspace.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(
            ThreadMetadata {
                thread_id: ThreadId::new(),
                session_id: Some(acp::SessionId::new(Arc::from("no-context-session"))),
                agent_id: agent::MAV_AGENT_ID.clone(),
                title: Some("Contextless Thread".into()),
                title_override: None,
                updated_at: Utc::now(),
                created_at: None,
                interacted_at: None,
                worktree_paths: WorktreePaths::default(),
                archived: false,
                remote_connection: None,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_b,
        "should have stayed on the active workspace when no path info is available"
    );
}

#[gpui::test]
async fn test_activate_archived_thread_saved_paths_opens_new_workspace(cx: &mut TestAppContext) {
    // Thread has saved metadata pointing to a path with no open workspace.
    // Expected: opens a new workspace for that path.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Save a thread with path_list pointing to project-b – which has no
    // open workspace.
    let path_list_b = PathList::new(&[std::path::PathBuf::from("/project-b")]);
    let session_id = acp::SessionId::new(Arc::from("archived-new-ws"));

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "should start with one workspace"
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(
            ThreadMetadata {
                thread_id: ThreadId::new(),
                session_id: Some(session_id.clone()),
                agent_id: agent::MAV_AGENT_ID.clone(),
                title: Some("New WS Thread".into()),
                title_override: None,
                updated_at: Utc::now(),
                created_at: None,
                interacted_at: None,
                worktree_paths: WorktreePaths::from_folder_paths(&path_list_b),
                archived: false,
                remote_connection: None,
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "should have opened a second workspace for the archived thread's saved paths"
    );
}
