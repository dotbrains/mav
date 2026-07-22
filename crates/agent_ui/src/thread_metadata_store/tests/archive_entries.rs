use super::*;

#[gpui::test]
async fn test_archive_and_unarchive_thread(cx: &mut TestAppContext) {
    init_test(cx);

    let paths = PathList::new(&[Path::new("/project-a")]);
    let now = Utc::now();
    let metadata = make_metadata("session-1", "Thread 1", now, paths.clone());
    let thread_id = metadata.thread_id;

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let path_entries: Vec<_> = store
            .entries_for_path(&paths, None)
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(path_entries, vec!["session-1"]);

        assert_eq!(store.archived_entries().count(), 0);
    });

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.archive(thread_id, None, cx);
        });
    });

    // Thread 1 should now be archived
    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let path_entries: Vec<_> = store
            .entries_for_path(&paths, None)
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert!(path_entries.is_empty());

        let archived: Vec<_> = store.archived_entries().collect();
        assert_eq!(archived.len(), 1);
        assert_eq!(
            archived[0].session_id.as_ref().unwrap().0.as_ref(),
            "session-1"
        );
        assert!(archived[0].archived);
    });

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.unarchive(thread_id, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let path_entries: Vec<_> = store
            .entries_for_path(&paths, None)
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(path_entries, vec!["session-1"]);

        assert_eq!(store.archived_entries().count(), 0);
    });
}

#[gpui::test]
async fn test_entries_for_path_excludes_archived(cx: &mut TestAppContext) {
    init_test(cx);

    let paths = PathList::new(&[Path::new("/project-a")]);
    let now = Utc::now();

    let metadata1 = make_metadata("session-1", "Active Thread", now, paths.clone());
    let metadata2 = make_metadata(
        "session-2",
        "Archived Thread",
        now - chrono::Duration::seconds(1),
        paths.clone(),
    );
    let session2_thread_id = metadata2.thread_id;

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(metadata1, cx);
            store.save(metadata2, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.archive(session2_thread_id, None, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let path_entries: Vec<_> = store
            .entries_for_path(&paths, None)
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(path_entries, vec!["session-1"]);

        assert_eq!(store.entries().count(), 2);

        let archived: Vec<_> = store
            .archived_entries()
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(archived, vec!["session-2"]);
    });
}

#[gpui::test]
async fn test_entries_filter_by_remote_connection(cx: &mut TestAppContext) {
    init_test(cx);

    let main_paths = PathList::new(&[Path::new("/project-a")]);
    let linked_paths = PathList::new(&[Path::new("/wt-feature")]);
    let now = Utc::now();

    let remote_a = RemoteConnectionOptions::Mock(remote::MockConnectionOptions { id: 1 });
    let remote_b = RemoteConnectionOptions::Mock(remote::MockConnectionOptions { id: 2 });

    // Three threads at the same folder_paths but different hosts.
    let local_thread = make_metadata("local-session", "Local Thread", now, main_paths.clone());

    let mut remote_a_thread = make_metadata(
        "remote-a-session",
        "Remote A Thread",
        now - chrono::Duration::seconds(1),
        main_paths.clone(),
    );
    remote_a_thread.remote_connection = Some(remote_a.clone());

    let mut remote_b_thread = make_metadata(
        "remote-b-session",
        "Remote B Thread",
        now - chrono::Duration::seconds(2),
        main_paths.clone(),
    );
    remote_b_thread.remote_connection = Some(remote_b.clone());

    let linked_worktree_paths =
        WorktreePaths::from_path_lists(main_paths.clone(), linked_paths).unwrap();

    let local_linked_thread = ThreadMetadata {
        thread_id: ThreadId::new(),
        archived: false,
        session_id: Some(acp::SessionId::new("local-linked")),
        agent_id: agent::MAV_AGENT_ID.clone(),
        title: Some("Local Linked".into()),
        title_override: None,
        updated_at: now,
        created_at: Some(now),
        interacted_at: None,
        worktree_paths: linked_worktree_paths.clone(),
        remote_connection: None,
    };

    let remote_linked_thread = ThreadMetadata {
        thread_id: ThreadId::new(),
        archived: false,
        session_id: Some(acp::SessionId::new("remote-linked")),
        agent_id: agent::MAV_AGENT_ID.clone(),
        title: Some("Remote Linked".into()),
        title_override: None,
        updated_at: now - chrono::Duration::seconds(1),
        created_at: Some(now - chrono::Duration::seconds(1)),
        interacted_at: None,
        worktree_paths: linked_worktree_paths,
        remote_connection: Some(remote_a.clone()),
    };

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(local_thread, cx);
            store.save(remote_a_thread, cx);
            store.save(remote_b_thread, cx);
            store.save(local_linked_thread, cx);
            store.save(remote_linked_thread, cx);
        });
    });
    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let local_entries: Vec<_> = store
            .entries_for_path(&main_paths, None)
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(local_entries, vec!["local-session"]);

        let remote_a_entries: Vec<_> = store
            .entries_for_path(&main_paths, Some(&remote_a))
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(remote_a_entries, vec!["remote-a-session"]);

        let remote_b_entries: Vec<_> = store
            .entries_for_path(&main_paths, Some(&remote_b))
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(remote_b_entries, vec!["remote-b-session"]);

        let mut local_main_entries: Vec<_> = store
            .entries_for_main_worktree_path(&main_paths, None)
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        local_main_entries.sort();
        assert_eq!(local_main_entries, vec!["local-linked", "local-session"]);

        let mut remote_main_entries: Vec<_> = store
            .entries_for_main_worktree_path(&main_paths, Some(&remote_a))
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        remote_main_entries.sort();
        assert_eq!(
            remote_main_entries,
            vec!["remote-a-session", "remote-linked"]
        );
    });
}

