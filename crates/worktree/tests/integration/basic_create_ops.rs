use super::*;
use pretty_assertions::assert_eq;

#[gpui::test(iterations = 30)]
async fn test_create_directory_during_initial_scan(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "b": {},
            "c": {},
            "d": {},
        }),
    )
    .await;

    let tree = Worktree::local(
        "/root".as_ref(),
        true,
        fs,
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    let snapshot1 = tree.update(cx, |tree, cx| {
        let tree = tree.as_local_mut().unwrap();
        let snapshot = Arc::new(Mutex::new(tree.snapshot()));
        tree.observe_updates(0, cx, {
            let snapshot = snapshot.clone();
            let settings = tree.settings();
            move |update| {
                snapshot
                    .lock()
                    .apply_remote_update(update, &settings.file_scan_inclusions);
                async { true }
            }
        });
        snapshot
    });

    let entry = tree
        .update(cx, |tree, cx| {
            tree.as_local_mut()
                .unwrap()
                .create_entry(rel_path("a/e").into(), true, None, cx)
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();
    assert!(entry.is_dir());

    cx.executor().run_until_parked();
    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entry_for_path(rel_path("a/e")).unwrap().kind,
            EntryKind::Dir
        );
    });

    let snapshot2 = tree.update(cx, |tree, _| tree.as_local().unwrap().snapshot());
    assert_eq!(
        snapshot1.lock().entries(true, 0).collect::<Vec<_>>(),
        snapshot2.entries(true, 0).collect::<Vec<_>>()
    );
}

#[gpui::test]
async fn test_create_dir_all_on_create_entry(cx: &mut TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let fs_fake = FakeFs::new(cx.background_executor.clone());
    fs_fake
        .insert_tree(
            "/root",
            json!({
                "a": {},
            }),
        )
        .await;

    let tree_fake = Worktree::local(
        "/root".as_ref(),
        true,
        fs_fake,
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    let entry = tree_fake
        .update(cx, |tree, cx| {
            tree.as_local_mut().unwrap().create_entry(
                rel_path("a/b/c/d.txt").into(),
                false,
                None,
                cx,
            )
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();
    assert!(entry.is_file());

    cx.executor().run_until_parked();
    tree_fake.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("a/b/c/d.txt"))
                .unwrap()
                .is_file()
        );
        assert!(tree.entry_for_path(rel_path("a/b/c")).unwrap().is_dir());
        assert!(tree.entry_for_path(rel_path("a/b")).unwrap().is_dir());
    });

    let fs_real = Arc::new(RealFs::new(None, cx.executor()));
    let temp_root = TempTree::new(json!({
        "a": {}
    }));

    let tree_real = Worktree::local(
        temp_root.path(),
        true,
        fs_real,
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    let entry = tree_real
        .update(cx, |tree, cx| {
            tree.as_local_mut().unwrap().create_entry(
                rel_path("a/b/c/d.txt").into(),
                false,
                None,
                cx,
            )
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();
    assert!(entry.is_file());

    cx.executor().run_until_parked();
    tree_real.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("a/b/c/d.txt"))
                .unwrap()
                .is_file()
        );
        assert!(tree.entry_for_path(rel_path("a/b/c")).unwrap().is_dir());
        assert!(tree.entry_for_path(rel_path("a/b")).unwrap().is_dir());
    });

    // Test smallest change
    let entry = tree_real
        .update(cx, |tree, cx| {
            tree.as_local_mut().unwrap().create_entry(
                rel_path("a/b/c/e.txt").into(),
                false,
                None,
                cx,
            )
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();
    assert!(entry.is_file());

    cx.executor().run_until_parked();
    tree_real.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("a/b/c/e.txt"))
                .unwrap()
                .is_file()
        );
    });

    // Test largest change
    let entry = tree_real
        .update(cx, |tree, cx| {
            tree.as_local_mut().unwrap().create_entry(
                rel_path("d/e/f/g.txt").into(),
                false,
                None,
                cx,
            )
        })
        .await
        .unwrap()
        .into_included()
        .unwrap();
    assert!(entry.is_file());

    cx.executor().run_until_parked();
    tree_real.read_with(cx, |tree, _| {
        assert!(
            tree.entry_for_path(rel_path("d/e/f/g.txt"))
                .unwrap()
                .is_file()
        );
        assert!(tree.entry_for_path(rel_path("d/e/f")).unwrap().is_dir());
        assert!(tree.entry_for_path(rel_path("d/e")).unwrap().is_dir());
        assert!(tree.entry_for_path(rel_path("d")).unwrap().is_dir());
    });
}
