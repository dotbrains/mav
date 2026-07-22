use super::*;

#[gpui::test]
async fn test_open_gitignored_files(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            ".gitignore": "node_modules\n",
            "one": {
                "node_modules": {
                    "a": {
                        "a1.js": "a1",
                        "a2.js": "a2",
                    },
                    "b": {
                        "b1.js": "b1",
                        "b2.js": "b2",
                    },
                    "c": {
                        "c1.js": "c1",
                        "c2.js": "c2",
                    }
                },
            },
            "two": {
                "x.js": "",
                "y.js": "",
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

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path(".gitignore"), false),
                (rel_path("one"), false),
                (rel_path("one/node_modules"), true),
                (rel_path("two"), false),
                (rel_path("two/x.js"), false),
                (rel_path("two/y.js"), false),
            ]
        );
    });

    // Open a file that is nested inside of a gitignored directory that
    // has not yet been expanded.
    let prev_read_dir_count = fs.read_dir_call_count();
    let loaded = tree
        .update(cx, |tree, cx| {
            tree.load_file(rel_path("one/node_modules/b/b1.js"), cx)
        })
        .await
        .unwrap();

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path(".gitignore"), false),
                (rel_path("one"), false),
                (rel_path("one/node_modules"), true),
                (rel_path("one/node_modules/a"), true),
                (rel_path("one/node_modules/b"), true),
                (rel_path("one/node_modules/b/b1.js"), true),
                (rel_path("one/node_modules/b/b2.js"), true),
                (rel_path("one/node_modules/c"), true),
                (rel_path("two"), false),
                (rel_path("two/x.js"), false),
                (rel_path("two/y.js"), false),
            ]
        );

        assert_eq!(
            loaded.file.path.as_ref(),
            rel_path("one/node_modules/b/b1.js")
        );

        // Only the newly-expanded directories are scanned.
        assert_eq!(fs.read_dir_call_count() - prev_read_dir_count, 2);
    });

    // Open another file in a different subdirectory of the same
    // gitignored directory.
    let prev_read_dir_count = fs.read_dir_call_count();
    let loaded = tree
        .update(cx, |tree, cx| {
            tree.load_file(rel_path("one/node_modules/a/a2.js"), cx)
        })
        .await
        .unwrap();

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_ignored))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path(".gitignore"), false),
                (rel_path("one"), false),
                (rel_path("one/node_modules"), true),
                (rel_path("one/node_modules/a"), true),
                (rel_path("one/node_modules/a/a1.js"), true),
                (rel_path("one/node_modules/a/a2.js"), true),
                (rel_path("one/node_modules/b"), true),
                (rel_path("one/node_modules/b/b1.js"), true),
                (rel_path("one/node_modules/b/b2.js"), true),
                (rel_path("one/node_modules/c"), true),
                (rel_path("two"), false),
                (rel_path("two/x.js"), false),
                (rel_path("two/y.js"), false),
            ]
        );

        assert_eq!(
            loaded.file.path.as_ref(),
            rel_path("one/node_modules/a/a2.js")
        );

        // Only the newly-expanded directory is scanned.
        assert_eq!(fs.read_dir_call_count() - prev_read_dir_count, 1);
    });

    let path = PathBuf::from("/root/one/node_modules/c/lib");

    // No work happens when files and directories change within an unloaded directory.
    let prev_fs_call_count = fs.read_dir_call_count() + fs.metadata_call_count();
    // When we open a directory, we check each ancestor whether it's a git
    // repository. That means we have an fs.metadata call per ancestor that we
    // need to subtract here.
    let ancestors = path.ancestors().count();

    fs.create_dir(path.as_ref()).await.unwrap();
    cx.executor().run_until_parked();

    assert_eq!(
        fs.read_dir_call_count() + fs.metadata_call_count() - prev_fs_call_count - ancestors,
        0
    );
}
