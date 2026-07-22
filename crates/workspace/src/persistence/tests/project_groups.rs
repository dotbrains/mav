use super::*;

#[gpui::test]
async fn test_restore_window_with_linked_worktree_and_multiple_project_groups(
    cx: &mut gpui::TestAppContext,
) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());

    // Main git repo at /repo
    fs.insert_tree(
        "/repo",
        json!({
            ".git": {
                "HEAD": "ref: refs/heads/main",
                "worktrees": {
                    "feature": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    // Linked worktree checkout pointing back to /repo
    fs.insert_tree(
        "/worktree-feature",
        json!({
            ".git": "gitdir: /repo/.git/worktrees/feature",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    // --- Phase 1: Set up the original multi-workspace window ---

    let project_1 = Project::test(fs.clone(), ["/repo".as_ref()], cx).await;
    let project_1_linked_worktree =
        Project::test(fs.clone(), ["/worktree-feature".as_ref()], cx).await;

    // Wait for git discovery to finish.
    cx.run_until_parked();

    // Create a second, unrelated project so we have two distinct project groups.
    fs.insert_tree(
        "/other-project",
        json!({
            ".git": { "HEAD": "ref: refs/heads/main" },
            "readme.md": ""
        }),
    )
    .await;
    let project_2 = Project::test(fs.clone(), ["/other-project".as_ref()], cx).await;
    cx.run_until_parked();

    // Create the MultiWorkspace with project_2, then add the main repo
    // and its linked worktree. The linked worktree is added last and
    // becomes the active workspace.
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_2.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| {
        mw.open_sidebar(cx);
    });

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_1.clone(), window, cx);
    });

    let workspace_worktree = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_1_linked_worktree.clone(), window, cx)
    });

    let tasks =
        multi_workspace.update_in(cx, |mw, window, cx| mw.flush_all_serialization(window, cx));
    cx.run_until_parked();
    for task in tasks {
        task.await;
    }
    cx.run_until_parked();

    let active_db_id = workspace_worktree.read_with(cx, |ws, _| ws.database_id());
    assert!(
        active_db_id.is_some(),
        "Active workspace should have a database ID"
    );

    // --- Phase 2: Read back and verify the serialized state ---

    let session_id = multi_workspace
        .read_with(cx, |mw, cx| mw.workspace().read(cx).session_id())
        .unwrap();
    let db = cx.update(|_, cx| WorkspaceDb::global(cx));
    let session_workspaces = db
        .last_session_workspace_locations(&session_id, None, fs.as_ref())
        .await
        .expect("should load session workspaces");
    assert!(
        !session_workspaces.is_empty(),
        "Should have at least one session workspace"
    );

    let multi_workspaces =
        cx.update(|_, cx| read_serialized_multi_workspaces(session_workspaces, cx));
    assert_eq!(
        multi_workspaces.len(),
        1,
        "All workspaces share one window, so there should be exactly one multi-workspace"
    );

    let serialized = &multi_workspaces[0];
    assert_eq!(
        serialized.active_workspace.workspace_id,
        active_db_id.unwrap(),
    );
    assert_eq!(serialized.state.project_groups.len(), 2,);

    // Verify the serialized project group keys round-trip back to the
    // originals.
    let restored_keys: Vec<ProjectGroupKey> = serialized
        .state
        .project_groups
        .iter()
        .cloned()
        .map(Into::into)
        .collect();
    let expected_keys = vec![
        ProjectGroupKey::new(None, PathList::new(&["/repo"])),
        ProjectGroupKey::new(None, PathList::new(&["/other-project"])),
    ];
    assert_eq!(
        restored_keys, expected_keys,
        "Deserialized project group keys should match the originals"
    );

    // --- Phase 3: Restore the window and verify the result ---

    let app_state =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).app_state().clone());

    let serialized_mw = multi_workspaces.into_iter().next().unwrap();
    let restored_handle: gpui::WindowHandle<MultiWorkspace> = cx
        .update(|_, cx| {
            cx.spawn(async move |mut cx| {
                crate::restore_multiworkspace(serialized_mw, app_state, &mut cx).await
            })
        })
        .await
        .expect("restore_multiworkspace should succeed");

    cx.run_until_parked();

    // The restored window should have the same project group keys.
    let restored_keys: Vec<ProjectGroupKey> = restored_handle
        .read_with(cx, |mw: &MultiWorkspace, _cx| mw.project_group_keys())
        .unwrap();
    assert_eq!(
        restored_keys, expected_keys,
        "Restored window should have the same project group keys as the original"
    );

    // The active workspace in the restored window should have the linked
    // worktree paths.
    let active_paths: Vec<PathBuf> = restored_handle
        .read_with(cx, |mw: &MultiWorkspace, cx| {
            mw.workspace()
                .read(cx)
                .root_paths(cx)
                .into_iter()
                .map(|p: Arc<Path>| p.to_path_buf())
                .collect()
        })
        .unwrap();
    assert_eq!(
        active_paths,
        vec![PathBuf::from("/worktree-feature")],
        "The restored active workspace should be the linked worktree project"
    );
}

