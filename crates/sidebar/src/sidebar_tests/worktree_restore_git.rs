use super::*;

#[gpui::test]
async fn test_restore_worktree_when_branch_has_moved(cx: &mut TestAppContext) {
    // restore_worktree_via_git should succeed when the branch has moved
    // to a different SHA since archival. The worktree stays in detached
    // HEAD and the moved branch is left untouched.
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
            sha: "original-sha".into(),
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

    let (multi_workspace, _cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    multi_workspace.update_in(_cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    let wt_repo = worktree_project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });
    let (staged_hash, unstaged_hash) = cx
        .update(|cx| wt_repo.update(cx, |repo, _| repo.create_archive_checkpoint()))
        .await
        .unwrap()
        .unwrap();

    // Move the branch to a different SHA.
    fs.with_git_state(Path::new("/project/.git"), false, |state| {
        state
            .refs
            .insert("refs/heads/feature-a".into(), "moved-sha".into());
    })
    .unwrap();

    let result = cx
        .spawn(|mut cx| async move {
            agent_ui::thread_worktree_archive::restore_worktree_via_git(
                &agent_ui::thread_metadata_store::ArchivedGitWorktree {
                    id: 1,
                    worktree_path: PathBuf::from("/wt-feature-a"),
                    main_repo_path: PathBuf::from("/project"),
                    branch_name: Some("feature-a".to_string()),
                    staged_commit_hash: staged_hash,
                    unstaged_commit_hash: unstaged_hash,
                    original_commit_hash: "original-sha".to_string(),
                },
                None,
                &mut cx,
            )
            .await
        })
        .await;

    assert!(
        result.is_ok(),
        "restore should succeed even when branch has moved: {:?}",
        result.err()
    );

    // The moved branch ref should be completely untouched.
    let branch_sha = fs
        .with_git_state(Path::new("/project/.git"), false, |state| {
            state.refs.get("refs/heads/feature-a").cloned()
        })
        .unwrap();
    assert_eq!(
        branch_sha.as_deref(),
        Some("moved-sha"),
        "the moved branch ref should not be modified by the restore"
    );
}

#[gpui::test]
async fn test_restore_worktree_when_branch_has_not_moved(cx: &mut TestAppContext) {
    // restore_worktree_via_git should succeed when the branch still
    // points at the same SHA as at archive time.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-b": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-b",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/wt-feature-b",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-b",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/wt-feature-b"),
            ref_name: Some("refs/heads/feature-b".into()),
            sha: "original-sha".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-b".as_ref()], cx).await;
    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, _cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    multi_workspace.update_in(_cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    let wt_repo = worktree_project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });
    let (staged_hash, unstaged_hash) = cx
        .update(|cx| wt_repo.update(cx, |repo, _| repo.create_archive_checkpoint()))
        .await
        .unwrap()
        .unwrap();

    // refs/heads/feature-b already points at "original-sha" (set by
    // add_linked_worktree_for_repo), matching original_commit_hash.

    let result = cx
        .spawn(|mut cx| async move {
            agent_ui::thread_worktree_archive::restore_worktree_via_git(
                &agent_ui::thread_metadata_store::ArchivedGitWorktree {
                    id: 1,
                    worktree_path: PathBuf::from("/wt-feature-b"),
                    main_repo_path: PathBuf::from("/project"),
                    branch_name: Some("feature-b".to_string()),
                    staged_commit_hash: staged_hash,
                    unstaged_commit_hash: unstaged_hash,
                    original_commit_hash: "original-sha".to_string(),
                },
                None,
                &mut cx,
            )
            .await
        })
        .await;

    assert!(
        result.is_ok(),
        "restore should succeed when branch has not moved: {:?}",
        result.err()
    );
}

#[gpui::test]
async fn test_restore_worktree_when_branch_does_not_exist(cx: &mut TestAppContext) {
    // restore_worktree_via_git should succeed when the branch no longer
    // exists (e.g. it was deleted while the thread was archived). The
    // code should attempt to recreate the branch.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-d": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-d",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/wt-feature-d",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-d",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/wt-feature-d"),
            ref_name: Some("refs/heads/feature-d".into()),
            sha: "original-sha".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-d".as_ref()], cx).await;
    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, _cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    multi_workspace.update_in(_cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    let wt_repo = worktree_project.read_with(cx, |project, cx| {
        project.repositories(cx).values().next().unwrap().clone()
    });
    let (staged_hash, unstaged_hash) = cx
        .update(|cx| wt_repo.update(cx, |repo, _| repo.create_archive_checkpoint()))
        .await
        .unwrap()
        .unwrap();

    // Remove the branch ref so change_branch will fail.
    fs.with_git_state(Path::new("/project/.git"), false, |state| {
        state.refs.remove("refs/heads/feature-d");
    })
    .unwrap();

    let result = cx
        .spawn(|mut cx| async move {
            agent_ui::thread_worktree_archive::restore_worktree_via_git(
                &agent_ui::thread_metadata_store::ArchivedGitWorktree {
                    id: 1,
                    worktree_path: PathBuf::from("/wt-feature-d"),
                    main_repo_path: PathBuf::from("/project"),
                    branch_name: Some("feature-d".to_string()),
                    staged_commit_hash: staged_hash,
                    unstaged_commit_hash: unstaged_hash,
                    original_commit_hash: "original-sha".to_string(),
                },
                None,
                &mut cx,
            )
            .await
        })
        .await;

    assert!(
        result.is_ok(),
        "restore should succeed when branch does not exist: {:?}",
        result.err()
    );
}
