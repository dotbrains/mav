use super::*;

#[gpui::test]
async fn test_recent_workspace_identity_deduplication(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());

    // Main repo with a linked worktree entry
    fs.insert_tree(
        "/repo",
        json!({
            ".git": {
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
        "/worktree",
        json!({
            ".git": "gitdir: /repo/.git/worktrees/feature",
            "src": { "main.rs": "" }
        }),
    )
    .await;

    // A plain non-git project
    fs.insert_tree(
        "/plain-project",
        json!({
            "src": { "main.rs": "" }
        }),
    )
    .await;

    // Another normal git repo (used in mixed-path entry)
    fs.insert_tree(
        "/other-repo",
        json!({
            ".git": {},
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    let t0 = Utc::now() - chrono::Duration::hours(4);
    let t1 = Utc::now() - chrono::Duration::hours(3);
    let t2 = Utc::now() - chrono::Duration::hours(2);
    let t3 = Utc::now() - chrono::Duration::hours(1);

    let workspaces = vec![
        local_recent_workspace(WorkspaceId(1), PathList::new(&["/repo"]), t0, fs.as_ref()).await,
        local_recent_workspace(
            WorkspaceId(2),
            PathList::new(&["/worktree"]),
            t1,
            fs.as_ref(),
        )
        .await,
        local_recent_workspace(
            WorkspaceId(3),
            PathList::new(&["/other-repo", "/worktree"]),
            t2,
            fs.as_ref(),
        )
        .await,
        local_recent_workspace(
            WorkspaceId(4),
            PathList::new(&["/plain-project"]),
            t3,
            fs.as_ref(),
        )
        .await,
    ];

    let result = dedupe_recent_workspaces(workspaces);

    // Should have 3 entries: #1 and #2 deduped into one, plus #3 and #4.
    assert_eq!(result.len(), 3);

    // First entry: /repo — deduplicated from #1 and #2.
    // Keeps the position of #1 (first seen), but with #2's later timestamp.
    assert_eq!(result[0].identity_paths.paths(), &[PathBuf::from("/repo")]);
    assert_eq!(result[0].timestamp, t1);

    // Second entry: mixed-path workspace with worktree resolved.
    // /worktree → /repo, so paths become [/other-repo, /repo] (sorted).
    assert_eq!(
        result[1].identity_paths.paths(),
        &[PathBuf::from("/other-repo"), PathBuf::from("/repo")]
    );
    assert_eq!(result[1].workspace_id, WorkspaceId(3));

    // Third entry: non-git project, unchanged.
    assert_eq!(
        result[2].identity_paths.paths(),
        &[PathBuf::from("/plain-project")]
    );
    assert_eq!(result[2].workspace_id, WorkspaceId(4));
}

#[gpui::test]
async fn test_recent_workspace_identity_for_bare_repo(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());

    // Bare repo at /foo/.bare (commondir doesn't end with .git)
    fs.insert_tree(
        "/foo/.bare",
        json!({
            "worktrees": {
                "my-feature": {
                    "commondir": "../../",
                    "HEAD": "ref: refs/heads/my-feature"
                }
            }
        }),
    )
    .await;

    // Linked worktree whose commondir resolves to a bare repo (/foo/.bare)
    fs.insert_tree(
        "/foo/my-feature",
        json!({
            ".git": "gitdir: /foo/.bare/worktrees/my-feature",
            "src": { "main.rs": "" }
        }),
    )
    .await;

    let t0 = Utc::now();

    let result = local_recent_workspace(
        WorkspaceId(1),
        PathList::new(&["/foo/my-feature"]),
        t0,
        fs.as_ref(),
    )
    .await;

    // Bare-backed worktrees should resolve to the repo identity path, which
    // is the parent directory users think of as the project root.
    assert_eq!(result.identity_paths.paths(), &[PathBuf::from("/foo")]);
}

#[gpui::test]
async fn test_recent_workspace_identity_deduplicates_main_and_linked_worktree(
    cx: &mut gpui::TestAppContext,
) {
    let fs = fs::FakeFs::new(cx.executor());

    fs.insert_tree(
        "/the-project",
        json!({
            ".git": "gitdir: ./.bare\n",
            ".bare": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    fs.insert_tree(
        "/the-project/feature-a",
        json!({
            ".git": "gitdir: ../.bare/worktrees/feature-a\n",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    let t0 = Utc::now() - chrono::Duration::hours(1);
    let t1 = Utc::now();
    let workspaces = vec![
        local_recent_workspace(
            WorkspaceId(1),
            PathList::new(&["/the-project"]),
            t0,
            fs.as_ref(),
        )
        .await,
        local_recent_workspace(
            WorkspaceId(2),
            PathList::new(&["/the-project/feature-a"]),
            t1,
            fs.as_ref(),
        )
        .await,
    ];

    let result = dedupe_recent_workspaces(workspaces);

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].identity_paths.paths(),
        &[PathBuf::from("/the-project")]
    );
    assert_eq!(result[0].workspace_id, WorkspaceId(2));
    assert_eq!(result[0].timestamp, t1);
}

#[gpui::test]
async fn test_recent_project_workspaces_preserve_reopen_paths(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db =
        WorkspaceDb::open_test_db("test_recent_project_workspaces_preserve_reopen_paths").await;

    fs.insert_tree(
        "/the-project",
        json!({
            ".git": "gitdir: ./.bare\n",
            ".bare": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    fs.insert_tree(
        "/the-project/feature-a",
        json!({
            ".git": "gitdir: ../.bare/worktrees/feature-a\n",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    db.save_workspace(workspace_with(
        1,
        &[Path::new("/the-project")],
        empty_pane_group(),
        None,
    ))
    .await;
    db.save_workspace(workspace_with(
        2,
        &[Path::new("/the-project/feature-a")],
        empty_pane_group(),
        None,
    ))
    .await;
    db.set_timestamp_for_tests(WorkspaceId(1), "2024-01-01 00:00:00".to_owned())
        .await
        .unwrap();
    db.set_timestamp_for_tests(WorkspaceId(2), "2024-01-01 00:00:01".to_owned())
        .await
        .unwrap();

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();

    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].workspace_id, WorkspaceId(2));
    assert_eq!(
        recents[0].paths.paths(),
        &[PathBuf::from("/the-project/feature-a")]
    );
    assert_eq!(
        recents[0].identity_paths.paths(),
        &[PathBuf::from("/the-project")]
    );
}

#[gpui::test]
async fn test_recent_project_workspaces_remote_identity_hint(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db("test_recent_project_workspaces_remote_identity_hint").await;

    let workspace = remote_workspace_with(1, "example.com", &[Path::new("/repo/feature-a")]);
    db.save_workspace(SerializedWorkspace {
        identity_paths: Some(PathList::new(&["/repo"])),
        ..workspace
    })
    .await;

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();

    assert_eq!(recents.len(), 1);
    assert_eq!(
        recents[0].paths.paths(),
        &[PathBuf::from("/repo/feature-a")]
    );
    assert_eq!(recents[0].identity_paths.paths(), &[PathBuf::from("/repo")]);
}

#[gpui::test]
async fn test_recent_project_workspaces_remote_paths_do_not_use_local_fs_identity(
    cx: &mut gpui::TestAppContext,
) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db(
        "test_recent_project_workspaces_remote_paths_do_not_use_local_fs_identity",
    )
    .await;

    fs.insert_tree(
        "/repo",
        json!({
            ".git": "gitdir: ./.bare\n",
            ".bare": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;
    fs.insert_tree(
        "/repo/feature-a",
        json!({
            ".git": "gitdir: ../.bare/worktrees/feature-a\n",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    db.save_workspace(remote_workspace_with(
        1,
        "example.com",
        &[Path::new("/repo/feature-a")],
    ))
    .await;

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();

    assert_eq!(recents.len(), 1);
    assert_eq!(
        recents[0].identity_paths.paths(),
        &[PathBuf::from("/repo/feature-a")]
    );
}

#[gpui::test]
async fn test_recent_project_workspaces_do_not_dedupe_remote_hosts(cx: &mut gpui::TestAppContext) {
    let fs = fs::FakeFs::new(cx.executor());
    let db = WorkspaceDb::open_test_db("test_recent_project_workspaces_do_not_dedupe_remote_hosts")
        .await;

    db.save_workspace(remote_workspace_with(1, "host-a", &[Path::new("/repo")]))
        .await;
    db.save_workspace(remote_workspace_with(2, "host-b", &[Path::new("/repo")]))
        .await;
    db.set_timestamp_for_tests(WorkspaceId(1), "2024-01-01 00:00:00".to_owned())
        .await
        .unwrap();
    db.set_timestamp_for_tests(WorkspaceId(2), "2024-01-01 00:00:01".to_owned())
        .await
        .unwrap();

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();

    assert_eq!(recents.len(), 2);
    assert_eq!(recents[0].workspace_id, WorkspaceId(2));
    assert_eq!(recents[1].workspace_id, WorkspaceId(1));
}

#[gpui::test]
async fn test_delete_recent_workspace_group_removes_all_matching_rows(
    cx: &mut gpui::TestAppContext,
) {
    let fs = fs::FakeFs::new(cx.executor());
    let db =
        WorkspaceDb::open_test_db("test_delete_recent_workspace_group_removes_all_matching_rows")
            .await;

    fs.insert_tree(
        "/the-group",
        json!({
            ".git": "gitdir: ./.bare\n",
            ".bare": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a"
                    }
                }
            },
            "src": { "main.rs": "" }
        }),
    )
    .await;

    fs.insert_tree(
        "/the-group/feature-a",
        json!({
            ".git": "gitdir: ../.bare/worktrees/feature-a\n",
            "src": { "lib.rs": "" }
        }),
    )
    .await;

    db.save_workspace(SerializedWorkspace {
        identity_paths: Some(PathList::new(&["/the-group"])),
        ..workspace_with(1, &[Path::new("/the-group")], empty_pane_group(), None)
    })
    .await;
    db.save_workspace(SerializedWorkspace {
        identity_paths: Some(PathList::new(&["/the-group"])),
        ..workspace_with(
            2,
            &[Path::new("/the-group/feature-a")],
            empty_pane_group(),
            None,
        )
    })
    .await;
    db.set_timestamp_for_tests(WorkspaceId(1), "2024-01-01 00:00:00".to_owned())
        .await
        .unwrap();
    db.set_timestamp_for_tests(WorkspaceId(2), "2024-01-01 00:00:01".to_owned())
        .await
        .unwrap();

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();
    assert_eq!(recents.len(), 1);

    let deleted = db.delete_recent_workspace_group(&recents[0]).await.unwrap();
    assert_eq!(deleted, vec![WorkspaceId(2), WorkspaceId(1)]);

    let recents = db.recent_project_workspaces(fs.as_ref()).await.unwrap();
    assert!(recents.is_empty());
}
