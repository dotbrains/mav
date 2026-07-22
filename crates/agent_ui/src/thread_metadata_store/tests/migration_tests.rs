use super::*;

#[gpui::test]
async fn test_migrate_thread_metadata_migrates_only_missing_threads(cx: &mut TestAppContext) {
    init_test(cx);

    let project_a_paths = PathList::new(&[Path::new("/project-a")]);
    let project_b_paths = PathList::new(&[Path::new("/project-b")]);
    let now = Utc::now();

    let existing_metadata = ThreadMetadata {
        thread_id: ThreadId::new(),
        session_id: Some(acp::SessionId::new("a-session-0")),
        agent_id: agent::MAV_AGENT_ID.clone(),
        title: Some("Existing Metadata".into()),
        title_override: None,
        updated_at: now - chrono::Duration::seconds(10),
        created_at: Some(now - chrono::Duration::seconds(10)),
        interacted_at: None,
        worktree_paths: WorktreePaths::from_folder_paths(&project_a_paths),
        remote_connection: None,
        archived: false,
    };

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(existing_metadata, cx);
        });
    });
    cx.run_until_parked();

    let threads_to_save = vec![
        (
            "a-session-0",
            "Thread A0 From Native Store",
            project_a_paths.clone(),
            now,
        ),
        (
            "a-session-1",
            "Thread A1",
            project_a_paths.clone(),
            now + chrono::Duration::seconds(1),
        ),
        (
            "b-session-0",
            "Thread B0",
            project_b_paths.clone(),
            now + chrono::Duration::seconds(2),
        ),
        (
            "projectless",
            "Projectless",
            PathList::default(),
            now + chrono::Duration::seconds(3),
        ),
    ];

    for (session_id, title, paths, updated_at) in &threads_to_save {
        let save_task = cx.update(|cx| {
            let thread_store = ThreadStore::global(cx);
            let session_id = session_id.to_string();
            let title = title.to_string();
            let paths = paths.clone();
            thread_store.update(cx, |store, cx| {
                store.save_thread(
                    acp::SessionId::new(session_id),
                    make_db_thread(&title, *updated_at),
                    paths,
                    cx,
                )
            })
        });
        save_task.await.unwrap();
        cx.run_until_parked();
    }

    run_store_migrations(cx);

    let list = cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.read(cx).entries().cloned().collect::<Vec<_>>()
    });

    assert_eq!(list.len(), 4);
    assert!(
        list.iter()
            .all(|metadata| metadata.agent_id.as_ref() == agent::MAV_AGENT_ID.as_ref())
    );

    let existing_metadata = list
        .iter()
        .find(|metadata| {
            metadata
                .session_id
                .as_ref()
                .is_some_and(|s| s.0.as_ref() == "a-session-0")
        })
        .unwrap();
    assert_eq!(existing_metadata.display_title(), "Existing Metadata");
    assert!(!existing_metadata.archived);

    let migrated_session_ids: Vec<_> = list
        .iter()
        .filter_map(|metadata| metadata.session_id.as_ref().map(|s| s.0.to_string()))
        .collect();
    assert!(migrated_session_ids.iter().any(|s| s == "a-session-1"));
    assert!(migrated_session_ids.iter().any(|s| s == "b-session-0"));
    assert!(migrated_session_ids.iter().any(|s| s == "projectless"));

    // The per-batch top-5 rescue applies: each migrated thread that has
    // a project becomes the most-recent-in-its-project within this batch
    // and is unarchived. Only the projectless thread stays archived,
    // because the rescue only applies to threads with a folder path.
    let migrated_by_session: HashMap<String, &ThreadMetadata> = list
        .iter()
        .filter_map(|metadata| {
            let session_id = metadata.session_id.as_ref()?.0.to_string();
            (session_id != "a-session-0").then_some((session_id, metadata))
        })
        .collect();
    assert!(!migrated_by_session["a-session-1"].archived);
    assert!(!migrated_by_session["b-session-0"].archived);
    assert!(migrated_by_session["projectless"].archived);
}

