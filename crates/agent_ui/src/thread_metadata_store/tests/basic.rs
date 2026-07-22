use super::*;

#[test]
fn test_thread_metadata_title_prefers_override() {
    let mut metadata = make_metadata(
        "session-1",
        "Agent Generated Title",
        Utc::now(),
        PathList::default(),
    );
    metadata.title_override = Some("User Title".into());

    assert_eq!(metadata.title().as_deref(), Some("User Title"));
    assert_eq!(metadata.display_title().as_ref(), "User Title");

    metadata.title_override = None;
    assert_eq!(metadata.title().as_deref(), Some("Agent Generated Title"));
    assert_eq!(metadata.display_title().as_ref(), "Agent Generated Title");
}

#[gpui::test]
async fn test_database_round_trips_title_override(_cx: &mut TestAppContext) {
    let now = Utc::now();
    let mut metadata = make_metadata(
        "session-1",
        "Agent Generated Title",
        now,
        PathList::new(&[Path::new("/project-a")]),
    );
    metadata.title_override = Some("User Title".into());

    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unknown_test");
    let db_name = format!("THREAD_METADATA_DB_{}", test_name);
    let db = ThreadMetadataDb(gpui::block_on(db::open_test_db::<ThreadMetadataDb>(
        &db_name,
    )));

    db.save(metadata).await.unwrap();

    let rows = db.list().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title.as_deref(), Some("Agent Generated Title"));
    assert_eq!(rows[0].title_override.as_deref(), Some("User Title"));
    assert_eq!(rows[0].title().as_deref(), Some("User Title"));
}

#[gpui::test]
async fn test_store_set_title_override_updates_cached_metadata(cx: &mut TestAppContext) {
    init_test(cx);

    let metadata = make_metadata(
        "session-1",
        "Agent Generated Title",
        Utc::now(),
        PathList::default(),
    );
    let thread_id = metadata.thread_id;

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(metadata, cx);
            store.set_title_override(thread_id, "User Title".into(), cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);
        let metadata = store.entry(thread_id).expect("metadata should be cached");
        assert_eq!(metadata.title.as_deref(), Some("Agent Generated Title"));
        assert_eq!(metadata.title_override.as_deref(), Some("User Title"));
        assert_eq!(metadata.display_title().as_ref(), "User Title");
    });
}

#[gpui::test]
async fn test_store_set_generated_title_clears_title_override(cx: &mut TestAppContext) {
    init_test(cx);

    let mut metadata = make_metadata(
        "session-1",
        "Old Generated Title",
        Utc::now(),
        PathList::default(),
    );
    metadata.title_override = Some("User Title".into());
    let thread_id = metadata.thread_id;

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(metadata, cx);
            store.set_generated_title(thread_id, "New Generated Title".into(), cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);
        let metadata = store.entry(thread_id).expect("metadata should be cached");
        assert_eq!(metadata.title.as_deref(), Some("New Generated Title"));
        assert_eq!(metadata.title_override, None);
        assert_eq!(metadata.display_title().as_ref(), "New Generated Title");
    });
}

