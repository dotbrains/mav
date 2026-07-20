use super::*;
use pretty_assertions::{assert_eq, assert_matches};

#[gpui::test(retries = 5)]
async fn test_rescan_and_remote_updates(cx: &mut gpui::TestAppContext) {
    use worktree::WorktreeModelHandle as _;

    init_test(cx);
    cx.executor().allow_parking();

    let dir = TempTree::new(json!({
        "a": {
            "file1": "",
            "file2": "",
            "file3": "",
        },
        "b": {
            "c": {
                "file4": "",
                "file5": "",
            }
        }
    }));

    let project = Project::test(Arc::new(RealFs::new(None, cx.executor())), [dir.path()], cx).await;

    let buffer_for_path = |path: &'static str, cx: &mut gpui::TestAppContext| {
        let buffer = project.update(cx, |p, cx| p.open_local_buffer(dir.path().join(path), cx));
        async move { buffer.await.unwrap() }
    };
    let id_for_path = |path: &'static str, cx: &mut gpui::TestAppContext| {
        project.update(cx, |project, cx| {
            let tree = project.worktrees(cx).next().unwrap();
            tree.read(cx)
                .entry_for_path(rel_path(path))
                .unwrap_or_else(|| panic!("no entry for path {}", path))
                .id
        })
    };

    let buffer2 = buffer_for_path("a/file2", cx).await;
    let buffer3 = buffer_for_path("a/file3", cx).await;
    let buffer4 = buffer_for_path("b/c/file4", cx).await;
    let buffer5 = buffer_for_path("b/c/file5", cx).await;

    let file2_id = id_for_path("a/file2", cx);
    let file3_id = id_for_path("a/file3", cx);
    let file4_id = id_for_path("b/c/file4", cx);

    // Create a remote copy of this worktree.
    let tree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let metadata = tree.update(cx, |tree, _| tree.metadata_proto());

    let updates = Arc::new(Mutex::new(Vec::new()));
    tree.update(cx, |tree, cx| {
        let updates = updates.clone();
        tree.observe_updates(0, cx, move |update| {
            updates.lock().push(update);
            async { true }
        });
    });

    let remote = cx.update(|cx| {
        Worktree::remote(
            0,
            ReplicaId::REMOTE_SERVER,
            metadata,
            project.read(cx).client().into(),
            project.read(cx).path_style(cx),
            cx,
        )
    });

    cx.executor().run_until_parked();

    cx.update(|cx| {
        assert!(!buffer2.read(cx).is_dirty());
        assert!(!buffer3.read(cx).is_dirty());
        assert!(!buffer4.read(cx).is_dirty());
        assert!(!buffer5.read(cx).is_dirty());
    });

    // Rename and delete files and directories.
    tree.flush_fs_events(cx).await;
    std::fs::rename(dir.path().join("a/file3"), dir.path().join("b/c/file3")).unwrap();
    std::fs::remove_file(dir.path().join("b/c/file5")).unwrap();
    std::fs::rename(dir.path().join("b/c"), dir.path().join("d")).unwrap();
    std::fs::rename(dir.path().join("a/file2"), dir.path().join("a/file2.new")).unwrap();
    tree.flush_fs_events(cx).await;

    cx.update(|app| {
        assert_eq!(
            tree.read(app).paths().collect::<Vec<_>>(),
            vec![
                rel_path("a"),
                rel_path("a/file1"),
                rel_path("a/file2.new"),
                rel_path("b"),
                rel_path("d"),
                rel_path("d/file3"),
                rel_path("d/file4"),
            ]
        );
    });

    assert_eq!(id_for_path("a/file2.new", cx), file2_id);
    assert_eq!(id_for_path("d/file3", cx), file3_id);
    assert_eq!(id_for_path("d/file4", cx), file4_id);

    cx.update(|cx| {
        assert_eq!(
            buffer2.read(cx).file().unwrap().path().as_ref(),
            rel_path("a/file2.new")
        );
        assert_eq!(
            buffer3.read(cx).file().unwrap().path().as_ref(),
            rel_path("d/file3")
        );
        assert_eq!(
            buffer4.read(cx).file().unwrap().path().as_ref(),
            rel_path("d/file4")
        );
        assert_eq!(
            buffer5.read(cx).file().unwrap().path().as_ref(),
            rel_path("b/c/file5")
        );

        assert_matches!(
            buffer2.read(cx).file().unwrap().disk_state(),
            DiskState::Present { .. }
        );
        assert_matches!(
            buffer3.read(cx).file().unwrap().disk_state(),
            DiskState::Present { .. }
        );
        assert_matches!(
            buffer4.read(cx).file().unwrap().disk_state(),
            DiskState::Present { .. }
        );
        assert_eq!(
            buffer5.read(cx).file().unwrap().disk_state(),
            DiskState::Deleted
        );
    });

    // Update the remote worktree. Check that it becomes consistent with the
    // local worktree.
    cx.executor().run_until_parked();

    remote.update(cx, |remote, _| {
        for update in updates.lock().drain(..) {
            remote.as_remote_mut().unwrap().update_from_remote(update);
        }
    });
    cx.executor().run_until_parked();
    remote.update(cx, |remote, _| {
        assert_eq!(
            remote.paths().collect::<Vec<_>>(),
            vec![
                rel_path("a"),
                rel_path("a/file1"),
                rel_path("a/file2.new"),
                rel_path("b"),
                rel_path("d"),
                rel_path("d/file3"),
                rel_path("d/file4"),
            ]
        );
    });
}

