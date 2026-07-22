use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_renaming_case_only(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    const OLD_NAME: &str = "aaa.rs";
    const NEW_NAME: &str = "AAA.rs";

    let fs = Arc::new(RealFs::new(None, cx.executor()));
    let temp_root = TempTree::new(json!({
        OLD_NAME: "",
    }));

    let tree = Worktree::local(
        temp_root.path(),
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
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![rel_path(""), rel_path(OLD_NAME)]
        );
    });

    fs.rename(
        &temp_root.path().join(OLD_NAME),
        &temp_root.path().join(NEW_NAME),
        fs::RenameOptions {
            overwrite: true,
            ignore_if_exists: true,
            create_parents: false,
        },
    )
    .await
    .unwrap();

    tree.flush_fs_events(cx).await;

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![rel_path(""), rel_path(NEW_NAME)]
        );
    });
}

#[gpui::test]
async fn test_dirs_no_longer_ignored(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            ".gitignore": "node_modules\n",
            "a": {
                "a.js": "",
            },
            "b": {
                "b.js": "",
            },
            "node_modules": {
                "c": {
                    "c.js": "",
                },
                "d": {
                    "d.js": "",
                    "e": {
                        "e1.js": "",
                        "e2.js": "",
                    },
                    "f": {
                        "f1.js": "",
                        "f2.js": "",
                    }
                },
            },
        }),
    )
    .await;

    let tree = Worktree::local(
        Path::new("/root"),
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

    // Open a file within the gitignored directory, forcing some of its
    // subdirectories to be read, but not all.
    let read_dir_count_1 = fs.read_dir_call_count();
    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("node_modules/d/d.js").into()])
    })
    .recv()
    .await;

    // Those subdirectories are now loaded.
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|e| (e.path.as_ref(), e.is_ignored))
                .collect::<Vec<_>>(),
            &[
                (rel_path(""), false),
                (rel_path(".gitignore"), false),
                (rel_path("a"), false),
                (rel_path("a/a.js"), false),
                (rel_path("b"), false),
                (rel_path("b/b.js"), false),
                (rel_path("node_modules"), true),
                (rel_path("node_modules/c"), true),
                (rel_path("node_modules/d"), true),
                (rel_path("node_modules/d/d.js"), true),
                (rel_path("node_modules/d/e"), true),
                (rel_path("node_modules/d/f"), true),
            ]
        );
    });
    let read_dir_count_2 = fs.read_dir_call_count();
    assert_eq!(read_dir_count_2 - read_dir_count_1, 2);

    // Update the gitignore so that node_modules is no longer ignored,
    // but a subdirectory is ignored
    fs.save("/root/.gitignore".as_ref(), &"e".into(), Default::default())
        .await
        .unwrap();
    cx.executor().run_until_parked();

    // All of the directories that are no longer ignored are now loaded.
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|e| (e.path.as_ref(), e.is_ignored))
                .collect::<Vec<_>>(),
            &[
                (rel_path(""), false),
                (rel_path(".gitignore"), false),
                (rel_path("a"), false),
                (rel_path("a/a.js"), false),
                (rel_path("b"), false),
                (rel_path("b/b.js"), false),
                // This directory is no longer ignored
                (rel_path("node_modules"), false),
                (rel_path("node_modules/c"), false),
                (rel_path("node_modules/c/c.js"), false),
                (rel_path("node_modules/d"), false),
                (rel_path("node_modules/d/d.js"), false),
                // This subdirectory is now ignored
                (rel_path("node_modules/d/e"), true),
                (rel_path("node_modules/d/f"), false),
                (rel_path("node_modules/d/f/f1.js"), false),
                (rel_path("node_modules/d/f/f2.js"), false),
            ]
        );
    });

    // Each of the newly-loaded directories is scanned only once.
    let read_dir_count_3 = fs.read_dir_call_count();
    assert_eq!(read_dir_count_3 - read_dir_count_2, 2);
}

#[gpui::test]
async fn test_write_file(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let dir = TempTree::new(json!({
        ".git": {},
        ".gitignore": "ignored-dir\n",
        "tracked-dir": {},
        "ignored-dir": {}
    }));

    let worktree = Worktree::local(
        dir.path(),
        true,
        Arc::new(RealFs::new(None, cx.executor())),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    #[cfg(not(target_os = "macos"))]
    fs::fs_watcher::global(|_| {}).unwrap();

    cx.read(|cx| worktree.read(cx).as_local().unwrap().scan_complete())
        .await;
    worktree.flush_fs_events(cx).await;

    worktree
        .update(cx, |tree, cx| {
            tree.write_file(
                rel_path("tracked-dir/file.txt").into(),
                "hello".into(),
                Default::default(),
                encoding_rs::UTF_8,
                false,
                cx,
            )
        })
        .await
        .unwrap();
    worktree
        .update(cx, |tree, cx| {
            tree.write_file(
                rel_path("ignored-dir/file.txt").into(),
                "world".into(),
                Default::default(),
                encoding_rs::UTF_8,
                false,
                cx,
            )
        })
        .await
        .unwrap();
    worktree.read_with(cx, |tree, _| {
        let tracked = tree
            .entry_for_path(rel_path("tracked-dir/file.txt"))
            .unwrap();
        let ignored = tree
            .entry_for_path(rel_path("ignored-dir/file.txt"))
            .unwrap();
        assert!(!tracked.is_ignored);
        assert!(ignored.is_ignored);
    });
}

#[gpui::test]
async fn test_hidden_files(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let dir = TempTree::new(json!({
        ".gitignore": "**/target\n",
        ".hidden_file": "content",
        ".hidden_dir": {
            "nested.rs": "code",
        },
        "src": {
            "visible.rs": "code",
        },
        "logs": {
            "app.log": "logs",
            "debug.log": "logs",
        },
        "visible.txt": "content",
    }));

    let tree = Worktree::local(
        dir.path(),
        true,
        Arc::new(RealFs::new(None, cx.executor())),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();
    cx.read(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;
    tree.flush_fs_events(cx).await;

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_hidden))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path(".gitignore"), true),
                (rel_path(".hidden_dir"), true),
                (rel_path(".hidden_dir/nested.rs"), true),
                (rel_path(".hidden_file"), true),
                (rel_path("logs"), false),
                (rel_path("logs/app.log"), false),
                (rel_path("logs/debug.log"), false),
                (rel_path("src"), false),
                (rel_path("src/visible.rs"), false),
                (rel_path("visible.txt"), false),
            ]
        );
    });

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.hidden_files = Some(vec!["**/*.log".to_string()]);
            });
        });
    });
    tree.flush_fs_events(cx).await;
    cx.executor().run_until_parked();

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_hidden))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path(".gitignore"), false),
                (rel_path(".hidden_dir"), false),
                (rel_path(".hidden_dir/nested.rs"), false),
                (rel_path(".hidden_file"), false),
                (rel_path("logs"), false),
                (rel_path("logs/app.log"), true),
                (rel_path("logs/debug.log"), true),
                (rel_path("src"), false),
                (rel_path("src/visible.rs"), false),
                (rel_path("visible.txt"), false),
            ]
        );
    });
}
