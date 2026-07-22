use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_traversal(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
           ".gitignore": "a/b\n",
           "a": {
               "b": "",
               "c": "",
           }
        }),
    )
    .await;

    let tree = Worktree::local(
        Path::new("/root"),
        true,
        fs,
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
            tree.entries(false, 0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path(".gitignore"),
                rel_path("a"),
                rel_path("a/c"),
            ]
        );
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path(".gitignore"),
                rel_path("a"),
                rel_path("a/b"),
                rel_path("a/c"),
            ]
        );
    })
}

#[gpui::test(iterations = 10)]
async fn test_circular_symlinks(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "lib": {
                "a": {
                    "a.txt": ""
                },
                "b": {
                    "b.txt": ""
                }
            }
        }),
    )
    .await;
    fs.create_symlink("/root/lib/a/lib".as_ref(), "..".into())
        .await
        .unwrap();
    fs.create_symlink("/root/lib/b/lib".as_ref(), "..".into())
        .await
        .unwrap();

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
            tree.entries(false, 0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("lib"),
                rel_path("lib/a"),
                rel_path("lib/a/a.txt"),
                rel_path("lib/a/lib"),
                rel_path("lib/b"),
                rel_path("lib/b/b.txt"),
                rel_path("lib/b/lib"),
            ]
        );
    });

    fs.rename(
        Path::new("/root/lib/a/lib"),
        Path::new("/root/lib/a/lib-2"),
        Default::default(),
    )
    .await
    .unwrap();
    cx.executor().run_until_parked();
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(false, 0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("lib"),
                rel_path("lib/a"),
                rel_path("lib/a/a.txt"),
                rel_path("lib/a/lib-2"),
                rel_path("lib/b"),
                rel_path("lib/b/b.txt"),
                rel_path("lib/b/lib"),
            ]
        );
    });
}