#[gpui::test]
async fn test_migrate_thread_metadata_noops_when_all_threads_already_exist(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let project_paths = PathList::new(&[Path::new("/project-a")]);
    let existing_updated_at = Utc::now();

    let existing_metadata = ThreadMetadata {
        thread_id: ThreadId::new(),
        session_id: Some(acp::SessionId::new("existing-session")),
        agent_id: agent::MAV_AGENT_ID.clone(),
        title: Some("Existing Metadata".into()),
        title_override: None,
        updated_at: existing_updated_at,
        created_at: Some(existing_updated_at),
        interacted_at: None,
        worktree_paths: WorktreePaths::from_folder_paths(&project_paths),
        remote_connection: None,
        archived: false,
    };

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(existing_metadata, cx);
        });
    });
    cx.run_until_parked();

    let save_task = cx.update(|cx| {
        let thread_store = ThreadStore::global(cx);
        thread_store.update(cx, |store, cx| {
            store.save_thread(
                acp::SessionId::new("existing-session"),
                make_db_thread(
                    "Updated Native Thread Title",
                    existing_updated_at + chrono::Duration::seconds(1),
                ),
                project_paths.clone(),
                cx,
            )
        })
    });
    save_task.await.unwrap();
    cx.run_until_parked();

    run_store_migrations(cx);

    let list = cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.read(cx).entries().cloned().collect::<Vec<_>>()
    });

    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].session_id.as_ref().unwrap().0.as_ref(),
        "existing-session"
    );
}

#[gpui::test]
async fn test_migrate_thread_remote_connections_backfills_from_workspace_db(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let folder_paths = PathList::new(&[Path::new("/remote-project")]);
    let updated_at = Utc::now();
    let metadata = make_metadata(
        "remote-session",
        "Remote Thread",
        updated_at,
        folder_paths.clone(),
    );

    cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });
    cx.run_until_parked();

    let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
    let workspace_id = workspace_db.next_id().await.unwrap();
    let serialized_paths = folder_paths.serialize();
    let remote_connection_id = 1_i64;
    workspace_db
        .write(move |conn| {
            let mut stmt = Statement::prepare(
                conn,
                "INSERT INTO remote_connections(id, kind, user, distro) VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut next_index = stmt.bind(&remote_connection_id, 1)?;
            next_index = stmt.bind(&"wsl", next_index)?;
            next_index = stmt.bind(&Some("anth".to_string()), next_index)?;
            stmt.bind(&Some("Ubuntu".to_string()), next_index)?;
            stmt.exec()?;

            let mut stmt = Statement::prepare(
                conn,
                "UPDATE workspaces SET paths = ?2, paths_order = ?3, remote_connection_id = ?4, timestamp = CURRENT_TIMESTAMP WHERE workspace_id = ?1",
            )?;
            let mut next_index = stmt.bind(&workspace_id, 1)?;
            next_index = stmt.bind(&serialized_paths.paths, next_index)?;
            next_index = stmt.bind(&serialized_paths.order, next_index)?;
            stmt.bind(&Some(remote_connection_id as i32), next_index)?;
            stmt.exec()
        })
        .await
        .unwrap();

    clear_thread_metadata_remote_connection_backfill(cx);
    cx.update(|cx| {
        migrate_thread_remote_connections(cx, Task::ready(Ok(())));
    });
    cx.run_until_parked();

    let metadata = cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store
            .read(cx)
            .entry_by_session(&acp::SessionId::new("remote-session"))
            .cloned()
            .expect("expected migrated metadata row")
    });

    assert_eq!(
        metadata.remote_connection,
        Some(RemoteConnectionOptions::Wsl(WslConnectionOptions {
            distro_name: "Ubuntu".to_string(),
            user: Some("anth".to_string()),
        }))
    );
}

