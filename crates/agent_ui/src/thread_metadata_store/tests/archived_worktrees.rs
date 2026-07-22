use super::*;

#[gpui::test]
async fn test_create_and_retrieve_archived_worktree(cx: &mut TestAppContext) {
    init_test(cx);
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));

    let id = store
        .read_with(cx, |store, cx| {
            store.create_archived_worktree(
                "/tmp/worktree".to_string(),
                "/home/user/repo".to_string(),
                Some("feature-branch".to_string()),
                "staged_aaa".to_string(),
                "unstaged_bbb".to_string(),
                "original_000".to_string(),
                cx,
            )
        })
        .await
        .unwrap();

    let thread_id_1 = ThreadId::new();

    store
        .read_with(cx, |store, cx| {
            store.link_thread_to_archived_worktree(thread_id_1, id, cx)
        })
        .await
        .unwrap();

    let worktrees = store
        .read_with(cx, |store, cx| {
            store.get_archived_worktrees_for_thread(thread_id_1, cx)
        })
        .await
        .unwrap();

    assert_eq!(worktrees.len(), 1);
    let wt = &worktrees[0];
    assert_eq!(wt.id, id);
    assert_eq!(wt.worktree_path, PathBuf::from("/tmp/worktree"));
    assert_eq!(wt.main_repo_path, PathBuf::from("/home/user/repo"));
    assert_eq!(wt.branch_name.as_deref(), Some("feature-branch"));
    assert_eq!(wt.staged_commit_hash, "staged_aaa");
    assert_eq!(wt.unstaged_commit_hash, "unstaged_bbb");
    assert_eq!(wt.original_commit_hash, "original_000");
}

#[gpui::test]
async fn test_delete_archived_worktree(cx: &mut TestAppContext) {
    init_test(cx);
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));

    let id = store
        .read_with(cx, |store, cx| {
            store.create_archived_worktree(
                "/tmp/worktree".to_string(),
                "/home/user/repo".to_string(),
                Some("main".to_string()),
                "deadbeef".to_string(),
                "deadbeef".to_string(),
                "original_000".to_string(),
                cx,
            )
        })
        .await
        .unwrap();

    let thread_id_1 = ThreadId::new();

    store
        .read_with(cx, |store, cx| {
            store.link_thread_to_archived_worktree(thread_id_1, id, cx)
        })
        .await
        .unwrap();

    store
        .read_with(cx, |store, cx| store.delete_archived_worktree(id, cx))
        .await
        .unwrap();

    let worktrees = store
        .read_with(cx, |store, cx| {
            store.get_archived_worktrees_for_thread(thread_id_1, cx)
        })
        .await
        .unwrap();
    assert!(worktrees.is_empty());
}

#[gpui::test]
async fn test_link_multiple_threads_to_archived_worktree(cx: &mut TestAppContext) {
    init_test(cx);
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));

    let id = store
        .read_with(cx, |store, cx| {
            store.create_archived_worktree(
                "/tmp/worktree".to_string(),
                "/home/user/repo".to_string(),
                None,
                "abc123".to_string(),
                "abc123".to_string(),
                "original_000".to_string(),
                cx,
            )
        })
        .await
        .unwrap();

    let thread_id_1 = ThreadId::new();
    let thread_id_2 = ThreadId::new();

    store
        .read_with(cx, |store, cx| {
            store.link_thread_to_archived_worktree(thread_id_1, id, cx)
        })
        .await
        .unwrap();

    store
        .read_with(cx, |store, cx| {
            store.link_thread_to_archived_worktree(thread_id_2, id, cx)
        })
        .await
        .unwrap();

    let wt1 = store
        .read_with(cx, |store, cx| {
            store.get_archived_worktrees_for_thread(thread_id_1, cx)
        })
        .await
        .unwrap();

    let wt2 = store
        .read_with(cx, |store, cx| {
            store.get_archived_worktrees_for_thread(thread_id_2, cx)
        })
        .await
        .unwrap();

    assert_eq!(wt1.len(), 1);
    assert_eq!(wt2.len(), 1);
    assert_eq!(wt1[0].id, wt2[0].id);
}

#[gpui::test]
async fn test_complete_worktree_restore_multiple_paths(cx: &mut TestAppContext) {
    init_test(cx);
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));

    let original_paths = PathList::new(&[
        Path::new("/projects/worktree-a"),
        Path::new("/projects/worktree-b"),
        Path::new("/other/unrelated"),
    ]);
    let meta = make_metadata("session-multi", "Multi Thread", Utc::now(), original_paths);
    let thread_id = meta.thread_id;

    store.update(cx, |store, cx| {
        store.save(meta, cx);
    });

    let replacements = vec![
        (
            PathBuf::from("/projects/worktree-a"),
            PathBuf::from("/restored/worktree-a"),
        ),
        (
            PathBuf::from("/projects/worktree-b"),
            PathBuf::from("/restored/worktree-b"),
        ),
    ];

    store.update(cx, |store, cx| {
        store.complete_worktree_restore(thread_id, &replacements, cx);
    });

    let entry = store.read_with(cx, |store, _cx| store.entry(thread_id).cloned());
    let entry = entry.unwrap();
    let paths = entry.folder_paths().paths();
    assert_eq!(paths.len(), 3);
    assert!(paths.contains(&PathBuf::from("/restored/worktree-a")));
    assert!(paths.contains(&PathBuf::from("/restored/worktree-b")));
    assert!(paths.contains(&PathBuf::from("/other/unrelated")));
}

