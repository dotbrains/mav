use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_root_repo_common_dir_for_relative_gitdir(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/repo"),
        json!({
            ".git": {
                "HEAD": "ref: refs/heads/main",
                "config": "[core]\n\tbare = false\n",
                "info": {
                    "exclude": "ignored.txt\n",
                },
                "worktrees": {
                    "feature-a": {
                        "HEAD": "ref: refs/heads/feature-a",
                        "commondir": "../..",
                        "gitdir": "/repo/feature-a/.git",
                    },
                },
            },
            "feature-a": {
                ".git": "gitdir: ../.git/worktrees/feature-a",
                "file.txt": "content",
                "ignored.txt": "ignored",
                "subdir": {
                    "file.txt": "content",
                    "ignored.txt": "ignored",
                },
            },
        }),
    )
    .await;

    let feature_tree = Worktree::local(
        path!("/repo/feature-a").as_ref(),
        true,
        fs.clone(),
        Arc::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    feature_tree
        .update(cx, |tree, _| tree.as_local().unwrap().scan_complete())
        .await;
    cx.run_until_parked();

    feature_tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.snapshot()
                .root_repo_common_dir()
                .map(|path| path.as_ref()),
            Some(Path::new(path!("/repo/.git"))),
        );
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                ignored_paths: &["ignored.txt"],
                tracked_paths: &["file.txt"],
                ..Default::default()
            },
        );
    });

    let nested_tree = Worktree::local(
        path!("/repo/feature-a/subdir").as_ref(),
        true,
        fs.clone(),
        Arc::default(),
        true,
        WorktreeId::from_proto(1),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    nested_tree
        .update(cx, |tree, _| tree.as_local().unwrap().scan_complete())
        .await;
    cx.run_until_parked();

    nested_tree.read_with(cx, |tree, _| {
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                ignored_paths: &["ignored.txt"],
                tracked_paths: &["file.txt"],
                ..Default::default()
            },
        );
    });

    fs.write(
        Path::new(path!("/repo/.git")).join(REPO_EXCLUDE).as_ref(),
        "file.txt\n".as_bytes(),
    )
    .await
    .unwrap();
    cx.run_until_parked();

    feature_tree.read_with(cx, |tree, _| {
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                ignored_paths: &["file.txt", "subdir/file.txt"],
                tracked_paths: &["ignored.txt", "subdir/ignored.txt"],
                ..Default::default()
            },
        );
    });
    nested_tree.read_with(cx, |tree, _| {
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                ignored_paths: &["file.txt"],
                tracked_paths: &["ignored.txt"],
                ..Default::default()
            },
        );
    });
}

