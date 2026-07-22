use super::*;
use pretty_assertions::assert_eq;

#[gpui::test(iterations = 10)]
async fn test_circular_symlinks_always(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.scan_symlinks =
                    Some(settings::ScanSymlinksSetting::Always);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project": {
                "lib": {
                    "a": {
                        "a.txt": ""
                    }
                },
                "deps": {}
            },
            "outside": {
                "data.txt": ""
            }
        }),
    )
    .await;

    fs.create_symlink("/root/project/deps/ext".as_ref(), "../../outside".into())
        .await
        .unwrap();
    fs.create_symlink("/root/outside/back".as_ref(), "../../project".into())
        .await
        .unwrap();

    let tree = Worktree::local(
        Path::new("/root/project"),
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
        let entries: Vec<_> = tree
            .entries(true, 0)
            .map(|entry| (entry.path.as_ref(), entry.is_external))
            .collect();

        assert_eq!(
            entries,
            vec![
                (rel_path(""), false),
                (rel_path("deps"), false),
                (rel_path("deps/ext"), true),
                (rel_path("deps/ext/data.txt"), true),
                (rel_path("lib"), false),
                (rel_path("lib/a"), false),
                (rel_path("lib/a/a.txt"), false),
            ]
        );
    });
}

#[gpui::test]
async fn test_scan_symlinks_always_respects_gitignore(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.scan_symlinks =
                    Some(settings::ScanSymlinksSetting::Always);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "project": {
                ".gitignore": "ignored-dep\n",
                "deps": {}
            },
            "external-included": {
                "src": {
                    "included.rs": ""
                }
            },
            "external-ignored": {
                "src": {
                    "ignored.rs": ""
                }
            }
        }),
    )
    .await;

    fs.create_symlink(
        "/root/project/deps/included-dep".as_ref(),
        "../../external-included".into(),
    )
    .await
    .unwrap();
    fs.create_symlink(
        "/root/project/deps/ignored-dep".as_ref(),
        "../../external-ignored".into(),
    )
    .await
    .unwrap();

    let tree = Worktree::local(
        Path::new("/root/project"),
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
                .map(|entry| (entry.path.as_ref(), entry.is_external, entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false, false),
                (rel_path(".gitignore"), false, false),
                (rel_path("deps"), false, false),
                (rel_path("deps/ignored-dep"), true, true),
                (rel_path("deps/included-dep"), true, false),
                (rel_path("deps/included-dep/src"), true, false),
                (rel_path("deps/included-dep/src/included.rs"), true, false),
            ]
        );

        assert_eq!(
            tree.entry_for_path(rel_path("deps/ignored-dep"))
                .unwrap()
                .kind,
            EntryKind::UnloadedDir
        );
    });
}

// Real-fs counterparts to the FakeFs scan_symlinks tests above. FakeFs does not
// model `fs::canonicalize` against a real filesystem, so platform-specific
// canonicalization or readdir behavior is not covered by the FakeFs tests.
// These tests use a real temp directory and a real symlink to exercise the
// production path on the host platform.
#[cfg(unix)]
#[gpui::test]
async fn test_real_fs_scan_symlinks_always(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.scan_symlinks =
                    Some(settings::ScanSymlinksSetting::Always);
            });
        });
    });

    let temp_root = TempTree::new(json!({
        "project": {
            "deps": {},
            "src": {
                "a.rs": "",
            },
        },
        "external": {
            "src": {
                "b.rs": "",
            },
        },
    }));

    // Relative symlink: from temp_root/project/deps/, `../../external` resolves
    // to temp_root/external — outside the worktree root at temp_root/project.
    std::os::unix::fs::symlink(
        "../../external",
        temp_root.path().join("project/deps/dep-external"),
    )
    .unwrap();

    let project_root = temp_root.path().join("project");
    let tree = Worktree::local(
        project_root.as_path(),
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

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_external))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("deps"), false),
                (rel_path("deps/dep-external"), true),
                (rel_path("deps/dep-external/src"), true),
                (rel_path("deps/dep-external/src/b.rs"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
            ]
        );
    });
}

#[cfg(unix)]
#[gpui::test]
async fn test_real_fs_scan_symlinks_expanded(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    init_test(cx);

    // scan_symlinks defaults to Expanded — no settings change needed.

    let temp_root = TempTree::new(json!({
        "project": {
            "deps": {},
            "src": {
                "a.rs": "",
            },
        },
        "external": {
            "src": {
                "b.rs": "",
            },
        },
    }));

    std::os::unix::fs::symlink(
        "../../external",
        temp_root.path().join("project/deps/dep-external"),
    )
    .unwrap();

    let project_root = temp_root.path().join("project");
    let tree = Worktree::local(
        project_root.as_path(),
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

    // Before expansion, the symlinked directory should appear as an UnloadedDir
    // with no children visible.
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_external))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("deps"), false),
                (rel_path("deps/dep-external"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
            ]
        );

        assert_eq!(
            tree.entry_for_path(rel_path("deps/dep-external"))
                .unwrap()
                .kind,
            EntryKind::UnloadedDir
        );
    });

    // Manually expand the symlinked directory. This is the case #51382 was
    // added to fix; if this assertion fails it's a regression of that fix on
    // real filesystems.
    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-external").into()])
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
                (rel_path("deps/dep-external"), true),
                (rel_path("deps/dep-external/src"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
            ]
        );
    });
}
