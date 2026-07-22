use super::random_ops_helpers::*;
use super::*;
use pretty_assertions::assert_eq;

#[gpui::test(iterations = 100)]
async fn test_random_worktree_operations_during_initial_scan(
    cx: &mut TestAppContext,
    mut rng: StdRng,
) {
    init_test(cx);
    let operations = env::var("OPERATIONS")
        .map(|o| o.parse().unwrap())
        .unwrap_or(5);
    let initial_entries = env::var("INITIAL_ENTRIES")
        .map(|o| o.parse().unwrap())
        .unwrap_or(20);

    let root_dir = Path::new(path!("/test"));
    let fs = FakeFs::new(cx.background_executor.clone()) as Arc<dyn Fs>;
    fs.as_fake().insert_tree(root_dir, json!({})).await;
    for _ in 0..initial_entries {
        randomly_mutate_fs(&fs, root_dir, 1.0, &mut rng).await;
    }
    log::info!("generated initial tree");

    let worktree = Worktree::local(
        root_dir,
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    let mut snapshots = vec![worktree.read_with(cx, |tree, _| tree.as_local().unwrap().snapshot())];
    let updates = Arc::new(Mutex::new(Vec::new()));
    worktree.update(cx, |tree, cx| {
        check_worktree_change_events(tree, cx);

        tree.as_local_mut().unwrap().observe_updates(0, cx, {
            let updates = updates.clone();
            move |update| {
                updates.lock().push(update);
                async { true }
            }
        });
    });

    for _ in 0..operations {
        worktree
            .update(cx, |worktree, cx| {
                randomly_mutate_worktree(worktree, &mut rng, cx)
            })
            .await
            .log_err();
        worktree.read_with(cx, |tree, _| {
            tree.as_local().unwrap().snapshot().check_invariants(true)
        });

        if rng.random_bool(0.6) {
            snapshots.push(worktree.read_with(cx, |tree, _| tree.as_local().unwrap().snapshot()));
        }
    }

    worktree
        .update(cx, |tree, _| tree.as_local_mut().unwrap().scan_complete())
        .await;

    cx.executor().run_until_parked();

    let final_snapshot = worktree.read_with(cx, |tree, _| {
        let tree = tree.as_local().unwrap();
        let snapshot = tree.snapshot();
        snapshot.check_invariants(true);
        snapshot
    });

    let settings = worktree.read_with(cx, |tree, _| tree.as_local().unwrap().settings());

    for (i, snapshot) in snapshots.into_iter().enumerate().rev() {
        let mut updated_snapshot = snapshot.clone();
        for update in updates.lock().iter() {
            if update.scan_id >= updated_snapshot.scan_id() as u64 {
                updated_snapshot
                    .apply_remote_update(update.clone(), &settings.file_scan_inclusions);
            }
        }

        assert_eq!(
            updated_snapshot.entries(true, 0).collect::<Vec<_>>(),
            final_snapshot.entries(true, 0).collect::<Vec<_>>(),
            "wrong updates after snapshot {i}: {updates:#?}",
        );
    }
}

#[gpui::test(iterations = 100)]
async fn test_random_worktree_changes(cx: &mut TestAppContext, mut rng: StdRng) {
    init_test(cx);
    let operations = env::var("OPERATIONS")
        .map(|o| o.parse().unwrap())
        .unwrap_or(40);
    let initial_entries = env::var("INITIAL_ENTRIES")
        .map(|o| o.parse().unwrap())
        .unwrap_or(20);

    let root_dir = Path::new(path!("/test"));
    let fs = FakeFs::new(cx.background_executor.clone()) as Arc<dyn Fs>;
    fs.as_fake().insert_tree(root_dir, json!({})).await;
    for _ in 0..initial_entries {
        randomly_mutate_fs(&fs, root_dir, 1.0, &mut rng).await;
    }
    log::info!("generated initial tree");

    let worktree = Worktree::local(
        root_dir,
        true,
        fs.clone(),
        Default::default(),
        true,
        WorktreeId::from_proto(0),
        &mut cx.to_async(),
    )
    .await
    .unwrap();

    let updates = Arc::new(Mutex::new(Vec::new()));
    worktree.update(cx, |tree, cx| {
        check_worktree_change_events(tree, cx);

        tree.as_local_mut().unwrap().observe_updates(0, cx, {
            let updates = updates.clone();
            move |update| {
                updates.lock().push(update);
                async { true }
            }
        });
    });

    worktree
        .update(cx, |tree, _| tree.as_local_mut().unwrap().scan_complete())
        .await;

    fs.as_fake().pause_events();
    let mut snapshots = Vec::new();
    let mut mutations_len = operations;
    while mutations_len > 1 {
        if rng.random_bool(0.2) {
            worktree
                .update(cx, |worktree, cx| {
                    randomly_mutate_worktree(worktree, &mut rng, cx)
                })
                .await
                .log_err();
        } else {
            randomly_mutate_fs(&fs, root_dir, 1.0, &mut rng).await;
        }

        let buffered_event_count = fs.as_fake().buffered_event_count();
        if buffered_event_count > 0 && rng.random_bool(0.3) {
            let len = rng.random_range(0..=buffered_event_count);
            log::info!("flushing {} events", len);
            fs.as_fake().flush_events(len);
        } else {
            randomly_mutate_fs(&fs, root_dir, 0.6, &mut rng).await;
            mutations_len -= 1;
        }

        cx.executor().run_until_parked();
        if rng.random_bool(0.2) {
            log::info!("storing snapshot {}", snapshots.len());
            let snapshot = worktree.read_with(cx, |tree, _| tree.as_local().unwrap().snapshot());
            snapshots.push(snapshot);
        }
    }

    log::info!("quiescing");
    fs.as_fake().flush_events(usize::MAX);
    cx.executor().run_until_parked();

    let snapshot = worktree.read_with(cx, |tree, _| tree.as_local().unwrap().snapshot());
    snapshot.check_invariants(true);
    let expanded_paths = snapshot
        .expanded_entries()
        .map(|e| e.path.clone())
        .collect::<Vec<_>>();

    {
        let new_worktree = Worktree::local(
            root_dir,
            true,
            fs.clone(),
            Default::default(),
            true,
            WorktreeId::from_proto(0),
            &mut cx.to_async(),
        )
        .await
        .unwrap();
        new_worktree
            .update(cx, |tree, _| tree.as_local_mut().unwrap().scan_complete())
            .await;
        new_worktree
            .update(cx, |tree, _| {
                tree.as_local_mut()
                    .unwrap()
                    .refresh_entries_for_paths(expanded_paths)
            })
            .recv()
            .await;
        let new_snapshot =
            new_worktree.read_with(cx, |tree, _| tree.as_local().unwrap().snapshot());
        assert_eq!(
            snapshot.entries_without_ids(true),
            new_snapshot.entries_without_ids(true)
        );
    }

    let settings = worktree.read_with(cx, |tree, _| tree.as_local().unwrap().settings());

    for (i, mut prev_snapshot) in snapshots.into_iter().enumerate().rev() {
        for update in updates.lock().iter() {
            if update.scan_id >= prev_snapshot.scan_id() as u64 {
                prev_snapshot.apply_remote_update(update.clone(), &settings.file_scan_inclusions);
            }
        }

        assert_eq!(
            prev_snapshot
                .entries(true, 0)
                .map(ignore_pending_dir)
                .collect::<Vec<_>>(),
            snapshot
                .entries(true, 0)
                .map(ignore_pending_dir)
                .collect::<Vec<_>>(),
            "wrong updates after snapshot {i}: {updates:#?}",
        );
    }

    fn ignore_pending_dir(entry: &Entry) -> Entry {
        let mut entry = entry.clone();
        if entry.kind.is_dir() {
            entry.kind = EntryKind::Dir
        }
        entry
    }
}

// The worktree's `UpdatedEntries` event can be used to follow along with
// all changes to the worktree's snapshot.