#[gpui::test]
async fn test_root_repo_common_dir(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    use git::repository::Worktree as GitWorktree;

    let fs = FakeFs::new(executor);

    // Set up a main repo and a linked worktree pointing back to it.
    fs.insert_tree(
        path!("/main_repo"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new(path!("/main_repo/.git")),
        false,
        GitWorktree {
            path: PathBuf::from(path!("/linked_worktree")),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    fs.write(
        path!("/linked_worktree/file.txt").as_ref(),
        "content".as_bytes(),
    )
    .await
    .unwrap();

    let tree = Worktree::local(
        path!("/linked_worktree").as_ref(),
        true,
        fs.clone(),
        Arc::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    tree.update(cx, |tree, _| tree.as_local().unwrap().scan_complete())
        .await;
    cx.run_until_parked();

    // For a linked worktree, root_repo_common_dir should point to the
    // main repo's .git, not the worktree-specific git directory.
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.snapshot().root_repo_common_dir().map(|p| p.as_ref()),
            Some(Path::new(path!("/main_repo/.git"))),
        );
    });

    let event_count: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    tree.update(cx, {
        let event_count = event_count.clone();
        |_, cx| {
            cx.subscribe(&cx.entity(), move |_, _, event, _| {
                if matches!(event, Event::UpdatedRootRepoCommonDir { .. }) {
                    event_count.set(event_count.get() + 1);
                }
            })
            .detach();
        }
    });

    // Remove .git — root_repo_common_dir should become None.
    fs.remove_file(
        &PathBuf::from(path!("/linked_worktree/.git")),
        Default::default(),
    )
    .await
    .unwrap();
    tree.flush_fs_events(cx).await;

    tree.read_with(cx, |tree, _| {
        assert_eq!(tree.snapshot().root_repo_common_dir(), None);
    });
    assert_eq!(
        event_count.get(),
        1,
        "should have emitted UpdatedRootRepoCommonDir on removal"
    );
}

#[gpui::test]
async fn test_invisible_worktree_does_not_track_ancestor_git_repository(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/repo"),
        json!({
            ".git": {},
            "project": {
                "file.txt": "content",
            },
        }),
    )
    .await;

    let worktree = Worktree::local(
        path!("/repo/project").as_ref(),
        false,
        fs.clone(),
        Arc::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    worktree.read_with(cx, |worktree, _| {
        let local_worktree = worktree.as_local().unwrap();
        assert!(local_worktree.repositories().is_empty());
        assert_eq!(local_worktree.root_repo_common_dir(), None);
    });
}

#[gpui::test]
async fn test_linked_worktree_gitfile_event_preserves_repo(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    // Regression test: in a linked worktree, `.git` is a file (containing
    // "gitdir: ..."), not a directory. When the background scanner receives
    // a filesystem event for a path inside the main repo's `.git` directory
    // (which it watches via the commondir), the ancestor-walking code in
    // `process_events` calls `is_git_dir` on each ancestor. If `is_git_dir`
    // treats `.git` files the same as `.git` directories, it incorrectly
    // identifies the gitfile as a git dir, adds it to `dot_git_abs_paths`,
    // and `update_git_repositories` panics because the path is outside the
    // worktree root.
    init_test(cx);
    use git::repository::Worktree as GitWorktree;

    let fs = FakeFs::new(executor);
    fs.insert_tree(path!("/main_repo"), json!({ ".git": {}, "file.txt": "" }))
        .await;
    fs.add_linked_worktree_for_repo(
        Path::new(path!("/main_repo/.git")),
        false,
        GitWorktree {
            path: PathBuf::from(path!("/linked_worktree")),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    fs.write(path!("/linked_worktree/file.txt").as_ref(), b"content")
        .await
        .unwrap();

    let tree = Worktree::local(
        path!("/linked_worktree").as_ref(),
        true,
        fs.clone(),
        Arc::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    tree.update(cx, |tree, _| tree.as_local().unwrap().scan_complete())
        .await;
    cx.run_until_parked();

    // Overwrite the .git gitfile with garbage to trigger an event for the
    // gitfile path itself, which only matches `dot_git_abs_path`.
    fs.write(path!("/linked_worktree/.git").as_ref(), b"garbage")
        .await
        .unwrap();
    tree.flush_fs_events(cx).await;

    // The worktree should still be intact.
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.snapshot().root_repo_common_dir().map(|p| p.as_ref()),
            Some(Path::new(path!("/main_repo/.git"))),
            "linked worktree repo should survive a gitfile change event"
        );
    });
}

#[gpui::test]
async fn test_noisy_dot_git_events_do_not_emit_git_repo_update(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    // Events for object database writes, hook files, lock files, and the
    // reflogs of HEAD/branches/remote-tracking branches carry no git state
    // changes that Mav cares about beyond what the accompanying ref or index
    // events already convey, so they must not trigger a git metadata rescan.
    // The stash reflog and ref updates themselves must still trigger one.
    //
    init_test(cx);

    use git::repository::Worktree as GitWorktree;

    let fs = FakeFs::new(executor);

    fs.insert_tree(
        path!("/main_repo"),
        json!({
            ".git": {},
            "file.txt": "content",
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new(path!("/main_repo/.git")),
        false,
        GitWorktree {
            path: PathBuf::from(path!("/linked_worktree")),
            ref_name: Some("refs/heads/feature".into()),
            sha: "abc123".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    fs.write(
        path!("/linked_worktree/file.txt").as_ref(),
        "content".as_bytes(),
    )
    .await
    .unwrap();

    let tree = Worktree::local(
        path!("/linked_worktree").as_ref(),
        true,
        fs.clone(),
        Arc::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    tree.update(cx, |tree, _| tree.as_local().unwrap().scan_complete())
        .await;
    cx.run_until_parked();

    let repo_update_count: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    tree.update(cx, {
        let repo_update_count = repo_update_count.clone();
        |_, cx| {
            cx.subscribe(&cx.entity(), move |_, _, event, _| {
                if matches!(event, Event::UpdatedGitRepositories(_)) {
                    repo_update_count.set(repo_update_count.get() + 1);
                }
            })
            .detach();
        }
    });

    let skipped_paths = [
        // Standard common git dir skipped paths
        path!("/main_repo/.git/objects/aa/bbccddee"),
        path!("/main_repo/.git/objects/pack/pack-1234.pack"),
        path!("/main_repo/.git/hooks/pre-commit"),
        path!("/main_repo/.git/logs/HEAD"),
        path!("/main_repo/.git/logs/refs/heads/main"),
        path!("/main_repo/.git/logs/refs/remotes/origin/main"),
        path!("/main_repo/.git/logs/refs/tags/v1.0"),
        path!("/main_repo/.git/rebase-merge/done"),
        path!("/main_repo/.git/rebase-apply/onto"),
        path!("/main_repo/.git/sequencer/todo"),
        path!("/main_repo/.git/index.lock"),
        path!("/main_repo/.git/refs/heads/main.lock"),
        path!("/main_repo/.git/COMMIT_EDITMSG"),
        path!("/main_repo/.git/packed-refs.new"),
        path!("/main_repo/.git/config.new"),
        path!("/main_repo/.git/index.new"),
        path!("/main_repo/.git/index-abc123.tmp"),
        path!("/main_repo/.git/FETCH_HEAD"),
        path!("/main_repo/.git/ORIG_HEAD"),
        path!("/main_repo/.git/BISECT_LOG"),
        path!("/main_repo/.git/info/refs"),
        path!("/main_repo/.git/info/refs_lzOf51"),
        path!("/main_repo/.git/gc.pid"),
        // Linked-worktree specific skipped paths
        path!("/main_repo/.git/worktrees/feature/index.lock"),
    ];
    for path in skipped_paths {
        fs.emit_fs_event(path, Some(PathEventKind::Changed));
        cx.run_until_parked();
        assert_eq!(
            repo_update_count.get(),
            0,
            "event for {path} should not emit UpdatedGitRepositories"
        );
    }

    let rescan_paths = [
        // Standard common git dir rescan paths
        path!("/main_repo/.git/logs/refs/stash"),
        path!("/main_repo/.git/refs/heads/main"),
        path!("/main_repo/.git/info/exclude"),
        path!("/main_repo/.git/refs/heads/branch.new"),
        path!("/main_repo/.git/refs/heads/branch.tmp"),
        // Linked-worktree worktree-specific rescan paths
        path!("/main_repo/.git/worktrees/feature/index"),
        path!("/main_repo/.git/worktrees/feature/HEAD"),
    ];
    for path in rescan_paths {
        let count_before = repo_update_count.get();
        fs.emit_fs_event(path, Some(PathEventKind::Changed));
        cx.run_until_parked();
        assert!(
            repo_update_count.get() > count_before,
            "event for {path} should emit UpdatedGitRepositories"
        );
    }
}
