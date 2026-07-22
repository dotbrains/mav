use super::*;

#[gpui::test]
async fn test_fs_events_in_exclusions(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let dir = TempTree::new(json!({
        ".git": {
            "HEAD": "ref: refs/heads/main\n",
            "foo": "bar",
        },
        ".gitignore": "**/target\n/node_modules\ntest_output\n",
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
                settings.project.worktree.file_scan_exclusions = Some(vec![
                    "**/.git".to_string(),
                    "node_modules/".to_string(),
                    "build_output".to_string(),
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
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                excluded_paths: &[
                    ".git/HEAD",
                    ".git/foo",
                    "node_modules",
                    "node_modules/.DS_Store",
                    "node_modules/prettier",
                    "node_modules/prettier/package.json",
                ],
                ignored_paths: &["target"],
                tracked_paths: &[
                    ".DS_Store",
                    "src/.DS_Store",
                    "src/lib.rs",
                    "src/foo/foo.rs",
                    "src/foo/another.rs",
                    "src/bar/bar.rs",
                    ".gitignore",
                ],
                included_paths: &[],
            },
        )
    });

    let new_excluded_dir = dir.path().join("build_output");
    let new_ignored_dir = dir.path().join("test_output");
    std::fs::create_dir_all(&new_excluded_dir)
        .unwrap_or_else(|e| panic!("Failed to create a {new_excluded_dir:?} directory: {e}"));
    std::fs::create_dir_all(&new_ignored_dir)
        .unwrap_or_else(|e| panic!("Failed to create a {new_ignored_dir:?} directory: {e}"));
    let node_modules_dir = dir.path().join("node_modules");
    let dot_git_dir = dir.path().join(".git");
    let src_dir = dir.path().join("src");
    for existing_dir in [&node_modules_dir, &dot_git_dir, &src_dir] {
        assert!(
            existing_dir.is_dir(),
            "Expect {existing_dir:?} to be present in the FS already"
        );
    }

    for directory_for_new_file in [
        new_excluded_dir,
        new_ignored_dir,
        node_modules_dir,
        dot_git_dir,
        src_dir,
    ] {
        std::fs::write(directory_for_new_file.join("new_file"), "new file contents")
            .unwrap_or_else(|e| {
                panic!("Failed to create in {directory_for_new_file:?} a new file: {e}")
            });
    }
    tree.flush_fs_events(cx).await;

    tree.read_with(cx, |tree, _| {
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                excluded_paths: &[
                    ".git/HEAD",
                    ".git/foo",
                    ".git/new_file",
                    "node_modules",
                    "node_modules/.DS_Store",
                    "node_modules/prettier",
                    "node_modules/prettier/package.json",
                    "node_modules/new_file",
                    "build_output",
                    "build_output/new_file",
                    "test_output/new_file",
                ],
                ignored_paths: &["target", "test_output"],
                tracked_paths: &[
                    ".DS_Store",
                    "src/.DS_Store",
                    "src/lib.rs",
                    "src/foo/foo.rs",
                    "src/foo/another.rs",
                    "src/bar/bar.rs",
                    "src/new_file",
                    ".gitignore",
                ],
                included_paths: &[],
            },
        )
    });
}

#[gpui::test]
async fn test_fs_events_in_dot_git_worktree(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();
    let dir = TempTree::new(json!({
        ".git": {
            "HEAD": "ref: refs/heads/main\n",
            "foo": "foo contents",
        },
    }));
    let dot_git_worktree_dir = dir.path().join(".git");

    let tree = Worktree::local(
        dot_git_worktree_dir.clone(),
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
                ignored_paths: &["HEAD", "foo"],
                ..Default::default()
            },
        )
    });

    std::fs::write(dot_git_worktree_dir.join("new_file"), "new file contents")
        .unwrap_or_else(|e| panic!("Failed to create in {dot_git_worktree_dir:?} a new file: {e}"));
    tree.flush_fs_events(cx).await;
    tree.read_with(cx, |tree, _| {
        check_worktree_entries(
            tree,
            WorktreeExpectations {
                ignored_paths: &["HEAD", "foo", "new_file"],
                ..Default::default()
            },
        )
    });
}
