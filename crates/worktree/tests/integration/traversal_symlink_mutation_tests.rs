use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_renaming_subdir_under_symlinked_root_keeps_children(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/target",
        json!({
            "file1.txt": "",
            "file2.log": "",
            "subdir-a": {
                "config.ini": "",
            },
            "subdir-b": {
                "nested": {
                    "note.md": "",
                },
            },
        }),
    )
    .await;
    fs.create_symlink("/link".as_ref(), "/target".into())
        .await
        .unwrap();

    let tree = Worktree::local(
        Path::new("/link"),
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

    fs.rename(
        Path::new("/link/subdir-a"),
        Path::new("/link/subdir-aa"),
        Default::default(),
    )
    .await
    .unwrap();

    wait_for_condition(cx, |cx| {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(rel_path("subdir-a")).is_none()
                && tree
                    .entry_for_path(rel_path("subdir-aa/config.ini"))
                    .is_some()
        })
    })
    .await;

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![
                rel_path(""),
                rel_path("file1.txt"),
                rel_path("file2.log"),
                rel_path("subdir-aa"),
                rel_path("subdir-aa/config.ini"),
                rel_path("subdir-b"),
                rel_path("subdir-b/nested"),
                rel_path("subdir-b/nested/note.md"),
            ]
        );
    });
}

#[gpui::test]
async fn test_symlinked_dir_inside_project(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());

    fs.insert_tree(
        "/root",
        json!({
            "project": {
                "real-dir": {
                    "existing.rs": "",
                    "nested": {
                        "deep.rs": ""
                    }
                },
                "links": {}
            }
        }),
    )
    .await;

    fs.create_symlink(
        "/root/project/links/internal".as_ref(),
        "../real-dir".into(),
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
                .map(|entry| (entry.path.as_ref(), entry.is_external))
                .collect::<Vec<_>>(),
            vec![
                (rel_path(""), false),
                (rel_path("links"), false),
                (rel_path("links/internal"), false),
                (rel_path("links/internal/existing.rs"), false),
                (rel_path("links/internal/nested"), false),
                (rel_path("links/internal/nested/deep.rs"), false),
                (rel_path("real-dir"), false),
                (rel_path("real-dir/existing.rs"), false),
                (rel_path("real-dir/nested"), false),
                (rel_path("real-dir/nested/deep.rs"), false),
            ]
        );

        assert_eq!(
            tree.entry_for_path(rel_path("links/internal"))
                .unwrap()
                .kind,
            EntryKind::Dir
        );
    });

    fs.insert_file(Path::new("/root/project/real-dir/new.txt"), b"".to_vec())
        .await;
    wait_for_condition(cx, |cx| {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(rel_path("links/internal/new.txt"))
                .is_some()
        })
    })
    .await;

    tree.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("links/internal/new.txt"))
                .is_some()
        );
    });

    fs.insert_file(
        Path::new("/root/project/real-dir/nested/inner.txt"),
        b"".to_vec(),
    )
    .await;
    wait_for_condition(cx, |cx| {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(rel_path("links/internal/nested/inner.txt"))
                .is_some()
        })
    })
    .await;

    tree.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("links/internal/nested/inner.txt"))
                .is_some()
        );
    });
}

#[gpui::test]
async fn test_scan_symlinks_always(cx: &mut TestAppContext) {
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
            "dir1": {
                "deps": {
                    // symlink target placed here by create_symlink below
                },
                "src": {
                    "a.rs": "",
                },
            },
            "dir2": {
                "src": {
                    "b.rs": "",
                }
            }
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

    // With scan_symlinks = Always, the symlinked directory's contents should be
    // fully visible on the first scan without any manual expansion.
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
                (rel_path("deps/dep-dir2/src/b.rs"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
            ]
        );
    });
}

#[gpui::test]
async fn test_scan_symlinks_expanded(cx: &mut TestAppContext) {
    init_test(cx);

    // scan_symlinks defaults to Expanded — no settings change needed.

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "deps": {
                    // symlink target placed here by create_symlink below
                },
                "src": {
                    "a.rs": "",
                },
            },
            "dir2": {
                "src": {
                    "b.rs": "",
                }
            }
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

    // With the default scan_symlinks = Expanded, the symlinked directory
    // should appear as an UnloadedDir entry with no children visible.
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

        assert_eq!(
            tree.entry_for_path(rel_path("deps/dep-dir2")).unwrap().kind,
            EntryKind::UnloadedDir
        );
    });

    // Manually expand the symlinked directory.
    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir2").into()])
    })
    .recv()
    .await;

    // After expansion, dep-dir2's immediate children are visible. Subdirectories
    // within it are present but not yet scanned.
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

        assert_eq!(
            tree.entry_for_path(rel_path("deps/dep-dir2/src"))
                .unwrap()
                .kind,
            EntryKind::UnloadedDir
        );
    });

    // Expand the subdirectory inside the symlinked directory.
    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("deps/dep-dir2/src").into()])
    })
    .recv()
    .await;

    // After expanding the subdirectory, its files are visible.
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
                (rel_path("deps/dep-dir2/src/b.rs"), true),
                (rel_path("src"), false),
                (rel_path("src/a.rs"), false),
            ]
        );
    });
}