#[gpui::test]
async fn test_migrate_thread_metadata_archives_beyond_five_most_recent_per_project(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let project_a_paths = PathList::new(&[Path::new("/project-a")]);
    let project_b_paths = PathList::new(&[Path::new("/project-b")]);
    let now = Utc::now();

    // Create 7 threads for project A and 3 for project B
    let mut threads_to_save = Vec::new();
    for i in 0..7 {
        threads_to_save.push((
            format!("a-session-{i}"),
            format!("Thread A{i}"),
            project_a_paths.clone(),
            now + chrono::Duration::seconds(i as i64),
        ));
    }
    for i in 0..3 {
        threads_to_save.push((
            format!("b-session-{i}"),
            format!("Thread B{i}"),
            project_b_paths.clone(),
            now + chrono::Duration::seconds(i as i64),
        ));
    }

    for (session_id, title, paths, updated_at) in &threads_to_save {
        let save_task = cx.update(|cx| {
            let thread_store = ThreadStore::global(cx);
            let session_id = session_id.to_string();
            let title = title.to_string();
            let paths = paths.clone();
            thread_store.update(cx, |store, cx| {
                store.save_thread(
                    acp::SessionId::new(session_id),
                    make_db_thread(&title, *updated_at),
                    paths,
                    cx,
                )
            })
        });
        save_task.await.unwrap();
        cx.run_until_parked();
    }

    run_store_migrations(cx);

    let list = cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.read(cx).entries().cloned().collect::<Vec<_>>()
    });

    assert_eq!(list.len(), 10);

    // Project A: 5 most recent should be unarchived, 2 oldest should be archived
    let mut project_a_entries: Vec<_> = list
        .iter()
        .filter(|m| *m.folder_paths() == project_a_paths)
        .collect();
    assert_eq!(project_a_entries.len(), 7);
    project_a_entries.sort_by_key(|entry| std::cmp::Reverse(entry.updated_at));

    for entry in &project_a_entries[..5] {
        assert!(
            !entry.archived,
            "Expected {:?} to be unarchived (top 5 most recent)",
            entry.session_id
        );
    }
    for entry in &project_a_entries[5..] {
        assert!(
            entry.archived,
            "Expected {:?} to be archived (older than top 5)",
            entry.session_id
        );
    }

    // Project B: all 3 should be unarchived (under the limit)
    let project_b_entries: Vec<_> = list
        .iter()
        .filter(|m| *m.folder_paths() == project_b_paths)
        .collect();
    assert_eq!(project_b_entries.len(), 3);
    assert!(project_b_entries.iter().all(|m| !m.archived));
}

// Regression test for the race between `ThreadStore::reload` and
// `migrate_thread_metadata`. `ThreadStore::new` constructs with an empty
// in-memory cache and kicks off `reload()` as a fire-and-forget task. If
// `migrate_thread_metadata` reads `ThreadStore::entries()` before that
// reload completes, it observes an empty iterator and no-ops, even though
// the on-disk legacy DB has threads to migrate. In production this
// manifests as "my old threads disappeared after upgrading": the threads
// are still in the legacy `threads.db`, but never make it into
// `sidebar_threads`, so the new sidebar UI can't see them.
#[gpui::test]
async fn test_migration_awaits_thread_store_reload(cx: &mut TestAppContext) {
    init_test(cx);

    // Seed the legacy threads DB via the ThreadStore (the only public
    // save path in this crate), then park to make sure the rows are on
    // disk and `ThreadStore`'s in-memory cache is populated.
    let project_paths = PathList::new(&[Path::new("/project-a")]);
    let now = Utc::now();
    for i in 0..3 {
        let save_task = cx.update(|cx| {
            let thread_store = ThreadStore::global(cx);
            let session_id = format!("legacy-session-{i}");
            let title = format!("Legacy Thread {i}");
            let updated_at = now + chrono::Duration::seconds(i as i64);
            let paths = project_paths.clone();
            thread_store.update(cx, |store, cx| {
                store.save_thread(
                    acp::SessionId::new(session_id),
                    make_db_thread(&title, updated_at),
                    paths,
                    cx,
                )
            })
        });
        save_task.await.unwrap();
        cx.run_until_parked();
    }

    // Re-initialize `ThreadStore` so its in-memory cache is freshly empty
    // and a new async `reload` task is kicked off. This reproduces the
    // cold-boot state where the migration runs before the store has
    // populated itself from disk. The on-disk legacy DB still has the
    // three threads we saved above.
    cx.update(|cx| ThreadStore::init_global(cx));

    // Crucially: do NOT run_until_parked here. If we parked, the reload
    // would complete, ThreadStore::entries() would return the 3 rows, and
    // the race would be hidden. We want the migration to run with
    // `ThreadStore::entries()` still returning an empty iterator.
    run_store_migrations(cx);

    let list = cx.update(|cx| {
        let store = ThreadMetadataStore::global(cx);
        store.read(cx).entries().cloned().collect::<Vec<_>>()
    });

    assert_eq!(
        list.len(),
        3,
        "Expected migration to pick up all 3 legacy threads even when \
         ThreadStore::reload has not yet completed, but got {} entries",
        list.len()
    );
}
