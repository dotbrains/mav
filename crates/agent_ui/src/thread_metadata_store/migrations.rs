use super::*;

pub fn init(cx: &mut App) {
    ThreadMetadataStore::init_global(cx);
    let migration_task = migrate_thread_metadata(cx);
    migrate_thread_remote_connections(cx, migration_task);
    migrate_thread_ids(cx);
}

/// Migrate existing thread metadata from native agent thread store to the new metadata storage.
/// We skip migrating threads that do not have a project.
///
/// TODO: Remove this after N weeks of shipping the sidebar
fn migrate_thread_metadata(cx: &mut App) -> Task<anyhow::Result<()>> {
    let store = ThreadMetadataStore::global(cx);
    let db = store.read(cx).db.clone();
    let thread_store = ThreadStore::global(cx);
    let thread_store_ready = thread_store.read(cx).reload_task();

    cx.spawn(async move |cx| {
        // Wait for `ThreadStore`'s initial reload to complete. Without this,
        // reading `entries()` races with the store's async population from
        // disk and usually observes an empty iterator, silently skipping the
        // migration on every launch. The regression test
        // `test_migration_awaits_thread_store_reload` pins this behavior.
        thread_store_ready.await;

        let existing_list = db.list()?;
        let existing_session_ids: HashSet<Arc<str>> = existing_list
            .into_iter()
            .filter_map(|m| m.session_id.map(|s| s.0))
            .collect();

        let mut to_migrate = thread_store.read_with(cx, |store, _cx| {
            store
                .entries()
                .filter_map(|entry| {
                    if existing_session_ids.contains(&entry.id.0) {
                        return None;
                    }

                    Some(ThreadMetadata {
                        thread_id: ThreadId::new(),
                        session_id: Some(entry.id),
                        agent_id: MAV_AGENT_ID.clone(),
                        title: if entry.title.is_empty()
                            || entry.title.as_ref() == DEFAULT_THREAD_TITLE
                        {
                            None
                        } else {
                            Some(entry.title)
                        },
                        title_override: None,
                        updated_at: entry.updated_at,
                        created_at: entry.created_at,
                        interacted_at: None,
                        worktree_paths: WorktreePaths::from_folder_paths(&entry.folder_paths),
                        remote_connection: None,
                        archived: true,
                    })
                })
                .collect::<Vec<_>>()
        });

        if to_migrate.is_empty() {
            return anyhow::Ok(());
        }

        // For each batch of newly-migrated threads, keep the 5 most recent
        // per project unarchived. Previously this was gated on
        // `is_first_migration` (an empty `sidebar_threads`), which meant any
        // subsequent batch of newly-discovered legacy threads got migrated as
        // fully archived. Running the rescue per-batch keeps the behavior
        // idempotent across partial migrations and re-runs.
        let mut per_project: HashMap<PathList, Vec<&mut ThreadMetadata>> = HashMap::default();
        for entry in &mut to_migrate {
            if entry.worktree_paths.is_empty() {
                continue;
            }
            per_project
                .entry(entry.worktree_paths.folder_path_list().clone())
                .or_default()
                .push(entry);
        }
        for entries in per_project.values_mut() {
            entries.sort_by_key(|entry| std::cmp::Reverse(entry.updated_at));
            for entry in entries.iter_mut().take(5) {
                entry.archived = false;
            }
        }

        log::info!("Migrating {} thread store entries", to_migrate.len());

        // Manually save each entry to the database and call reload, otherwise
        // we'll end up triggering lots of reloads after each save
        for entry in to_migrate {
            db.save(entry).await?;
        }

        log::info!("Finished migrating thread store entries");

        let _ = store.update(cx, |store, cx| store.reload(cx));
        anyhow::Ok(())
    })
}

fn migrate_thread_remote_connections(cx: &mut App, migration_task: Task<anyhow::Result<()>>) {
    let store = ThreadMetadataStore::global(cx);
    let db = store.read(cx).db.clone();
    let kvp = KeyValueStore::global(cx);
    let workspace_db = WorkspaceDb::global(cx);
    let fs = <dyn Fs>::global(cx);

    cx.spawn(async move |cx| -> anyhow::Result<()> {
        migration_task.await?;

        if kvp
            .read_kvp(THREAD_REMOTE_CONNECTION_MIGRATION_KEY)?
            .is_some()
        {
            return Ok(());
        }

        let recent_workspaces = workspace_db
            .recent_project_workspaces_ungrouped(fs.as_ref())
            .await?;

        let mut local_path_lists = HashSet::<PathList>::default();
        let mut remote_path_lists = HashMap::<PathList, RemoteConnectionOptions>::default();

        recent_workspaces
            .iter()
            .filter(|workspace| {
                !workspace.paths.is_empty()
                    && matches!(workspace.location, SerializedWorkspaceLocation::Local)
            })
            .for_each(|workspace| {
                local_path_lists.insert(workspace.paths.clone());
            });

        for workspace in recent_workspaces {
            match workspace.location {
                SerializedWorkspaceLocation::Remote(remote_connection)
                    if !local_path_lists.contains(&workspace.paths) =>
                {
                    remote_path_lists
                        .entry(workspace.paths)
                        .or_insert(remote_connection);
                }
                _ => {}
            }
        }

        let mut reloaded = false;
        for metadata in db.list()? {
            if metadata.remote_connection.is_some() {
                continue;
            }

            if let Some(remote_connection) = remote_path_lists
                .get(metadata.folder_paths())
                .or_else(|| remote_path_lists.get(metadata.main_worktree_paths()))
            {
                db.save(ThreadMetadata {
                    remote_connection: Some(remote_connection.clone()),
                    ..metadata
                })
                .await?;
                reloaded = true;
            }
        }

        let reloaded_task = reloaded
            .then_some(store.update(cx, |store, cx| store.reload(cx)))
            .unwrap_or(Task::ready(()).shared());

        kvp.write_kvp(
            THREAD_REMOTE_CONNECTION_MIGRATION_KEY.to_string(),
            "1".to_string(),
        )
        .await?;
        reloaded_task.await;

        Ok(())
    })
    .detach_and_log_err(cx);
}

fn migrate_thread_ids(cx: &mut App) {
    let store = ThreadMetadataStore::global(cx);
    let db = store.read(cx).db.clone();
    let kvp = KeyValueStore::global(cx);

    cx.spawn(async move |cx| -> anyhow::Result<()> {
        if kvp.read_kvp(THREAD_ID_MIGRATION_KEY)?.is_some() {
            return Ok(());
        }

        let mut reloaded = false;
        for metadata in db.list()? {
            db.save(metadata).await?;
            reloaded = true;
        }

        let reloaded_task = reloaded
            .then_some(store.update(cx, |store, cx| store.reload(cx)))
            .unwrap_or(Task::ready(()).shared());

        kvp.write_kvp(THREAD_ID_MIGRATION_KEY.to_string(), "1".to_string())
            .await?;
        reloaded_task.await;

        Ok(())
    })
    .detach_and_log_err(cx);
}
