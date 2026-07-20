use super::*;

#[gpui::test]
async fn test_archive_removes_worktree_even_when_workspace_paths_diverge(cx: &mut TestAppContext) {
    // When the thread's folder_paths don't exactly match any workspace's
    // root paths (e.g. because a folder was added to the workspace after
    // the thread was created), workspace_to_remove is None. But the linked
    // worktree workspace still needs to be removed so that its worktree
    // entities are released, allowing git worktree removal to proceed.
    //
    // With the fix, archive_thread scans roots_to_archive for any linked
    // worktree workspaces and includes them in the removal set, even when
    // the thread's folder_paths don't match the workspace's root paths.
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
            "src": {
                "main.rs": "fn main() {}",
            },
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

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    // Save thread metadata using folder_paths that DON'T match the
    // workspace's root paths. This simulates the case where the workspace's
    // paths diverged (e.g. a folder was added after thread creation).
    // This causes workspace_to_remove to be None because
    // workspace_for_paths can't find a workspace with these exact paths.
    let wt_thread_id = acp::SessionId::new(Arc::from("worktree-thread"));
    save_thread_metadata_with_main_paths(
        "worktree-thread",
        "Worktree Thread",
        PathList::new(&[
            PathBuf::from("/worktrees/project/feature-a/project"),
            PathBuf::from("/nonexistent"),
        ]),
        PathList::new(&[PathBuf::from("/project"), PathBuf::from("/nonexistent")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        cx,
    );

    // Also save a main thread so the sidebar has something to show.
    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "should start with 2 workspaces (main + linked worktree)"
    );

    // Archive the worktree thread.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&wt_thread_id, window, cx);
    });

    cx.run_until_parked();

    // The linked worktree workspace should have been removed, even though
    // workspace_to_remove was None (paths didn't match).
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "linked worktree workspace should be removed after archiving, \
         even when folder_paths don't match workspace root paths"
    );

    // The thread should still be archived (not unarchived due to an error).
    let still_archived = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&wt_thread_id)
            .map(|t| t.archived)
    });
    assert_eq!(
        still_archived,
        Some(true),
        "thread should still be archived (not rolled back due to error)"
    );

    // The linked worktree directory should be removed from disk.
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from disk"
    );
}
