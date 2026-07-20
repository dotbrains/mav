use super::*;

#[gpui::test]
async fn test_archive_thread_uses_next_threads_own_workspace(cx: &mut TestAppContext) {
    // Regression test: archive_thread previously always loaded the next thread
    // through group_workspace (the main workspace's ProjectHeader), even when
    // the next thread belonged to an absorbed linked-worktree workspace. That
    // caused the worktree thread to be loaded in the main panel, which bound it
    // to the main project and corrupted its stored folder_paths.
    //
    // The fix: use next.workspace (ThreadEntryWorkspace::Open) when available,
    // falling back to group_workspace only for Closed workspaces.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    // Activate main workspace so the sidebar tracks the main panel.
    multi_workspace.update_in(cx, |mw, window, cx| {
        let workspace = mw.workspaces().next().unwrap().clone();
        mw.activate(workspace, None, window, cx);
    });

    let main_workspace =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    let main_panel = add_agent_panel(&main_workspace, cx);
    let _worktree_panel = add_agent_panel(&worktree_workspace, cx);

    // Open Thread 2 in the main panel and keep it running.
    let connection = StubAgentConnection::new();
    open_thread_with_connection(&main_panel, connection.clone(), cx);
    send_message(&main_panel, cx);

    let thread2_session_id = active_session_id(&main_panel, cx);

    cx.update(|_, cx| {
        connection.send_update(
            thread2_session_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("working...".into())),
            cx,
        );
    });

    // Save thread 2's metadata with a newer timestamp so it sorts above thread 1.
    save_thread_metadata(
        thread2_session_id.clone(),
        Some("Thread 2".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );

    // Save thread 1's metadata with the worktree path and an older timestamp so
    // it sorts below thread 2. archive_thread will find it as the "next" candidate.
    let thread1_session_id = acp::SessionId::new(Arc::from("thread1-worktree-session"));
    save_thread_metadata(
        thread1_session_id,
        Some("Thread 1".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );

    cx.run_until_parked();

    // Verify the sidebar absorbed thread 1 under [project] with the worktree chip.
    let entries_before = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries_before.iter().any(|s| s.contains("{wt-feature-a}")),
        "Thread 1 should appear with the linked-worktree chip before archiving: {:?}",
        entries_before
    );

    // The sidebar should track T2 as the focused thread (derived from the
    // main panel's active view).
    sidebar.read_with(cx, |s, _| {
        assert_active_thread(
            s,
            &thread2_session_id,
            "focused thread should be Thread 2 before archiving",
        );
    });

    // Archive thread 2.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&thread2_session_id, window, cx);
    });

    cx.run_until_parked();

    // The main panel's active thread must still be thread 2.
    let main_active = main_panel.read_with(cx, |panel, cx| {
        panel
            .active_agent_thread(cx)
            .map(|t| t.read(cx).session_id().clone())
    });
    assert_eq!(
        main_active,
        Some(thread2_session_id.clone()),
        "main panel should not have been taken over by loading the linked-worktree thread T1; \
             before the fix, archive_thread used group_workspace instead of next.workspace, \
             causing T1 to be loaded in the wrong panel"
    );

    // Thread 1 should still appear in the sidebar with its worktree chip
    // (Thread 2 was archived so it is gone from the list).
    let entries_after = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries_after.iter().any(|s| s.contains("{wt-feature-a}")),
        "T1 should still carry its linked-worktree chip after archiving T2: {:?}",
        entries_after
    );
}

#[gpui::test]
async fn test_archive_last_worktree_thread_removes_workspace(cx: &mut TestAppContext) {
    // When the last non-archived thread for a linked worktree is archived,
    // the linked worktree workspace should be removed from the multi-workspace.
    // The main worktree workspace should remain (it's always reachable via
    // the project header).
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;

    fs.insert_tree(
        "/worktrees/project/feature-a/project",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/project/feature-a/project"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "abc".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/project/feature-a/project"),
        None,
        cx,
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(
        fs.clone(),
        ["/worktrees/project/feature-a/project".as_ref()],
        cx,
    )
    .await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let _worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    // Save a thread for the main project.
    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );

    // Save a thread for the linked worktree.
    let wt_thread_id = acp::SessionId::new(Arc::from("worktree-thread"));
    save_thread_metadata(
        wt_thread_id.clone(),
        Some("Worktree Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // Should have 2 workspaces.
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "should start with 2 workspaces (main + linked worktree)"
    );

    // Archive the worktree thread (the only thread for /wt-feature-a).
    sidebar.update_in(cx, |sidebar: &mut Sidebar, window, cx| {
        sidebar.archive_thread(&wt_thread_id, window, cx);
    });

    // archive_thread spawns a multi-layered chain of tasks (workspace
    // removal → git persist → disk removal), each of which may spawn
    // further background work. Each run_until_parked() call drives one
    // layer of pending work.

    cx.run_until_parked();

    // The linked worktree workspace should have been removed.
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "linked worktree workspace should be removed after archiving its last thread"
    );

    // The linked worktree checkout directory should also be removed from disk.
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from disk after archiving its last thread"
    );

    // The main thread should still be visible.
    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries.iter().any(|e| e.contains("Main Thread")),
        "main thread should still be visible: {entries:?}"
    );
    assert!(
        !entries.iter().any(|e| e.contains("Worktree Thread")),
        "archived worktree thread should not be visible: {entries:?}"
    );

    // The archived thread must retain its folder_paths so it can be
    // restored to the correct workspace later.
    let wt_thread_id = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&wt_thread_id)
            .unwrap()
            .thread_id
    });
    let archived_paths = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(wt_thread_id)
            .unwrap()
            .folder_paths()
            .clone()
    });
    assert_eq!(
        archived_paths.paths(),
        &[PathBuf::from("/worktrees/project/feature-a/project")],
        "archived thread must retain its folder_paths for restore"
    );
}
