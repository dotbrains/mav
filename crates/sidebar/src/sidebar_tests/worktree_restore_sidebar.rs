use super::*;

#[gpui::test]
async fn test_restore_worktree_thread_uses_main_repo_project_group_key(cx: &mut TestAppContext) {
    // Activating an archived linked worktree thread whose directory has
    // been deleted should reuse the existing main repo workspace, not
    // create a new one. The provisional ProjectGroupKey must be derived
    // from main_worktree_paths so that find_or_create_local_workspace
    // matches the main repo workspace when the worktree path is absent.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-c": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-c",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;

    fs.insert_tree(
        "/wt-feature-c",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-c",
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/wt-feature-c"),
            ref_name: Some("refs/heads/feature-c".into()),
            sha: "original-sha".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-c".as_ref()], cx).await;

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

    // Save thread metadata for the linked worktree.
    let wt_session_id = acp::SessionId::new(Arc::from("wt-thread-c"));
    save_thread_metadata(
        wt_session_id.clone(),
        Some("Worktree Thread C".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );
    cx.run_until_parked();

    let thread_id = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&wt_session_id)
            .unwrap()
            .thread_id
    });

    // Archive the thread without creating ArchivedGitWorktree records.
    let store = cx.update(|_window, cx| ThreadMetadataStore::global(cx));
    cx.update(|_window, cx| {
        store.update(cx, |store, cx| store.archive(thread_id, None, cx));
    });
    cx.run_until_parked();

    // Remove the worktree workspace and delete the worktree from disk.
    let main_workspace =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    let remove_task = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove(
            vec![worktree_workspace],
            move |_this, _window, _cx| Task::ready(Ok(main_workspace)),
            window,
            cx,
        )
    });
    remove_task.await.ok();
    cx.run_until_parked();
    cx.run_until_parked();
    fs.remove_dir(
        Path::new("/wt-feature-c"),
        fs::RemoveOptions {
            recursive: true,
            ignore_if_not_exists: true,
        },
    )
    .await
    .unwrap();

    let workspace_count_before = multi_workspace.read_with(cx, |mw, _| mw.workspaces().count());
    assert_eq!(
        workspace_count_before, 1,
        "should have only the main workspace"
    );

    // Activate the archived thread. The worktree path is missing from
    // disk, so find_or_create_local_workspace falls back to the
    // provisional ProjectGroupKey to find a matching workspace.
    let metadata = cx.update(|_window, cx| store.read(cx).entry(thread_id).unwrap().clone());
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });
    cx.run_until_parked();

    // The provisional key should use [/project] (the main repo),
    // which matches the existing main workspace. If it incorrectly
    // used [/wt-feature-c] (the linked worktree path), no workspace
    // would match and a spurious new one would be created.
    let workspace_count_after = multi_workspace.read_with(cx, |mw, _| mw.workspaces().count());
    assert_eq!(
        workspace_count_after, 1,
        "restoring a linked worktree thread should reuse the main repo workspace, \
         not create a new one (workspace count went from {workspace_count_before} to \
         {workspace_count_after})"
    );
}

#[gpui::test]
async fn test_archive_last_worktree_thread_not_blocked_by_remote_thread_at_same_path(
    cx: &mut TestAppContext,
) {
    // A remote thread at the same path as a local linked worktree thread
    // should not prevent the local workspace from being removed when the
    // local thread is archived (the last local thread for that worktree).
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
        "/wt-feature-a",
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
            path: PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "abc".into(),
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

    // Save a local thread for the linked worktree.
    let wt_thread_id = acp::SessionId::new(Arc::from("worktree-thread"));
    save_thread_metadata(
        wt_thread_id.clone(),
        Some("Local Worktree Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );

    // Save a remote thread at the same /wt-feature-a path but on a
    // different host. This should NOT count as a remaining thread for
    // the local linked worktree workspace.
    let remote_host =
        remote::RemoteConnectionOptions::Mock(remote::MockConnectionOptions { id: 99 });
    cx.update(|_window, cx| {
        let metadata = ThreadMetadata {
            thread_id: ThreadId::new(),
            session_id: Some(acp::SessionId::new(Arc::from("remote-wt-thread"))),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title: Some("Remote Worktree Thread".into()),
            title_override: None,
            updated_at: chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
            created_at: None,
            interacted_at: None,
            worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
                "/wt-feature-a",
            )])),
            archived: false,
            remote_connection: Some(remote_host),
        };
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "should start with 2 workspaces (main + linked worktree)"
    );

    // The remote thread should NOT appear in the sidebar (it belongs
    // to a different host and no matching remote project group exists).
    let entries_before = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries_before
            .iter()
            .any(|e| e.contains("Remote Worktree Thread")),
        "remote thread should not appear in local sidebar: {entries_before:?}"
    );

    // Archive the local worktree thread.
    sidebar.update_in(cx, |sidebar: &mut Sidebar, window, cx| {
        sidebar.archive_thread(&wt_thread_id, window, cx);
    });

    cx.run_until_parked();

    // The linked worktree workspace should be removed because the
    // only *local* thread for it was archived. The remote thread at
    // the same path should not have prevented removal.
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "linked worktree workspace should be removed; the remote thread at the same path \
         should not count as a remaining local thread"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries.iter().any(|e| e.contains("Main Thread")),
        "main thread should still be visible: {entries:?}"
    );
    assert!(
        !entries.iter().any(|e| e.contains("Local Worktree Thread")),
        "archived local worktree thread should not be visible: {entries:?}"
    );
    assert!(
        !entries.iter().any(|e| e.contains("Remote Worktree Thread")),
        "remote thread should still not appear in local sidebar: {entries:?}"
    );
}

#[gpui::test]
async fn test_linked_worktree_threads_not_duplicated_across_groups(cx: &mut TestAppContext) {
    // When a multi-root workspace (e.g. [/other, /project]) shares a
    // repo with a single-root workspace (e.g. [/project]), linked
    // worktree threads from the shared repo should only appear under
    // the dedicated group [project], not under [other, project].
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });
    let fs = FakeFs::new(cx.executor());

    // Two independent repos, each with their own git history.
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/other",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    // Register the linked worktree in the main repo.
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

    // Workspace 1: just /project.
    let project_only = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    project_only
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    // Workspace 2: /other and /project together (multi-root).
    let multi_root =
        project::Project::test(fs.clone(), ["/other".as_ref(), "/project".as_ref()], cx).await;
    multi_root
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    // Save a thread under the linked worktree path BEFORE setting up
    // the sidebar and panels, so that reconciliation sees the [project]
    // group as non-empty and doesn't create a spurious draft there.
    let wt_session_id = acp::SessionId::new(Arc::from("wt-thread"));
    save_thread_metadata(
        wt_session_id,
        Some("Worktree Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_only.clone(), window, cx));
    let (sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    let multi_root_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(multi_root.clone(), window, cx)
    });
    add_agent_panel(&multi_root_workspace, cx);
    cx.run_until_parked();

    // The thread should appear only under [project] (the dedicated
    // group for the /project repo), not under [other, project].
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [other, project]",
            "v [project]",
            "  Worktree Thread {wt-feature-a}",
        ]
    );
}

fn thread_id_for(session_id: &acp::SessionId, cx: &mut TestAppContext) -> ThreadId {
    cx.read(|cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(session_id)
            .map(|m| m.thread_id)
            .expect("thread metadata should exist")
    })
}
