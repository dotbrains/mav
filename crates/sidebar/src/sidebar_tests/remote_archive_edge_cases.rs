use super::*;

#[gpui::test]
async fn test_remote_linked_worktree_workspace_to_remove_uses_remote_connection(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    init_test(cx);

    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        workspace::init(app_state.clone(), cx);
        app_state
    });

    let server_fs = FakeFs::new(server_cx.executor());
    server_fs
        .insert_tree(
            "/project",
            serde_json::json!({
                ".git": {},
                "src": {},
            }),
        )
        .await;
    server_fs
        .insert_tree(
            "/external-worktree",
            serde_json::json!({
                ".git": "gitdir: /project/.git/worktrees/feature-a",
                "src": {},
            }),
        )
        .await;
    server_fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    server_fs.insert_branches(Path::new("/project/.git"), &["main", "feature-a"]);
    server_fs
        .add_linked_worktree_for_repo(
            Path::new("/project/.git"),
            false,
            git::repository::Worktree {
                path: PathBuf::from("/external-worktree"),
                ref_name: Some("refs/heads/feature-a".into()),
                sha: "abc".into(),
                is_main: false,
                is_bare: false,
            },
        )
        .await;

    let (worktree_project, _headless, remote_connection) = start_remote_project(
        &server_fs,
        Path::new("/external-worktree"),
        &app_state,
        None,
        cx,
        server_cx,
    )
    .await;
    worktree_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    cx.update(|cx| <dyn fs::Fs>::set_global(app_state.fs.clone(), cx));

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        MultiWorkspace::test_new(worktree_project.clone(), window, cx)
    });
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_session_id = acp::SessionId::new(Arc::from("remote-worktree-thread"));
    let worktree_folder_paths = PathList::new(&[PathBuf::from("/external-worktree")]);
    let main_folder_paths = PathList::new(&[PathBuf::from("/project")]);
    let worktree_thread_id = ThreadId::new();
    cx.update(|_window, cx| {
        let metadata = ThreadMetadata {
            thread_id: worktree_thread_id,
            session_id: Some(worktree_session_id.clone()),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title: Some("Remote Worktree Thread".into()),
            title_override: None,
            updated_at: chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
            created_at: None,
            interacted_at: None,
            worktree_paths: WorktreePaths::from_path_lists(
                main_folder_paths,
                worktree_folder_paths.clone(),
            )
            .unwrap(),
            archived: false,
            remote_connection: Some(remote_connection.clone()),
        };
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();

    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(
                    &worktree_folder_paths,
                    Some(&remote_connection),
                    cx,
                )
            })
            .is_some(),
        "remote linked-worktree workspace should be open before archiving"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "the test must exercise a remote-only workspace lookup"
    );
    assert_ne!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace().read(cx).project_group_key(cx)
            })
            .path_list(),
        &worktree_folder_paths,
        "remote workspace must be classified as a linked worktree under the main project"
    );

    let workspace_to_remove = sidebar.read_with(cx, |sidebar, cx| {
        sidebar
            .linked_worktree_workspace_to_remove(
                &worktree_folder_paths,
                Some(&remote_connection),
                Some(worktree_thread_id),
                None,
                &[],
                cx,
            )
            .map(|workspace| workspace.entity_id())
    });
    let active_workspace_id = multi_workspace.read_with(cx, |multi_workspace, _cx| {
        multi_workspace.workspace().entity_id()
    });
    assert_eq!(
        workspace_to_remove,
        Some(active_workspace_id),
        "archive helper should resolve the remote linked-worktree workspace"
    );
    assert!(
        server_fs.is_dir(Path::new("/external-worktree")).await,
        "direct helper check should not remove the linked worktree from disk"
    );
}

#[gpui::test]
async fn test_remote_archive_thread_with_disconnected_remote(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    // When a remote thread has no linked-worktree state to archive (only
    // a main worktree), archival is a pure metadata operation: no RPCs
    // are issued against the remote server. This must succeed even when
    // the connection has dropped out, because losing connectivity should
    // not block users from cleaning up their thread list.
    //
    // Threads that *do* have linked-worktree state require a live
    // connection to run the git worktree removal on the server; that
    // path is covered by `test_remote_archive_thread_with_active_connection`.
    init_test(cx);

    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        workspace::init(app_state.clone(), cx);
        app_state
    });

    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let server_fs = FakeFs::new(server_cx.executor());
    server_fs
        .insert_tree(
            "/project",
            serde_json::json!({
                ".git": {},
                "src": { "main.rs": "fn main() {}" },
            }),
        )
        .await;
    server_fs.set_branch_name(Path::new("/project/.git"), Some("main"));

    let (project, _headless, _opts) = start_remote_project(
        &server_fs,
        Path::new("/project"),
        &app_state,
        None,
        cx,
        server_cx,
    )
    .await;
    let remote_client = project
        .read_with(cx, |project, _cx| project.remote_client())
        .expect("remote project should expose its client");

    cx.update(|cx| <dyn fs::Fs>::set_global(app_state.fs.clone(), cx));

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let thread_id = acp::SessionId::new(Arc::from("remote-thread"));
    save_thread_metadata(
        thread_id.clone(),
        Some("Remote Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();

    // Sanity-check: there is nothing on the remote fs outside the main
    // repo, so archival should not need to touch the server.
    assert!(
        !server_fs.is_dir(Path::new("/worktrees")).await,
        "no linked worktrees on the server before archiving"
    );

    // Disconnect the remote connection before archiving. We don't
    // `run_until_parked` here because the disconnect itself triggers
    // reconnection work that can't complete in the test environment.
    remote_client.update(cx, |client, cx| {
        client.simulate_disconnect(cx).detach();
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&thread_id, window, cx);
    });
    cx.run_until_parked();

    let is_archived = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&thread_id)
            .map(|t| t.archived)
            .unwrap_or(false)
    });
    assert!(
        is_archived,
        "thread should be archived even when remote is disconnected"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries.iter().any(|e| e.contains("Remote Thread")),
        "archived thread should be hidden from sidebar: {entries:?}"
    );
}
