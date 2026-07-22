use super::*;

impl ThreadMetadataStore {
    fn new(db: ThreadMetadataDb, cx: &mut Context<Self>) -> Self {
        let weak_store = cx.weak_entity();

        cx.observe_new::<crate::ConversationView>(move |_view, _window, cx| {
            let view_entity = cx.entity();
            let entity_id = view_entity.entity_id();

            cx.on_release({
                let weak_store = weak_store.clone();
                move |_view, cx| {
                    weak_store
                        .update(cx, |store, _cx| {
                            store.conversation_subscriptions.remove(&entity_id);
                        })
                        .ok();
                }
            })
            .detach();

            weak_store
                .update(cx, |this, cx| {
                    let subscription = cx.subscribe(&view_entity, Self::handle_conversation_event);
                    this.conversation_subscriptions
                        .insert(entity_id, subscription);
                })
                .ok();
        })
        .detach();

        let (tx, rx) = async_channel::unbounded();
        let _db_operations_task = cx.background_spawn({
            let db = db.clone();
            async move {
                while let Ok(first_update) = rx.recv().await {
                    let mut updates = vec![first_update];
                    while let Ok(update) = rx.try_recv() {
                        updates.push(update);
                    }
                    let updates = Self::dedup_db_operations(updates);
                    for operation in updates {
                        match operation {
                            DbOperation::Upsert(metadata) => {
                                db.save(metadata).await.log_err();
                            }
                            DbOperation::Delete(thread_id) => {
                                db.delete(thread_id).await.log_err();
                            }
                        }
                    }
                }
            }
        });

        let mut this = Self {
            db,
            threads: HashMap::default(),
            threads_by_paths: HashMap::default(),
            threads_by_main_paths: HashMap::default(),
            threads_by_session: HashMap::default(),
            reload_task: None,
            conversation_subscriptions: HashMap::default(),
            pending_thread_ops_tx: tx,
            in_flight_archives: HashMap::default(),
            _db_operations_task,
        };
        let _ = this.reload(cx);
        this
    }

    fn dedup_db_operations(operations: Vec<DbOperation>) -> Vec<DbOperation> {
        let mut ops = HashMap::default();
        for operation in operations.into_iter().rev() {
            if ops.contains_key(&operation.id()) {
                continue;
            }
            ops.insert(operation.id(), operation);
        }
        ops.into_values().collect()
    }

    fn handle_conversation_event(
        &mut self,
        conversation_view: Entity<crate::ConversationView>,
        _event: &crate::conversation_view::RootThreadUpdated,
        cx: &mut Context<Self>,
    ) {
        let view = conversation_view.read(cx);
        let thread_id = view.thread_id;
        let Some(thread) = view.root_thread(cx) else {
            return;
        };

        let is_draft = view.is_draft(cx);
        let thread_ref = thread.read(cx);
        // Collab-hosted threads don't own their metadata locally.
        if thread_ref.project().read(cx).is_via_collab() {
            return;
        }
        let existing_thread = self.entry(thread_id);

        // New ACP sessions exist before the user sends. Keep draft metadata
        // sessionless until the conversation is promoted by user input.
        let session_id = if is_draft {
            None
        } else {
            Some(thread_ref.session_id().clone())
        };
        let title = if is_draft { None } else { thread_ref.title() };
        let title_override = existing_thread.and_then(|t| t.title_override.clone());

        let updated_at = Utc::now();

        let created_at = existing_thread
            .and_then(|t| t.created_at)
            .unwrap_or_else(|| updated_at);

        let interacted_at = existing_thread
            .map(|t| t.interacted_at)
            .unwrap_or(Some(updated_at));

        let agent_id = thread_ref.connection().agent_id();

        // Preserve project-dependent fields for archived threads.
        // The worktree may already have been removed from the
        // project as part of the archive flow, so re-evaluating
        // these from the current project state would yield
        // empty/incorrect results.
        let (worktree_paths, remote_connection) =
            if let Some(existing) = existing_thread.filter(|t| t.archived) {
                (
                    existing.worktree_paths.clone(),
                    existing.remote_connection.clone(),
                )
            } else {
                let project = thread_ref.project().read(cx);
                let worktree_paths = project.worktree_paths(cx);
                let remote_connection = project.remote_connection_options(cx);

                (worktree_paths, remote_connection)
            };

        // Threads without a folder path (e.g. started in an empty
        // window) are archived by default so they don't get lost,
        // because they won't show up in the sidebar. Users can reload
        // them from the archive.
        let archived = existing_thread
            .map(|t| t.archived)
            .unwrap_or(worktree_paths.is_empty());

        let was_draft = existing_thread.map_or(true, |t| t.is_draft());
        if was_draft && !is_draft {
            // Draft has been promoted: drop its persisted prompt since the
            // promoted thread now owns its prompt state via the native
            // agent's thread database.
            crate::draft_prompt_store::delete(thread_id, cx).detach_and_log_err(cx);
        }

        let metadata = ThreadMetadata {
            thread_id,
            session_id,
            agent_id,
            title,
            title_override,
            created_at: Some(created_at),
            interacted_at,
            updated_at,
            worktree_paths,
            remote_connection,
            archived,
        };

        self.save(metadata, cx);
    }
}