#[gpui::test]
async fn test_symlinks_pointing_outside(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "deps": {
                    // symlinks here
                },
                "src": {
                    "a.rs": "",
                    "b.rs": "",
                },
            },
            "dir2": {
                "src": {
                    "c.rs": "",
                    "d.rs": "",
                }
            },
            "dir3": {
                "deps": {},
                "src": {
                    "e.rs": "",
                    "f.rs": "",
                    "nested": {
                        "deep.rs": ""
                    }
                },
            }
        }),
    )
    .await;

    // These symlinks point to directories outside of the worktree's root, dir1.
    fs.create_symlink("/root/dir1/deps/dep-dir2".as_ref(), "../../dir2".into())
        .await
        .unwrap();
    fs.create_symlink("/root/dir1/deps/dep-dir3".as_ref(), "../../dir3".into())
        .await
        .unwrap();
    fs.create_symlink(
        "/root/dir1/deps/dep-dir3-alias".as_ref(),
        "../../dir3".into(),
    )
    .await
    .unwrap();
    fs.create_symlink(
        "/root/dir1/deps/dep-dir3-nested".as_ref(),
        "../../dir3/src/nested".into(),
    )
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

    let tree_updates = Arc::new(Mutex::new(Vec::new()));
    tree.update(cx, |_, cx| {
        let tree_updates = tree_updates.clone();
        cx.subscribe(&tree, move |_, _, event, _| {
            if let Event::UpdatedEntries(update) = event {
                tree_updates.lock().extend(
                    update
                        .iter()
                        .map(|(path, _, change)| (path.clone(), *change)),
                );
            }
        })
        .detach();
    });

    // The symlinked directories are not scanned by default.
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_external))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("deps"), false),
                (rel_path("deps/dep-dir2"), true),
                (rel_path("deps/dep-dir3"), true),
                (rel_path("deps/dep-dir3-alias"), true),
                (rel_path("deps/dep-dir3-nested"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
                (rel_path("src/b.rs"), false),
            ]
        );

        assert_eq!(
            tree.entry_for_path(rel_path("deps/dep-dir2")).unwrap().kind,
            EntryKind::UnloadedDir
        );
    });

    // Expand one of the symlinked directories.
    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir3").into()])
    })
    .recv()
    .await;

    // The expanded directory's contents are loaded. Subdirectories are
    // not scanned yet.
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_external))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("deps"), false),
                (rel_path("deps/dep-dir2"), true),
                (rel_path("deps/dep-dir3"), true),
                (rel_path("deps/dep-dir3/deps"), true),
                (rel_path("deps/dep-dir3/src"), true),
                (rel_path("deps/dep-dir3-alias"), true),
                (rel_path("deps/dep-dir3-nested"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
                (rel_path("src/b.rs"), false),
            ]
        );
    });
    assert_eq!(
        mem::take(&mut *tree_updates.lock()),
        &[
            (rel_path("deps/dep-dir3").into(), PathChange::Loaded),
            (rel_path("deps/dep-dir3/deps").into(), PathChange::Loaded),
            (rel_path("deps/dep-dir3/src").into(), PathChange::Loaded)
        ]
    );

    // Expand a subdirectory of one of the symlinked directories.
    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir3/src").into()])
    })
    .recv()
    .await;

    // The expanded subdirectory's contents are loaded.
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| (entry.path.as_ref(), entry.is_external))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("deps"), false),
                (rel_path("deps/dep-dir2"), true),
                (rel_path("deps/dep-dir3"), true),
                (rel_path("deps/dep-dir3/deps"), true),
                (rel_path("deps/dep-dir3/src"), true),
                (rel_path("deps/dep-dir3/src/e.rs"), true),
                (rel_path("deps/dep-dir3/src/f.rs"), true),
                (rel_path("deps/dep-dir3/src/nested"), true),
                (rel_path("deps/dep-dir3-alias"), true),
                (rel_path("deps/dep-dir3-nested"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
                (rel_path("src/b.rs"), false),
            ]
        );
    });

    assert_eq!(
        mem::take(&mut *tree_updates.lock()),
        &[
            (rel_path("deps/dep-dir3/src").into(), PathChange::Loaded),
            (
                rel_path("deps/dep-dir3/src/e.rs").into(),
                PathChange::Loaded
            ),
            (
                rel_path("deps/dep-dir3/src/f.rs").into(),
                PathChange::Loaded
            ),
            (
                rel_path("deps/dep-dir3/src/nested").into(),
                PathChange::Loaded
            )
        ]
    );

    // After an external symlink subtree is loaded, changes in the target should be reflected.
    fs.insert_file(Path::new("/root/dir3/src/new.rs"), b"".to_vec())
        .await;

    wait_for_condition(cx, |cx| {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(rel_path("deps/dep-dir3/src/new.rs"))
                .is_some()
        })
    })
    .await;

    tree.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("deps/dep-dir3/src/new.rs"))
                .is_some()
        );
    });

    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir3-alias").into()])
    })
    .recv()
    .await;

    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir3-alias/src").into()])
    })
    .recv()
    .await;

    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir3-nested").into()])
    })
    .recv()
    .await;
    // Create a file in the shared target subtree. Because dep-dir3 and dep-dir3-alias both
    // point to the same target, both logical paths should observe the new file.
    fs.insert_file(Path::new("/root/dir3/src/shared-new.rs"), b"".to_vec())
        .await;

    wait_for_condition(cx, |cx| {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(rel_path("deps/dep-dir3/src/shared-new.rs"))
                .is_some()
                && tree
                    .entry_for_path(rel_path("deps/dep-dir3-alias/src/shared-new.rs"))
                    .is_some()
        })
    })
    .await;

    tree.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("deps/dep-dir3/src/shared-new.rs"))
                .is_some()
        );
        assert!(
            tree.entry_for_path(rel_path("deps/dep-dir3-alias/src/shared-new.rs"))
                .is_some()
        );
    });

    // Create a file under the more specific nested target. Longest-prefix matching means this should appear under dep-dir3-nested
    fs.insert_file(
        Path::new("/root/dir3/src/nested/longest-prefix.rs"),
        b"".to_vec(),
    )
    .await;

    wait_for_condition(cx, |cx| {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(rel_path("deps/dep-dir3-nested/longest-prefix.rs"))
                .is_some()
        })
    })
    .await;

    tree.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("deps/dep-dir3-nested/longest-prefix.rs"))
                .is_some()
        );
        assert!(
            tree.entry_for_path(rel_path("deps/dep-dir3/src/nested/longest-prefix.rs"))
                .is_none()
        );
        assert!(
            tree.entry_for_path(rel_path("deps/dep-dir3-alias/src/nested/longest-prefix.rs"))
                .is_none()
        );
    });
}