#[cfg(target_os = "linux")]
#[gpui::test(retries = 5)]
async fn test_recreated_directory_receives_child_events(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.executor().allow_parking();

    let dir = TempTree::new(json!({}));
    let project = Project::test(Arc::new(RealFs::new(None, cx.executor())), [dir.path()], cx).await;
    let tree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());

    tree.flush_fs_events(cx).await;

    let repro_dir = dir.path().join("repro");
    std::fs::create_dir(&repro_dir).unwrap();
    tree.flush_fs_events(cx).await;

    cx.update(|cx| {
        assert!(tree.read(cx).entry_for_path(rel_path("repro")).is_some());
    });

    std::fs::remove_dir_all(&repro_dir).unwrap();
    tree.flush_fs_events(cx).await;

    cx.update(|cx| {
        assert!(tree.read(cx).entry_for_path(rel_path("repro")).is_none());
    });

    std::fs::create_dir(&repro_dir).unwrap();
    tree.flush_fs_events(cx).await;

    cx.update(|cx| {
        assert!(tree.read(cx).entry_for_path(rel_path("repro")).is_some());
    });

    std::fs::write(repro_dir.join("repro-marker"), "").unwrap();
    tree.flush_fs_events(cx).await;

    cx.update(|cx| {
        assert!(
            tree.read(cx)
                .entry_for_path(rel_path("repro/repro-marker"))
                .is_some()
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_buffer_identity_across_renames(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a": {
                "file1": "",
            }
        }),
    )
    .await;

    let project = Project::test(fs, [Path::new(path!("/dir"))], cx).await;
    let tree = project.update(cx, |project, cx| project.worktrees(cx).next().unwrap());
    let tree_id = tree.update(cx, |tree, _| tree.id());

    let id_for_path = |path: &'static str, cx: &mut gpui::TestAppContext| {
        project.update(cx, |project, cx| {
            let tree = project.worktrees(cx).next().unwrap();
            tree.read(cx)
                .entry_for_path(rel_path(path))
                .unwrap_or_else(|| panic!("no entry for path {}", path))
                .id
        })
    };

    let dir_id = id_for_path("a", cx);
    let file_id = id_for_path("a/file1", cx);
    let buffer = project
        .update(cx, |p, cx| {
            p.open_buffer((tree_id, rel_path("a/file1")), cx)
        })
        .await
        .unwrap();
    buffer.update(cx, |buffer, _| assert!(!buffer.is_dirty()));

    project
        .update(cx, |project, cx| {
            project.rename_entry(dir_id, (tree_id, rel_path("b")).into(), cx)
        })
        .unwrap()
        .await
        .into_included()
        .unwrap();
    cx.executor().run_until_parked();

    assert_eq!(id_for_path("b", cx), dir_id);
    assert_eq!(id_for_path("b/file1", cx), file_id);
    buffer.update(cx, |buffer, _| assert!(!buffer.is_dirty()));
}