#[gpui::test]
async fn test_save_all_persists_multiple_threads(cx: &mut TestAppContext) {
    init_test(cx);

    let paths = PathList::new(&[Path::new("/project-a")]);
    let now = Utc::now();

    let m1 = make_metadata("session-1", "Thread One", now, paths.clone());
    let m2 = make_metadata(
        "session-2",
        "Thread Two",
        now - chrono::Duration::seconds(1),
        paths.clone(),
    );
    let m3 = make_metadata(
        "session-3",
        "Thread Three",
        now - chrono::Duration::seconds(2),
        paths,
    );

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save_all(vec![m1, m2, m3], cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        assert_eq!(store.entries().count(), 3);
        assert!(
            store
                .entry_by_session(&acp::SessionId::new("session-1"))
                .is_some()
        );
        assert!(
            store
                .entry_by_session(&acp::SessionId::new("session-2"))
                .is_some()
        );
        assert!(
            store
                .entry_by_session(&acp::SessionId::new("session-3"))
                .is_some()
        );

        assert_eq!(store.entry_ids().count(), 3);
    });
}

#[gpui::test]
async fn test_archived_flag_persists_across_reload(cx: &mut TestAppContext) {
    init_test(cx);

    let paths = PathList::new(&[Path::new("/project-a")]);
    let now = Utc::now();
    let metadata = make_metadata("session-1", "Thread 1", now, paths.clone());
    let thread_id = metadata.thread_id;

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.archive(thread_id, None, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            let _ = store.reload(cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let thread = store
            .entry_by_session(&acp::SessionId::new("session-1"))
            .expect("thread should exist after reload");
        assert!(thread.archived);

        let path_entries: Vec<_> = store
            .entries_for_path(&paths, None)
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert!(path_entries.is_empty());

        let archived: Vec<_> = store
            .archived_entries()
            .filter_map(|e| e.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(archived, vec!["session-1"]);
    });
}

#[gpui::test]
async fn test_archive_nonexistent_thread_is_noop(cx: &mut TestAppContext) {
    init_test(cx);

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.archive(ThreadId::new(), None, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        assert!(store.is_empty());
        assert_eq!(store.entries().count(), 0);
        assert_eq!(store.archived_entries().count(), 0);
    });
}

#[gpui::test]
async fn test_save_followed_by_archiving_without_parking(cx: &mut TestAppContext) {
    init_test(cx);

    let paths = PathList::new(&[Path::new("/project-a")]);
    let now = Utc::now();
    let metadata = make_metadata("session-1", "Thread 1", now, paths);
    let thread_id = metadata.thread_id;

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(metadata.clone(), cx);
            store.archive(thread_id, None, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let entries: Vec<ThreadMetadata> = store.entries().cloned().collect();
        pretty_assertions::assert_eq!(
            entries,
            vec![ThreadMetadata {
                archived: true,
                ..metadata
            }]
        );
    });
}
