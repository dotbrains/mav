use super::*;

#[gpui::test]
async fn test_archive_mixed_workspace_closes_only_archived_worktree_items(cx: &mut TestAppContext) {
    // When a workspace contains both a worktree being archived and other
    // worktrees that should remain, only the editor items referencing the
    // archived worktree should be closed — the workspace itself must be
    // preserved.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/main-repo",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-b": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-b",
                    },
                },
            },
            "src": {
                "lib.rs": "pub fn hello() {}",
            },
        }),
    )
    .await;

    fs.insert_tree(
        "/worktrees/main-repo/feature-b/main-repo",
        serde_json::json!({
            ".git": "gitdir: /main-repo/.git/worktrees/feature-b",
            "src": {
                "main.rs": "fn main() { hello(); }",
            },
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/main-repo/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/main-repo/feature-b/main-repo"),
            ref_name: Some("refs/heads/feature-b".into()),
            sha: "def".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/main-repo/feature-b/main-repo"),
        None,
        cx,
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    // Create a single project that contains BOTH the main repo and the
    // linked worktree — this makes it a "mixed" workspace.
    let mixed_project = project::Project::test(
        fs.clone(),
        [
            "/main-repo".as_ref(),
            "/worktrees/main-repo/feature-b/main-repo".as_ref(),
        ],
        cx,
    )
    .await;

    mixed_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) = cx
        .add_window_view(|window, cx| MultiWorkspace::test_new(mixed_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Open editor items in both worktrees so we can verify which ones
    // get closed.
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let worktree_ids: Vec<(WorktreeId, Arc<Path>)> = workspace.read_with(cx, |ws, cx| {
        ws.project()
            .read(cx)
            .visible_worktrees(cx)
            .map(|wt| (wt.read(cx).id(), wt.read(cx).abs_path()))
            .collect()
    });

    let main_repo_wt_id = worktree_ids
        .iter()
        .find(|(_, path)| path.as_ref() == Path::new("/main-repo"))
        .map(|(id, _)| *id)
        .expect("should find main-repo worktree");

    let feature_b_wt_id = worktree_ids
        .iter()
        .find(|(_, path)| path.as_ref() == Path::new("/worktrees/main-repo/feature-b/main-repo"))
        .map(|(id, _)| *id)
        .expect("should find feature-b worktree");

    // Open files from both worktrees.
    let main_repo_path = project::ProjectPath {
        worktree_id: main_repo_wt_id,
        path: Arc::from(rel_path("src/lib.rs")),
    };
    let feature_b_path = project::ProjectPath {
        worktree_id: feature_b_wt_id,
        path: Arc::from(rel_path("src/main.rs")),
    };

    workspace
        .update_in(cx, |ws, window, cx| {
            ws.open_path(main_repo_path.clone(), None, true, window, cx)
        })
        .await
        .expect("should open main-repo file");
    workspace
        .update_in(cx, |ws, window, cx| {
            ws.open_path(feature_b_path.clone(), None, true, window, cx)
        })
        .await
        .expect("should open feature-b file");

    cx.run_until_parked();

    // Verify both items are open.
    let open_paths_before: Vec<project::ProjectPath> = workspace.read_with(cx, |ws, cx| {
        ws.panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx)
                    .items()
                    .filter_map(|item| item.project_path(cx))
            })
            .collect()
    });
    assert!(
        open_paths_before
            .iter()
            .any(|pp| pp.worktree_id == main_repo_wt_id),
        "main-repo file should be open"
    );
    assert!(
        open_paths_before
            .iter()
            .any(|pp| pp.worktree_id == feature_b_wt_id),
        "feature-b file should be open"
    );

    // Save thread metadata for the linked worktree with deliberately
    // mismatched folder_paths to trigger the scan-based detection.
    save_thread_metadata_with_main_paths(
        "feature-b-thread",
        "Feature B Thread",
        PathList::new(&[
            PathBuf::from("/worktrees/main-repo/feature-b/main-repo"),
            PathBuf::from("/nonexistent"),
        ]),
        PathList::new(&[PathBuf::from("/main-repo"), PathBuf::from("/nonexistent")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        cx,
    );

    // Save another thread that references only the main repo (not the
    // linked worktree) so archiving the feature-b thread's worktree isn't
    // blocked by another unarchived thread referencing the same path.
    save_thread_metadata_with_main_paths(
        "other-thread",
        "Other Thread",
        PathList::new(&[PathBuf::from("/main-repo")]),
        PathList::new(&[PathBuf::from("/main-repo")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // There should still be exactly 1 workspace.
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "should have 1 workspace (the mixed workspace)"
    );

    // Archive the feature-b thread.
    let fb_session_id = acp::SessionId::new(Arc::from("feature-b-thread"));
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&fb_session_id, window, cx);
    });

    cx.run_until_parked();

    // The workspace should still exist (it's "mixed" — has non-archived worktrees).
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "mixed workspace should be preserved"
    );

    // Only the feature-b editor item should have been closed.
    let open_paths_after: Vec<project::ProjectPath> = workspace.read_with(cx, |ws, cx| {
        ws.panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx)
                    .items()
                    .filter_map(|item| item.project_path(cx))
            })
            .collect()
    });
    assert!(
        open_paths_after
            .iter()
            .any(|pp| pp.worktree_id == main_repo_wt_id),
        "main-repo file should still be open"
    );
    assert!(
        !open_paths_after
            .iter()
            .any(|pp| pp.worktree_id == feature_b_wt_id),
        "feature-b file should have been closed"
    );
}
