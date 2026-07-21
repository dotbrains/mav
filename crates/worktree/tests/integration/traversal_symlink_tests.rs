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
