use super::*;

#[gpui::test]
async fn test_activating_workspace_with_draft_does_not_create_extras(cx: &mut TestAppContext) {
    // When a workspace has a draft (from the panel's load fallback)
    // and the user activates it (e.g. by clicking the placeholder or
    // the project header), no extra drafts should be created.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a =
        project::Project::test(fs.clone() as Arc<dyn Fs>, ["/project-a".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    let workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let _panel_a = add_agent_panel(&workspace_a, cx);
    cx.run_until_parked();

    // Add project-b with its own workspace and agent panel.
    let project_b =
        project::Project::test(fs.clone() as Arc<dyn Fs>, ["/project-b".as_ref()], cx).await;
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    // Explicitly create a draft on workspace_b so the sidebar tracks one.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.create_new_thread(&workspace_b, window, cx);
    });
    cx.run_until_parked();

    // Count project-b's drafts.
    let count_b_drafts = |cx: &mut gpui::VisualTestContext| {
        let entries = visible_entries_as_strings(&sidebar, cx);
        entries
            .iter()
            .skip_while(|e| !e.contains("project-b"))
            .take_while(|e| !e.starts_with("v ") || e.contains("project-b"))
            .filter(|e| e.contains("Draft"))
            .count()
    };
    let drafts_before = count_b_drafts(cx);

    // Switch away from project-b, then back.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_b.clone(), None, window, cx);
    });
    cx.run_until_parked();

    let drafts_after = count_b_drafts(cx);
    assert_eq!(
        drafts_before, drafts_after,
        "activating workspace should not create extra drafts"
    );

    // The draft should be highlighted as active after switching back.
    sidebar.read_with(cx, |sidebar, _| {
        assert_active_draft(
            sidebar,
            &workspace_b,
            "draft should be active after switching back to its workspace",
        );
    });
}

#[gpui::test]
async fn test_non_archive_thread_paths_migrate_on_worktree_add_and_remove(cx: &mut TestAppContext) {
    // Historical threads (not open in any agent panel) should have their
    // worktree paths updated when a folder is added to or removed from the
    // project.
    let (_fs, project) = init_multi_project_test(&["/project-a", "/project-b"], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Save two threads directly into the metadata store (not via the agent
    // panel), so they are purely historical — no open views hold them.
    // Use different timestamps so sort order is deterministic.
    save_thread_metadata(
        acp::SessionId::new(Arc::from("hist-1")),
        Some("Historical 1".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    save_thread_metadata(
        acp::SessionId::new(Arc::from("hist-2")),
        Some("Historical 2".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 1).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    // Sanity-check: both threads exist under the initial key [/project-a].
    let old_key_paths = PathList::new(&[PathBuf::from("/project-a")]);
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store
                .entries_for_main_worktree_path(&old_key_paths, None)
                .count(),
            2,
            "should have 2 historical threads under old key before worktree add"
        );
    });

    // Add a second worktree to the project.
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    // The historical threads should now be indexed under the new combined
    // key [/project-a, /project-b].
    let new_key_paths = PathList::new(&[PathBuf::from("/project-a"), PathBuf::from("/project-b")]);
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store
                .entries_for_main_worktree_path(&old_key_paths, None)
                .count(),
            0,
            "should have 0 historical threads under old key after worktree add"
        );
        assert_eq!(
            store
                .entries_for_main_worktree_path(&new_key_paths, None)
                .count(),
            2,
            "should have 2 historical threads under new key after worktree add"
        );
    });

    // Sidebar should show threads under the new header.
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            "v [project-a, project-b]",
            "  Historical 2",
            "  Historical 1",
        ]
    );

    // Now remove the second worktree.
    let worktree_id = project.read_with(cx, |project, cx| {
        project
            .visible_worktrees(cx)
            .find(|wt| wt.read(cx).abs_path().as_ref() == Path::new("/project-b"))
            .map(|wt| wt.read(cx).id())
            .expect("should find project-b worktree")
    });
    project.update(cx, |project, cx| {
        project.remove_worktree(worktree_id, cx);
    });
    cx.run_until_parked();

    // Historical threads should migrate back to the original key.
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store
                .entries_for_main_worktree_path(&new_key_paths, None)
                .count(),
            0,
            "should have 0 historical threads under new key after worktree remove"
        );
        assert_eq!(
            store
                .entries_for_main_worktree_path(&old_key_paths, None)
                .count(),
            2,
            "should have 2 historical threads under old key after worktree remove"
        );
    });

    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [project-a]", "  Historical 2", "  Historical 1",]
    );
}
