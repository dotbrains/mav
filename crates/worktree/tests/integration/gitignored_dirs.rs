use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_create_file_in_expanded_gitignored_dir(cx: &mut TestAppContext) {
    // Tests the behavior of our worktree refresh when a file in a gitignored directory
    // is created.
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            ".gitignore": "ignored_dir\n",
            "ignored_dir": {
                "existing_file.txt": "existing content",
                "another_file.txt": "another content",
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
        let ignored_dir = tree.entry_for_path(rel_path("ignored_dir")).unwrap();
        assert!(ignored_dir.is_ignored);
        assert_eq!(ignored_dir.kind, EntryKind::UnloadedDir);
    });

    tree.update(cx, |tree, cx| {
        tree.load_file(rel_path("ignored_dir/existing_file.txt"), cx)
    })
    .await
    .unwrap();

    tree.read_with(cx, |tree, _| {
        let ignored_dir = tree.entry_for_path(rel_path("ignored_dir")).unwrap();
        assert!(ignored_dir.is_ignored);
        assert_eq!(ignored_dir.kind, EntryKind::Dir);

        assert!(
            tree.entry_for_path(rel_path("ignored_dir/existing_file.txt"))
                .is_some()
        );
        assert!(
            tree.entry_for_path(rel_path("ignored_dir/another_file.txt"))
                .is_some()
        );
    });

    let entry = tree
        .update(cx, |tree, cx| {
            tree.create_entry(rel_path("ignored_dir/new_file.txt").into(), false, None, cx)
        })
        .await
        .unwrap();
    assert!(entry.into_included().is_some());

    cx.executor().run_until_parked();

    tree.read_with(cx, |tree, _| {
        let ignored_dir = tree.entry_for_path(rel_path("ignored_dir")).unwrap();
        assert!(ignored_dir.is_ignored);
        assert_eq!(
            ignored_dir.kind,
            EntryKind::Dir,
            "ignored_dir should still be loaded, not UnloadedDir"
        );

        assert!(
            tree.entry_for_path(rel_path("ignored_dir/existing_file.txt"))
                .is_some(),
            "existing_file.txt should still be visible"
        );
        assert!(
            tree.entry_for_path(rel_path("ignored_dir/another_file.txt"))
                .is_some(),
            "another_file.txt should still be visible"
        );
        assert!(
            tree.entry_for_path(rel_path("ignored_dir/new_file.txt"))
                .is_some(),
            "new_file.txt should be visible"
        );
    });
}

#[gpui::test]
async fn test_fs_event_for_gitignored_dir_does_not_lose_contents(cx: &mut TestAppContext) {
    // Tests the behavior of our worktree refresh when a directory modification for a gitignored directory
    // is triggered.
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            ".gitignore": "ignored_dir\n",
            "ignored_dir": {
                "file1.txt": "content1",
                "file2.txt": "content2",
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

    // Load a file to expand the ignored directory
    tree.update(cx, |tree, cx| {
        tree.load_file(rel_path("ignored_dir/file1.txt"), cx)
    })
    .await
    .unwrap();

    tree.read_with(cx, |tree, _| {
        let ignored_dir = tree.entry_for_path(rel_path("ignored_dir")).unwrap();
        assert_eq!(ignored_dir.kind, EntryKind::Dir);
        assert!(
            tree.entry_for_path(rel_path("ignored_dir/file1.txt"))
                .is_some()
        );
        assert!(
            tree.entry_for_path(rel_path("ignored_dir/file2.txt"))
                .is_some()
        );
    });

    fs.emit_fs_event("/root/ignored_dir", Some(fs::PathEventKind::Changed));
    tree.flush_fs_events(cx).await;

    tree.read_with(cx, |tree, _| {
        let ignored_dir = tree.entry_for_path(rel_path("ignored_dir")).unwrap();
        assert_eq!(
            ignored_dir.kind,
            EntryKind::Dir,
            "ignored_dir should still be loaded (Dir), not UnloadedDir"
        );
        assert!(
            tree.entry_for_path(rel_path("ignored_dir/file1.txt"))
                .is_some(),
            "file1.txt should still be visible after directory fs event"
        );
        assert!(
            tree.entry_for_path(rel_path("ignored_dir/file2.txt"))
                .is_some(),
            "file2.txt should still be visible after directory fs event"
        );
    });
}
