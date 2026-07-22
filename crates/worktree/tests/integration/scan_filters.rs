use super::*;

#[gpui::test]
async fn test_file_scan_inclusions(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let dir = TempTree::new(json!({
        ".gitignore": "**/target\n/node_modules\ntop_level.txt\n",
        "target": {
            "index": "blah2"
        },
        "node_modules": {
            ".DS_Store": "",
            "prettier": {
                "package.json": "{}",
            },
            "package.json": "//package.json"
        },
        "src": {
            ".DS_Store": "",
            "foo": {
                "foo.rs": "mod another;\n",
                "another.rs": "// another",
            },
            "bar": {
                "bar.rs": "// bar",
            },
            "lib.rs": "mod foo;\nmod bar;\n",
        },
        "top_level.txt": "top level file",
        ".DS_Store": "",
    }));
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(vec![]);
                settings.project.worktree.file_scan_inclusions = Some(vec![
                    "node_modules/**/package.json".to_string(),
                    "**/.DS_Store".to_string(),
                ]);
            });
        });
    });

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
        // Assert that file_scan_inclusions overrides  file_scan_exclusions.
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                excluded_paths: &[],
                ignored_paths: &["target", "node_modules"],
                tracked_paths: &["src/lib.rs", "src/bar/bar.rs", ".gitignore"],
                included_paths: &[
                    "node_modules/prettier/package.json",
                    ".DS_Store",
                    "node_modules/.DS_Store",
                    "src/.DS_Store",
                ],
            },
        )
    });
}

#[gpui::test]
async fn test_file_scan_exclusions_overrules_inclusions(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let dir = TempTree::new(json!({
        ".gitignore": "**/target\n/node_modules\n",
        "target": {
            "index": "blah2"
        },
        "node_modules": {
            ".DS_Store": "",
            "prettier": {
                "package.json": "{}",
            },
        },
        "src": {
            ".DS_Store": "",
            "foo": {
                "foo.rs": "mod another;\n",
                "another.rs": "// another",
            },
        },
        ".DS_Store": "",
    }));

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions =
                    Some(vec!["**/.DS_Store".to_string()]);
                settings.project.worktree.file_scan_inclusions =
                    Some(vec!["**/.DS_Store".to_string()]);
            });
        });
    });

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
        // Assert that file_scan_inclusions overrides  file_scan_exclusions.
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                excluded_paths: &[".DS_Store, src/.DS_Store"],
                ignored_paths: &["target", "node_modules"],
                tracked_paths: &["src/foo/another.rs", "src/foo/foo.rs", ".gitignore"],
                included_paths: &[],
            },
        )
    });
}

#[gpui::test]
async fn test_file_scan_inclusions_reindexes_on_setting_change(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let dir = TempTree::new(json!({
        ".gitignore": "**/target\n/node_modules/\n",
        "target": {
            "index": "blah2"
        },
        "node_modules": {
            ".DS_Store": "",
            "prettier": {
                "package.json": "{}",
            },
        },
        "src": {
            ".DS_Store": "",
            "foo": {
                "foo.rs": "mod another;\n",
                "another.rs": "// another",
            },
        },
        ".DS_Store": "",
    }));

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(vec![]);
                settings.project.worktree.file_scan_inclusions =
                    Some(vec!["node_modules/**".to_string()]);
            });
        });
    });
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
        assert!(
            tree.entry_for_path(rel_path("node_modules"))
                .is_some_and(|f| f.is_always_included)
        );
        assert!(
            tree.entry_for_path(rel_path("node_modules/prettier/package.json"))
                .is_some_and(|f| f.is_always_included)
        );
    });

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(vec![]);
                settings.project.worktree.file_scan_inclusions = Some(vec![]);
            });
        });
    });
    cx.read(|cx| tree.read(cx).as_local().unwrap().scan_complete())
        .await;
    tree.flush_fs_events(cx).await;

    tree.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("node_modules"))
                .is_some_and(|f| !f.is_always_included)
        );
        assert!(
            tree.entry_for_path(rel_path("node_modules/prettier/package.json"))
                .is_some_and(|f| !f.is_always_included)
        );
    });
}

#[gpui::test]
async fn test_file_scan_exclusions(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let dir = TempTree::new(json!({
        ".gitignore": "**/target\n/node_modules\n",
        "target": {
            "index": "blah2"
        },
        "node_modules": {
            ".DS_Store": "",
            "prettier": {
                "package.json": "{}",
            },
        },
        "src": {
            ".DS_Store": "",
            "foo": {
                "foo.rs": "mod another;\n",
                "another.rs": "// another",
            },
            "bar": {
                "bar.rs": "// bar",
            },
            "lib.rs": "mod foo;\nmod bar;\n",
        },
        ".DS_Store": "",
    }));
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions =
                    Some(vec!["**/foo/**".to_string(), "**/.DS_Store".to_string()]);
            });
        });
    });

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
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                excluded_paths: &[
                    "src/foo/foo.rs",
                    "src/foo/another.rs",
                    "node_modules/.DS_Store",
                    "src/.DS_Store",
                    ".DS_Store",
                ],
                ignored_paths: &["target", "node_modules"],
                tracked_paths: &["src/lib.rs", "src/bar/bar.rs", ".gitignore"],
                included_paths: &[],
            },
        )
    });

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions =
                    Some(vec!["**/node_modules/**".to_string()]);
            });
        });
    });
    tree.flush_fs_events(cx).await;
    cx.executor().run_until_parked();
    tree.read_with(cx, |tree, _| {
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                excluded_paths: &[
                    "node_modules/prettier/package.json",
                    "node_modules/.DS_Store",
                    "node_modules",
                ],
                ignored_paths: &["target"],
                tracked_paths: &[
                    ".gitignore",
                    "src/lib.rs",
                    "src/bar/bar.rs",
                    "src/foo/foo.rs",
                    "src/foo/another.rs",
                    "src/.DS_Store",
                    ".DS_Store",
                ],
                included_paths: &[],
            },
        )
    });
}