#[gpui::test]
async fn test_store_initializes_cache_from_database(cx: &mut TestAppContext) {
    let first_paths = PathList::new(&[Path::new("/project-a")]);
    let second_paths = PathList::new(&[Path::new("/project-b")]);
    let now = Utc::now();
    let older = now - chrono::Duration::seconds(1);

    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unknown_test");
    let db_name = format!("THREAD_METADATA_DB_{}", test_name);
    let db = ThreadMetadataDb(gpui::block_on(db::open_test_db::<ThreadMetadataDb>(
        &db_name,
    )));

    db.save(make_metadata(
        "session-1",
        "First Thread",
        now,
        first_paths.clone(),
    ))
    .await
    .unwrap();
    db.save(make_metadata(
        "session-2",
        "Second Thread",
        older,
        second_paths.clone(),
    ))
    .await
    .unwrap();

    cx.update(|cx| {
        let settings_store = settings::SettingsStore::test(cx);
        cx.set_global(settings_store);
        ThreadMetadataStore::init_global(cx);
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        assert_eq!(store.entry_ids().count(), 2);
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

        let first_path_entries: Vec<_> = store
            .entries_for_path(&first_paths, None)
            .filter_map(|entry| entry.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(first_path_entries, vec!["session-1"]);

        let second_path_entries: Vec<_> = store
            .entries_for_path(&second_paths, None)
            .filter_map(|entry| entry.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(second_path_entries, vec!["session-2"]);
    });
}

#[gpui::test]
async fn test_store_cache_updates_after_save_and_delete(cx: &mut TestAppContext) {
    init_test(cx);

    let first_paths = PathList::new(&[Path::new("/project-a")]);
    let second_paths = PathList::new(&[Path::new("/project-b")]);
    let initial_time = Utc::now();
    let updated_time = initial_time + chrono::Duration::seconds(1);

    let initial_metadata = make_metadata(
        "session-1",
        "First Thread",
        initial_time,
        first_paths.clone(),
    );
    let session1_thread_id = initial_metadata.thread_id;

    let second_metadata = make_metadata(
        "session-2",
        "Second Thread",
        initial_time,
        second_paths.clone(),
    );
    let session2_thread_id = second_metadata.thread_id;

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(initial_metadata, cx);
            store.save(second_metadata, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        let first_path_entries: Vec<_> = store
            .entries_for_path(&first_paths, None)
            .filter_map(|entry| entry.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(first_path_entries, vec!["session-1"]);

        let second_path_entries: Vec<_> = store
            .entries_for_path(&second_paths, None)
            .filter_map(|entry| entry.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(second_path_entries, vec!["session-2"]);
    });

    let moved_metadata = ThreadMetadata {
        thread_id: session1_thread_id,
        session_id: Some(acp::SessionId::new("session-1")),
        agent_id: agent::MAV_AGENT_ID.clone(),
        title: Some("First Thread".into()),
        title_override: None,
        updated_at: updated_time,
        created_at: Some(updated_time),
        interacted_at: None,
        worktree_paths: WorktreePaths::from_folder_paths(&second_paths),
        remote_connection: None,
        archived: false,
    };

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(moved_metadata, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        assert_eq!(store.entry_ids().count(), 2);
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

        let first_path_entries: Vec<_> = store
            .entries_for_path(&first_paths, None)
            .filter_map(|entry| entry.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert!(first_path_entries.is_empty());

        let second_path_entries: Vec<_> = store
            .entries_for_path(&second_paths, None)
            .filter_map(|entry| entry.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(second_path_entries.len(), 2);
        assert!(second_path_entries.contains(&"session-1".to_string()));
        assert!(second_path_entries.contains(&"session-2".to_string()));
    });

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.delete(session2_thread_id, cx);
        });
    });

    cx.run_until_parked();

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        let store = store.read(cx);

        assert_eq!(store.entry_ids().count(), 1);

        let second_path_entries: Vec<_> = store
            .entries_for_path(&second_paths, None)
            .filter_map(|entry| entry.session_id.as_ref().map(|s| s.0.to_string()))
            .collect();
        assert_eq!(second_path_entries, vec!["session-1"]);
    });
}

#[test]
fn test_dedup_db_operations_keeps_latest_operation_for_session() {
    let now = Utc::now();

    let meta = make_metadata("session-1", "First Thread", now, PathList::default());
    let thread_id = meta.thread_id;
    let operations = vec![DbOperation::Upsert(meta), DbOperation::Delete(thread_id)];

    let deduped = ThreadMetadataStore::dedup_db_operations(operations);

    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0], DbOperation::Delete(thread_id));
}

#[test]
fn test_dedup_db_operations_keeps_latest_insert_for_same_session() {
    let now = Utc::now();
    let later = now + chrono::Duration::seconds(1);

    let old_metadata = make_metadata("session-1", "Old Title", now, PathList::default());
    let shared_thread_id = old_metadata.thread_id;
    let new_metadata = ThreadMetadata {
        thread_id: shared_thread_id,
        ..make_metadata("session-1", "New Title", later, PathList::default())
    };

    let deduped = ThreadMetadataStore::dedup_db_operations(vec![
        DbOperation::Upsert(old_metadata),
        DbOperation::Upsert(new_metadata.clone()),
    ]);

    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0], DbOperation::Upsert(new_metadata));
}

#[test]
fn test_dedup_db_operations_preserves_distinct_sessions() {
    let now = Utc::now();

    let metadata1 = make_metadata("session-1", "First Thread", now, PathList::default());
    let metadata2 = make_metadata("session-2", "Second Thread", now, PathList::default());
    let deduped = ThreadMetadataStore::dedup_db_operations(vec![
        DbOperation::Upsert(metadata1.clone()),
        DbOperation::Upsert(metadata2.clone()),
    ]);

    assert_eq!(deduped.len(), 2);
    assert!(deduped.contains(&DbOperation::Upsert(metadata1)));
    assert!(deduped.contains(&DbOperation::Upsert(metadata2)));
}
