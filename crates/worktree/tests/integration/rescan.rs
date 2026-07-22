use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_root_rescan_reconciles_stale_state(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "old.txt": "",
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
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![rel_path(""), rel_path("old.txt")]
        );
    });

    fs.pause_events();
    fs.remove_file(Path::new("/root/old.txt"), RemoveOptions::default())
        .await
        .unwrap();
    fs.insert_file(Path::new("/root/new.txt"), Vec::new()).await;
    assert_eq!(fs.buffered_event_count(), 2);
    fs.clear_buffered_events();

    tree.read_with(cx, |tree, _| {
        assert!(tree.entry_for_path(rel_path("old.txt")).is_some());
        assert!(tree.entry_for_path(rel_path("new.txt")).is_none());
    });

    fs.emit_fs_event("/root", Some(fs::PathEventKind::Rescan));
    fs.unpause_events_and_flush();
    tree.flush_fs_events(cx).await;

    tree.read_with(cx, |tree, _| {
        assert!(tree.entry_for_path(rel_path("old.txt")).is_none());
        assert!(tree.entry_for_path(rel_path("new.txt")).is_some());
        assert_eq!(
            tree.entries(true, 0)
                .map(|entry| entry.path.as_ref())
                .collect::<Vec<_>>(),
            vec![rel_path(""), rel_path("new.txt")]
        );
    });
}

#[gpui::test]
async fn test_root_rescan_does_not_miss_event_before_readding_root_watcher(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree("/root", json!({})).await;

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

    fs.create_file_before_next_watch_add("/root", "/root/created-before-root-readd.txt");
    fs.emit_fs_event("/root", Some(PathEventKind::Rescan));

    wait_for_condition(cx, |cx| {
        tree.read_with(cx, |tree, _| {
            tree.entry_for_path(rel_path("created-before-root-readd.txt"))
                .is_some()
        })
    })
    .await;
}

#[gpui::test]
async fn test_subtree_rescan_reports_unchanged_descendants_as_updated(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "dir": {
                "child.txt": "",
                "nested": {
                    "grandchild.txt": "",
                },
                "remove": {
                    "removed.txt": "",
                }
            },
            "other.txt": "",
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

    let tree_updates = Arc::new(Mutex::new(Vec::new()));
    tree.update(cx, |_, cx| {
        let tree_updates = tree_updates.clone();
        cx.subscribe(&tree, move |_, _, event, _| {
            if let Event::UpdatedEntries(update) = event {
                tree_updates.lock().extend(
                    update
                        .iter()
                        .filter(|(path, _, _)| path.as_ref() != rel_path("fs-event-sentinel"))
                        .map(|(path, _, change)| (path.clone(), *change)),
                );
            }
        })
        .detach();
    });
    fs.pause_events();
    fs.insert_file("/root/dir/new.txt", b"new content".to_vec())
        .await;
    fs.remove_dir(
        "/root/dir/remove".as_ref(),
        RemoveOptions {
            recursive: true,
            ignore_if_not_exists: false,
        },
    )
    .await
    .unwrap();
    fs.clear_buffered_events();
    fs.unpause_events_and_flush();

    fs.emit_fs_event("/root/dir", Some(fs::PathEventKind::Rescan));
    tree.flush_fs_events(cx).await;

    assert_eq!(
        mem::take(&mut *tree_updates.lock()),
        &[
            (rel_path("dir").into(), PathChange::Updated),
            (rel_path("dir/child.txt").into(), PathChange::Updated),
            (rel_path("dir/nested").into(), PathChange::Updated),
            (
                rel_path("dir/nested/grandchild.txt").into(),
                PathChange::Updated
            ),
            (rel_path("dir/new.txt").into(), PathChange::Added),
            (rel_path("dir/remove").into(), PathChange::Removed),
            (
                rel_path("dir/remove/removed.txt").into(),
                PathChange::Removed
            ),
        ]
    );

    tree.read_with(cx, |tree, _| {
        assert!(tree.entry_for_path(rel_path("other.txt")).is_some());
    });
}
