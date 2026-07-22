use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_deferred_watch_repository_above_root(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor);
    fs.insert_tree(
        path!("/root"),
        json!({
            ".git": {},
            "subproject": {
                "a.txt": "A"
            }
        }),
    )
    .await;
    let worktree = Worktree::local(
        path!("/root/subproject").as_ref(),
        true,
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

    worktree.update(cx, |worktree, cx| {
        worktree.as_local_mut().unwrap().set_defer_watch(true, cx);
    });
    worktree
        .update(cx, |worktree, _| {
            worktree.as_local().unwrap().scan_complete()
        })
        .await;
    cx.run_until_parked();

    let repos = worktree.update(cx, |worktree, _| {
        worktree.as_local().unwrap().repositories()
    });
    pretty_assertions::assert_eq!(repos, [Path::new(path!("/root")).into()]);
}

#[gpui::test]
async fn test_deferred_watch_symlinks_pointing_outside(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "deps": {},
                "src": {
                    "a.rs": "",
                },
            },
            "dir2": {
                "src": {
                    "c.rs": "",
                }
            },
        }),
    )
    .await;

    fs.create_symlink("/root/dir1/deps/dep-dir2".as_ref(), "../../dir2".into())
        .await
        .unwrap();

    let tree = Worktree::local(
        Path::new("/root/dir1"),
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    cx.read(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;
    cx.run_until_parked();

    tree.update(cx, |tree, cx| {
        tree.as_local_mut().unwrap().set_defer_watch(true, cx);
    });
    cx.read(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;
    cx.run_until_parked();

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_external))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("deps"), false),
                (rel_path("deps/dep-dir2"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
            ]
        );
    });

    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir2").into()])
    })
    .recv()
    .await;

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_external))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("deps"), false),
                (rel_path("deps/dep-dir2"), true),
                (rel_path("deps/dep-dir2/src"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
            ]
        );
    });

    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir2/src").into()])
    })
    .recv()
    .await;

    tree.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("deps/dep-dir2/src/c.rs"))
                .is_some()
        );
    });

    fs.insert_file(Path::new("/root/dir2/src/new.rs"), b"".to_vec())
        .await;

    wait_for_condition(cx, |cx| {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(rel_path("deps/dep-dir2/src/new.rs"))
                .is_some()
        })
    })
    .await;
}