#[gpui::test]
async fn test_complete_worktree_restore_preserves_unmatched_paths(cx: &mut TestAppContext) {
    init_test(cx);
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));

    let original_paths =
        PathList::new(&[Path::new("/projects/worktree-a"), Path::new("/other/path")]);
    let meta = make_metadata("session-partial", "Partial", Utc::now(), original_paths);
    let thread_id = meta.thread_id;

    store.update(cx, |store, cx| {
        store.save(meta, cx);
    });

    let replacements = vec![
        (
            PathBuf::from("/projects/worktree-a"),
            PathBuf::from("/new/worktree-a"),
        ),
        (
            PathBuf::from("/nonexistent/path"),
            PathBuf::from("/should/not/appear"),
        ),
    ];

    store.update(cx, |store, cx| {
        store.complete_worktree_restore(thread_id, &replacements, cx);
    });

    let entry = store.read_with(cx, |store, _cx| store.entry(thread_id).cloned());
    let entry = entry.unwrap();
    let paths = entry.folder_paths().paths();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&PathBuf::from("/new/worktree-a")));
    assert!(paths.contains(&PathBuf::from("/other/path")));
    assert!(!paths.contains(&PathBuf::from("/should/not/appear")));
}

#[gpui::test]
async fn test_update_restored_worktree_paths_multiple(cx: &mut TestAppContext) {
    init_test(cx);
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));

    let original_paths = PathList::new(&[
        Path::new("/projects/worktree-a"),
        Path::new("/projects/worktree-b"),
        Path::new("/other/unrelated"),
    ]);
    let meta = make_metadata("session-multi", "Multi Thread", Utc::now(), original_paths);
    let thread_id = meta.thread_id;

    store.update(cx, |store, cx| {
        store.save(meta, cx);
    });

    let replacements = vec![
        (
            PathBuf::from("/projects/worktree-a"),
            PathBuf::from("/restored/worktree-a"),
        ),
        (
            PathBuf::from("/projects/worktree-b"),
            PathBuf::from("/restored/worktree-b"),
        ),
    ];

    store.update(cx, |store, cx| {
        store.update_restored_worktree_paths(thread_id, &replacements, cx);
    });

    let entry = store.read_with(cx, |store, _cx| store.entry(thread_id).cloned());
    let entry = entry.unwrap();
    let paths = entry.folder_paths().paths();
    assert_eq!(paths.len(), 3);
    assert!(paths.contains(&PathBuf::from("/restored/worktree-a")));
    assert!(paths.contains(&PathBuf::from("/restored/worktree-b")));
    assert!(paths.contains(&PathBuf::from("/other/unrelated")));
}

#[gpui::test]
async fn test_update_restored_worktree_paths_preserves_unmatched(cx: &mut TestAppContext) {
    init_test(cx);
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));

    let original_paths =
        PathList::new(&[Path::new("/projects/worktree-a"), Path::new("/other/path")]);
    let meta = make_metadata("session-partial", "Partial", Utc::now(), original_paths);
    let thread_id = meta.thread_id;

    store.update(cx, |store, cx| {
        store.save(meta, cx);
    });

    let replacements = vec![
        (
            PathBuf::from("/projects/worktree-a"),
            PathBuf::from("/new/worktree-a"),
        ),
        (
            PathBuf::from("/nonexistent/path"),
            PathBuf::from("/should/not/appear"),
        ),
    ];

    store.update(cx, |store, cx| {
        store.update_restored_worktree_paths(thread_id, &replacements, cx);
    });

    let entry = store.read_with(cx, |store, _cx| store.entry(thread_id).cloned());
    let entry = entry.unwrap();
    let paths = entry.folder_paths().paths();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&PathBuf::from("/new/worktree-a")));
    assert!(paths.contains(&PathBuf::from("/other/path")));
    assert!(!paths.contains(&PathBuf::from("/should/not/appear")));
}

#[gpui::test]
async fn test_multiple_archived_worktrees_per_thread(cx: &mut TestAppContext) {
    init_test(cx);
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));

    let id1 = store
        .read_with(cx, |store, cx| {
            store.create_archived_worktree(
                "/projects/worktree-a".to_string(),
                "/home/user/repo".to_string(),
                Some("branch-a".to_string()),
                "staged_a".to_string(),
                "unstaged_a".to_string(),
                "original_000".to_string(),
                cx,
            )
        })
        .await
        .unwrap();

    let id2 = store
        .read_with(cx, |store, cx| {
            store.create_archived_worktree(
                "/projects/worktree-b".to_string(),
                "/home/user/repo".to_string(),
                Some("branch-b".to_string()),
                "staged_b".to_string(),
                "unstaged_b".to_string(),
                "original_000".to_string(),
                cx,
            )
        })
        .await
        .unwrap();

    let thread_id_1 = ThreadId::new();

    store
        .read_with(cx, |store, cx| {
            store.link_thread_to_archived_worktree(thread_id_1, id1, cx)
        })
        .await
        .unwrap();

    store
        .read_with(cx, |store, cx| {
            store.link_thread_to_archived_worktree(thread_id_1, id2, cx)
        })
        .await
        .unwrap();

    let worktrees = store
        .read_with(cx, |store, cx| {
            store.get_archived_worktrees_for_thread(thread_id_1, cx)
        })
        .await
        .unwrap();

    assert_eq!(worktrees.len(), 2);

    let paths: Vec<&Path> = worktrees
        .iter()
        .map(|w| w.worktree_path.as_path())
        .collect();
    assert!(paths.contains(&Path::new("/projects/worktree-a")));
    assert!(paths.contains(&Path::new("/projects/worktree-b")));
}
