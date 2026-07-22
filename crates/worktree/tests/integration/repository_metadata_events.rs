use super::*;

#[gpui::test]
async fn test_linked_worktree_event_in_unregistered_common_git_dir_does_not_panic(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    // Regression test: a rescan event on a linked worktree's commondir
    // must not panic when the worktree's repository has already been
    // unregistered from `git_repositories`.
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

    // Unregister the linked worktree's repository by removing its gitfile.
    fs.remove_file(
        Path::new(path!("/linked_worktree/.git")),
        Default::default(),
    )
    .await
    .unwrap();
    tree.flush_fs_events(cx).await;

    // Deliver the kind of Rescan event `FsWatcher` emits when the kernel
    // signals `need_rescan` for the commondir.
    fs.emit_fs_event(path!("/main_repo/.git"), Some(fs::PathEventKind::Rescan));
    cx.run_until_parked();
    tree.flush_fs_events(cx).await;
}

#[gpui::test]
async fn test_dot_git_dir_event_does_not_suppress_children(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    // On Windows, modifying a file inside .git causes ReadDirectoryChangesW to also emit
    // a Modify event for the .git directory itself (because its last-write timestamp changes).
    // When these events arrive in the same batch, a naive ancestor-based dedup would collapse
    // all child events into the .git directory event, losing the information about which
    // specific files changed. This test verifies that the git-related event processing happens
    // before the dedup, so that meaningful .git child events still trigger UpdatedGitRepositories.
    init_test(cx);

    let fs = FakeFs::new(executor.clone());
    let project_dir = Path::new(path!("/project"));
    fs.insert_tree(
        project_dir,
        json!({
            ".git": {},
            "src": {
                "main.rs": "fn main() {}",
            },
        }),
    )
    .await;

    let worktree = Worktree::local(
        project_dir,
        true,
        fs.clone(),
        Default::default(),
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

    let dot_git = project_dir.join(DOT_GIT);

    // Case 1: Events for .git AND .git/index.lock should NOT emit UpdatedGitRepositories
    // (index.lock is in the skipped files list)
    {
        let mut events = cx.events(&worktree);
        fs.pause_events();
        fs.emit_fs_event(dot_git.clone(), Some(PathEventKind::Changed));
        fs.emit_fs_event(dot_git.join("index.lock"), Some(PathEventKind::Created));
        fs.unpause_events_and_flush();
        executor.run_until_parked();

        let got_git_update = drain_git_repo_updates(&mut events);
        assert!(
            !got_git_update,
            "should NOT emit UpdatedGitRepositories when .git batch only contains index.lock"
        );
    }

    // Case 2: Event for just .git (bare directory event) should NOT emit UpdatedGitRepositories
    {
        let mut events = cx.events(&worktree);
        fs.pause_events();
        fs.emit_fs_event(dot_git.clone(), Some(PathEventKind::Changed));
        fs.unpause_events_and_flush();
        executor.run_until_parked();

        let got_git_update = drain_git_repo_updates(&mut events);
        assert!(
            !got_git_update,
            "should NOT emit UpdatedGitRepositories for a bare .git directory event"
        );
    }

    // Case 3: Events for .git AND .git/index should emit UpdatedGitRepositories
    {
        let mut events = cx.events(&worktree);
        fs.pause_events();
        fs.emit_fs_event(dot_git.clone(), Some(PathEventKind::Changed));
        fs.emit_fs_event(dot_git.join("index"), Some(PathEventKind::Changed));
        fs.unpause_events_and_flush();
        executor.run_until_parked();

        let got_git_update = drain_git_repo_updates(&mut events);
        assert!(
            got_git_update,
            "should emit UpdatedGitRepositories when .git batch contains index"
        );
    }

    // Case 4: Event for .git/index only should emit UpdatedGitRepositories
    {
        let mut events = cx.events(&worktree);
        fs.pause_events();
        fs.emit_fs_event(dot_git.join("index"), Some(PathEventKind::Changed));
        fs.unpause_events_and_flush();
        executor.run_until_parked();

        let got_git_update = drain_git_repo_updates(&mut events);
        assert!(
            got_git_update,
            "should emit UpdatedGitRepositories for a .git/index event"
        );
    }

    {
        let mut events = cx.events(&worktree);
        fs.pause_events();
        fs.emit_fs_event(dot_git, Some(PathEventKind::Rescan));
        fs.unpause_events_and_flush();
        executor.run_until_parked();

        let got_git_update = drain_git_repo_updates(&mut events);
        assert!(
            got_git_update,
            "should emit UpdatedGitRepositories for a .git rescan event"
        );
    }

    {
        let mut events = cx.events(&worktree);
        fs.pause_events();
        fs.emit_fs_event(project_dir, Some(PathEventKind::Rescan));
        fs.unpause_events_and_flush();
        executor.run_until_parked();

        let got_git_update = drain_git_repo_updates(&mut events);
        assert!(
            got_git_update,
            "should emit UpdatedGitRepositories for a .git rescan event"
        );
    }
}
