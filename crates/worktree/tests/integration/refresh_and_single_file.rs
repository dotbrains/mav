use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_refresh_entries_for_paths_creates_ancestors(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root",
        json!({
            "a": {
                "b": {
                    "c": {
                        "deep_file.txt": "content",
                        "sibling.txt": "content"
                    },
                    "d": {
                        "under_sibling_dir.txt": "content"
                    }
                }
            }
        }),
    )
    .await;

    let tree = Worktree::local(
        Path::new("/root"),
        true,
        fs.clone(),
        Default::default(),
        false, // Disable scanning so the initial scan doesn't discover any entries
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
                .map(|e| e.path.as_ref())
                .collect::<Vec<_>>(),
            &[rel_path("")],
            "Only root entry should exist when scanning is disabled"
        );

        assert!(tree.entry_for_path(rel_path("a")).is_none());
        assert!(tree.entry_for_path(rel_path("a/b")).is_none());
        assert!(tree.entry_for_path(rel_path("a/b/c")).is_none());
        assert!(
            tree.entry_for_path(rel_path("a/b/c/deep_file.txt"))
                .is_none()
        );
    });

    tree.read_with(cx, |tree, _| {
        tree.as_local()
            .unwrap()
            .refresh_entries_for_paths(vec![rel_path("a/b/c/deep_file.txt").into()])
    })
    .recv()
    .await;

    tree.read_with(cx, |tree, _| {
        assert_eq!(
            tree.entries(true, 0)
                .map(|e| e.path.as_ref())
                .collect::<Vec<_>>(),
            &[
                rel_path(""),
                rel_path("a"),
                rel_path("a/b"),
                rel_path("a/b/c"),
                rel_path("a/b/c/deep_file.txt"),
                rel_path("a/b/c/sibling.txt"),
                rel_path("a/b/d"),
            ],
            "All ancestors should be created when refreshing a deeply nested path"
        );
    });
}

#[gpui::test]
async fn test_single_file_worktree_deleted(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());

    fs.insert_tree(
        "/root",
        json!({
            "test.txt": "content",
        }),
    )
    .await;

    let tree = Worktree::local(
        Path::new("/root/test.txt"),
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
        assert!(tree.is_single_file(), "Should be a single-file worktree");
        assert_eq!(tree.abs_path().as_ref(), Path::new("/root/test.txt"));
    });

    // Delete the file
    fs.remove_file(Path::new("/root/test.txt"), Default::default())
        .await
        .unwrap();

    // Subscribe to worktree events
    let deleted_event_received = Rc::new(Cell::new(false));
    let _subscription = cx.update({
        let deleted_event_received = deleted_event_received.clone();
        |cx| {
            cx.subscribe(&tree, move |_, event, _| {
                if matches!(event, Event::Deleted) {
                    deleted_event_received.set(true);
                }
            })
        }
    });

    // Trigger filesystem events - the scanner should detect the file is gone immediately
    // and emit a Deleted event
    cx.background_executor.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(1));
    cx.background_executor.run_until_parked();

    assert!(
        deleted_event_received.get(),
        "Should receive Deleted event when single-file worktree root is deleted"
    );
}
