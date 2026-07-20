use super::*;

#[gpui::test]
async fn test_collab_guest_move_thread_paths_is_noop(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project = project::Project::test(fs, ["/project-a".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    // Set up the sidebar while the project is local. This registers the
    // WorktreePathsChanged subscription for the project.
    let _sidebar = setup_sidebar(&multi_workspace, cx);

    let session_id = acp::SessionId::new(Arc::from("test-thread"));
    save_named_thread_metadata("test-thread", "My Thread", &project, cx).await;

    let thread_id = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&session_id)
            .map(|e| e.thread_id)
            .expect("thread must be in the store")
    });

    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx);
        let entry = store.read(cx).entry(thread_id).unwrap();
        assert_eq!(
            entry.folder_paths().paths(),
            &[PathBuf::from("/project-a")],
            "thread must be saved with /project-a before collab"
        );
    });

    // Transition the project into collab mode. The sidebar's subscription is
    // still active from when the project was local.
    project.update(cx, |project, _cx| {
        project.mark_as_collab_for_testing();
    });

    // Adding a worktree fires WorktreePathsChanged with old_paths = {/project-a}.
    // The sidebar's subscription is still active, so move_thread_paths is called.
    // Without the is_via_collab() guard inside move_thread_paths, this would
    // update the stored thread paths from {/project-a} to {/project-a, /project-b}.
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx);
        let entry = store
            .read(cx)
            .entry(thread_id)
            .expect("thread must still exist");
        assert_eq!(
            entry.folder_paths().paths(),
            &[PathBuf::from("/project-a")],
            "thread path must not change when project is via collab"
        );
    });
}

#[gpui::test]
async fn test_cmd_click_project_header_returns_to_last_active_linked_worktree_workspace(
    cx: &mut TestAppContext,
) {
    // Regression test for: cmd-clicking a project group header should return
    // the user to the workspace they most recently had active in that group,
    // including workspaces rooted at a linked worktree.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project-a",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project-a/.git"),
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

    let main_project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let worktree_project_a =
        project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    main_project_a
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project_a
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    // The multi-workspace starts with the main-paths workspace of group A
    // as the initially active workspace.
    let (multi_workspace, cx) = cx
        .add_window_view(|window, cx| MultiWorkspace::test_new(main_project_a.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Capture the initially active workspace (group A's main-paths workspace)
    // *before* registering additional workspaces, since `workspaces()` returns
    // retained workspaces in registration order — not activation order — and
    // the multi-workspace's starting workspace may not be retained yet.
    let main_workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    // Register the linked-worktree workspace (group A) and the group-B
    // workspace. Both get retained by the multi-workspace.
    let worktree_workspace_a = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project_a.clone(), window, cx)
    });
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });

    cx.run_until_parked();

    // Step 1: activate the linked-worktree workspace. The MultiWorkspace
    // records this as the last-active workspace for group A on its
    // ProjectGroupState. (We don't assert on the initial active workspace
    // because `test_add_workspace` may auto-activate newly registered
    // workspaces — what matters for this test is the explicit sequence of
    // activations below.)
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(worktree_workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        worktree_workspace_a,
        "linked-worktree workspace should be active after step 1"
    );

    // Step 2: switch to the workspace for group B. Group A's last-active
    // workspace remains the linked-worktree one (group B getting activated
    // records *its own* last-active workspace, not group A's).
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_b.clone(), None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_b,
        "group B's workspace should be active after step 2"
    );

    // Step 3: simulate cmd-click on group A's header. The project group key
    // for group A is derived from the *main-paths* workspace (linked-worktree
    // workspaces share the same key because it normalizes to main-worktree
    // paths).
    let group_a_key = main_workspace_a.read_with(cx, |ws, cx| ws.project_group_key(cx));
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.activate_or_open_workspace_for_group(&group_a_key, window, cx);
    });
    cx.run_until_parked();

    // Expected: we're back in the linked-worktree workspace, not the
    // main-paths one.
    let active_after_cmd_click = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    assert_eq!(
        active_after_cmd_click, worktree_workspace_a,
        "cmd-click on group A's header should return to the last-active \
         linked-worktree workspace, not the main-paths workspace"
    );
    assert_ne!(
        active_after_cmd_click, main_workspace_a,
        "cmd-click must not fall back to the main-paths workspace when a \
         linked-worktree workspace was the last-active one for the group"
    );
}
