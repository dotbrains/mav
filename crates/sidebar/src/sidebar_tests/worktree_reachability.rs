use super::*;

#[gpui::test]
async fn test_worktree_add_only_regroups_threads_for_changed_workspace(cx: &mut TestAppContext) {
    // When two workspaces share the same project group (same main path)
    // but have different folder paths (main repo vs linked worktree),
    // adding a worktree to the main workspace should regroup only that
    // workspace and its threads into the new project group. Threads for the
    // linked worktree workspace should remain under the original group.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-feature"),
            ref_name: Some("refs/heads/feature".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    // Workspace A: main repo at /project.
    let main_project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, ["/project".as_ref()], cx).await;
    // Workspace B: linked worktree of the same repo (same group, different folder).
    let worktree_project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, ["/wt-feature".as_ref()], cx).await;

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
        mw.test_add_workspace(worktree_project.clone(), window, cx);
    });
    cx.run_until_parked();

    // Save a thread for each workspace's folder paths.
    let time_main = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 1).unwrap();
    let time_wt = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 2).unwrap();
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-main")),
        Some("Main Thread".into()),
        time_main,
        Some(time_main),
        None,
        &main_project,
        cx,
    );
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-wt")),
        Some("Worktree Thread".into()),
        time_wt,
        Some(time_wt),
        None,
        &worktree_project,
        cx,
    );
    cx.run_until_parked();

    let folder_paths_main = PathList::new(&[PathBuf::from("/project")]);
    let folder_paths_wt = PathList::new(&[PathBuf::from("/wt-feature")]);

    // Sanity-check: each thread is indexed under its own folder paths, but
    // both appear under the shared sidebar group keyed by the main worktree.
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store.entries_for_path(&folder_paths_main, None).count(),
            1,
            "one thread under [/project]"
        );
        assert_eq!(
            store.entries_for_path(&folder_paths_wt, None).count(),
            1,
            "one thread under [/wt-feature]"
        );
    });
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            "v [project]",
            "  Worktree Thread {wt-feature}",
            "  Main Thread",
        ]
    );

    // Add /project-b to the main project only.
    main_project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    // Main Thread (folder paths [/project]) should be regrouped to
    // [/project, /project-b]. Worktree Thread should remain under the
    // original [/project] group.
    let folder_paths_main_b =
        PathList::new(&[PathBuf::from("/project"), PathBuf::from("/project-b")]);
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store.entries_for_path(&folder_paths_main, None).count(),
            0,
            "main thread should no longer be under old folder paths [/project]"
        );
        assert_eq!(
            store.entries_for_path(&folder_paths_main_b, None).count(),
            1,
            "main thread should now be under [/project, /project-b]"
        );
        assert_eq!(
            store.entries_for_path(&folder_paths_wt, None).count(),
            1,
            "worktree thread should remain unchanged under [/wt-feature]"
        );
    });

    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            "v [project]",
            "  Worktree Thread {wt-feature}",
            "v [project, project-b]",
            "  Main Thread",
        ]
    );
}

#[gpui::test]
async fn test_linked_worktree_workspace_reachable_after_adding_worktree_to_project(
    cx: &mut TestAppContext,
) {
    // When a linked worktree is opened as its own workspace and then a new
    // folder is added to the main project group, the linked worktree
    // workspace must still be reachable from some sidebar entry.
    let (_fs, project) = init_multi_project_test(&["/my-project"], cx).await;
    let fs = _fs.clone();

    // Set up git worktree infrastructure.
    fs.insert_tree(
        "/my-project/.git/worktrees/wt-0",
        serde_json::json!({
            "commondir": "../../",
            "HEAD": "ref: refs/heads/wt-0",
        }),
    )
    .await;
    fs.insert_tree(
        "/worktrees/wt-0",
        serde_json::json!({
            ".git": "gitdir: /my-project/.git/worktrees/wt-0",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/my-project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/wt-0"),
            ref_name: Some("refs/heads/wt-0".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    // Re-scan so the main project discovers the linked worktree.
    project.update(cx, |p, cx| p.git_scans_complete(cx)).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Open the linked worktree as its own workspace.
    let worktree_project = project::Project::test(
        fs.clone() as Arc<dyn fs::Fs>,
        ["/worktrees/wt-0".as_ref()],
        cx,
    )
    .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx);
    });
    cx.run_until_parked();

    // Both workspaces should be reachable.
    let workspace_count = multi_workspace.read_with(cx, |mw, _| mw.workspaces().count());
    assert_eq!(workspace_count, 2, "should have 2 workspaces");

    // Add a new folder to the main project, changing the project group key.
    fs.insert_tree(
        "/other-project",
        serde_json::json!({ ".git": {}, "src": {} }),
    )
    .await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/other-project", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    // The linked worktree workspace must still be reachable.
    let entries = visible_entries_as_strings(&sidebar, cx);
    let mw_workspaces: Vec<_> = multi_workspace.read_with(cx, |mw, _| {
        mw.workspaces().map(|ws| ws.entity_id()).collect()
    });
    sidebar.read_with(cx, |sidebar, cx| {
        let multi_workspace = multi_workspace.read(cx);
        let reachable: std::collections::HashSet<gpui::EntityId> = sidebar
            .contents
            .entries
            .iter()
            .flat_map(|entry| entry.reachable_workspaces(multi_workspace, cx))
            .map(|ws| ws.entity_id())
            .collect();
        let all: std::collections::HashSet<gpui::EntityId> =
            mw_workspaces.iter().copied().collect();
        let unreachable = &all - &reachable;
        assert!(
            unreachable.is_empty(),
            "all workspaces should be reachable after adding folder; \
             unreachable: {:?}, entries: {:?}",
            unreachable,
            entries,
        );
    });
}