#[gpui::test]
async fn test_remove_project_group_falls_back_to_neighbor(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let dir_a = unique_test_dir(&fs, "group-a").await;
    let dir_b = unique_test_dir(&fs, "group-b").await;
    let dir_c = unique_test_dir(&fs, "group-c").await;

    let project_a = Project::test(fs.clone(), [dir_a.as_path()], cx).await;
    let project_b = Project::test(fs.clone(), [dir_b.as_path()], cx).await;
    let project_c = Project::test(fs.clone(), [dir_c.as_path()], cx).await;

    // Create a multi-workspace with project A, then add B and C.
    // project_groups stores newest first: [C, B, A].
    // Sidebar displays in the same order: C (top), B (middle), A (bottom).
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| mw.open_sidebar(cx));

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _workspace_c = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_c.clone(), window, cx)
    });
    cx.run_until_parked();

    let key_a = project_a.read_with(cx, |p, cx| p.project_group_key(cx));
    let key_b = project_b.read_with(cx, |p, cx| p.project_group_key(cx));
    let key_c = project_c.read_with(cx, |p, cx| p.project_group_key(cx));

    // Activate workspace B so removing its group exercises the fallback.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_b.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // --- Remove group B (the middle one). ---
    // In the sidebar [C, B, A], "below" B is A.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove_project_group(&key_b, window, cx)
            .detach_and_log_err(cx);
    });
    cx.run_until_parked();

    let active_paths =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).root_paths(cx));
    assert_eq!(
        active_paths
            .iter()
            .map(|p| p.to_path_buf())
            .collect::<Vec<_>>(),
        vec![dir_a.clone()],
        "After removing the middle group, should fall back to the group below (A)"
    );

    // After removing B, keys = [A, C], sidebar = [C, A].
    // Activate workspace A (the bottom) so removing it tests the
    // "fall back upward" path.
    let workspace_a =
        multi_workspace.read_with(cx, |mw, _cx| mw.workspaces().next().unwrap().clone());
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // --- Remove group A (the bottom one in sidebar). ---
    // Nothing below A, so should fall back upward to C.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove_project_group(&key_a, window, cx)
            .detach_and_log_err(cx);
    });
    cx.run_until_parked();

    let active_paths =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).root_paths(cx));
    assert_eq!(
        active_paths
            .iter()
            .map(|p| p.to_path_buf())
            .collect::<Vec<_>>(),
        vec![dir_c.clone()],
        "After removing the bottom group, should fall back to the group above (C)"
    );

    // --- Remove group C (the only one remaining). ---
    // Should create an empty workspace.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove_project_group(&key_c, window, cx)
            .detach_and_log_err(cx);
    });
    cx.run_until_parked();

    let active_paths =
        multi_workspace.read_with(cx, |mw, cx| mw.workspace().read(cx).root_paths(cx));
    assert!(
        active_paths.is_empty(),
        "After removing the only remaining group, should have an empty workspace"
    );
}

/// Regression test for a crash where `find_or_create_local_workspace`
/// returned a workspace that was about to be removed, hitting an assert
/// in `MultiWorkspace::remove`.
///
/// The scenario: two workspaces share the same root paths (e.g. due to
/// a provisional key mismatch). When the first is removed and the
/// fallback searches for the same paths, `workspace_for_paths` must
/// skip the doomed workspace so the assert in `remove` is satisfied.
#[gpui::test]
async fn test_remove_fallback_skips_excluded_workspaces(cx: &mut gpui::TestAppContext) {
    crate::tests::init_test(cx);

    let fs = fs::FakeFs::new(cx.executor());
    let dir = unique_test_dir(&fs, "shared").await;

    // Two projects that open the same directory — this creates two
    // workspaces whose root_paths are identical.
    let project_a = Project::test(fs.clone(), [dir.as_path()], cx).await;
    let project_b = Project::test(fs.clone(), [dir.as_path()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));

    multi_workspace.update(cx, |mw, cx| mw.open_sidebar(cx));

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    cx.run_until_parked();

    // workspace_a is first in the workspaces vec.
    let workspace_a =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().cloned().unwrap());
    assert_ne!(workspace_a, workspace_b);

    // Activate workspace_a so removing it triggers the fallback path.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // Remove workspace_a. The fallback searches for the same paths.
    // Without the `excluding` parameter, `workspace_for_paths` would
    // return workspace_a (first match) and the assert in `remove`
    // would fire. With the fix, workspace_a is skipped and
    // workspace_b is found instead.
    let path_list = PathList::new(std::slice::from_ref(&dir));
    let excluded = vec![workspace_a.clone()];
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove(
            vec![workspace_a.clone()],
            move |this, window, cx| {
                this.find_or_create_local_workspace(
                    path_list,
                    None,
                    &excluded,
                    None,
                    OpenMode::Activate,
                    window,
                    cx,
                )
            },
            window,
            cx,
        )
        .detach_and_log_err(cx);
    });
    cx.run_until_parked();

    // workspace_b should now be active — workspace_a was removed.
    multi_workspace.read_with(cx, |mw, _cx| {
        assert_eq!(
            mw.workspace(),
            &workspace_b,
            "fallback should have found workspace_b, not the excluded workspace_a"
        );
    });
}
