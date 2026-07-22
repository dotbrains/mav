use super::*;

impl ThreadMetadataStore {
    #[cfg(not(any(test, feature = "test-support")))]
    pub fn init_global(cx: &mut App) {
        if cx.has_global::<Self>() {
            return;
        }

        let db = ThreadMetadataDb::global(cx);
        let thread_store = cx.new(|cx| Self::new(db, cx));
        cx.set_global(GlobalThreadMetadataStore(thread_store));
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn init_global(cx: &mut App) {
        let db_name = TestMetadataDbName::global(cx);
        let db = gpui::block_on(db::open_test_db::<ThreadMetadataDb>(&db_name));
        let thread_store = cx.new(|cx| Self::new(ThreadMetadataDb(db), cx));
        cx.set_global(GlobalThreadMetadataStore(thread_store));
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalThreadMetadataStore>()
            .map(|store| store.0.clone())
    }

    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalThreadMetadataStore>().0.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.threads.is_empty()
    }

    /// Returns all thread IDs.
    pub fn entry_ids(&self) -> impl Iterator<Item = ThreadId> + '_ {
        self.threads.keys().copied()
    }

    /// Returns the metadata for a specific thread, if it exists.
    pub fn entry(&self, thread_id: ThreadId) -> Option<&ThreadMetadata> {
        self.threads.get(&thread_id)
    }

    /// Returns the metadata for a thread identified by its ACP session ID.
    pub fn entry_by_session(&self, session_id: &acp::SessionId) -> Option<&ThreadMetadata> {
        let thread_id = self.threads_by_session.get(session_id)?;
        self.threads.get(thread_id)
    }

    /// Returns all threads.
    pub fn entries(&self) -> impl Iterator<Item = &ThreadMetadata> + '_ {
        self.threads.values()
    }

    pub fn reload_task(&self) -> Shared<Task<()>> {
        self.reload_task
            .clone()
            .unwrap_or_else(|| Task::ready(()).shared())
    }

    /// Returns all archived threads.
    pub fn archived_entries(&self) -> impl Iterator<Item = &ThreadMetadata> + '_ {
        self.entries().filter(|t| t.archived)
    }

    /// Returns all threads for the given path list and remote connection,
    /// excluding archived threads.
    ///
    /// When `remote_connection` is `Some`, only threads whose persisted
    /// `remote_connection` matches by normalized identity are returned.
    /// When `None`, only local (non-remote) threads are returned.
    pub fn entries_for_path<'a>(
        &'a self,
        path_list: &PathList,
        remote_connection: Option<&'a RemoteConnectionOptions>,
    ) -> impl Iterator<Item = &'a ThreadMetadata> + 'a {
        self.threads_by_paths
            .get(path_list)
            .into_iter()
            .flatten()
            .filter_map(|s| self.threads.get(s))
            .filter(|s| !s.archived)
            .filter(move |s| s.matches_remote_connection(remote_connection))
    }

    /// Returns threads whose `main_worktree_paths` matches the given path list
    /// and remote connection, excluding archived threads. This finds threads
    /// that were opened in a linked worktree but are associated with the given
    /// main worktree.
    ///
    /// When `remote_connection` is `Some`, only threads whose persisted
    /// `remote_connection` matches by normalized identity are returned.
    /// When `None`, only local (non-remote) threads are returned.
    pub fn entries_for_main_worktree_path<'a>(
        &'a self,
        path_list: &PathList,
        remote_connection: Option<&'a RemoteConnectionOptions>,
    ) -> impl Iterator<Item = &'a ThreadMetadata> + 'a {
        self.threads_by_main_paths
            .get(path_list)
            .into_iter()
            .flatten()
            .filter_map(|s| self.threads.get(s))
            .filter(|s| !s.archived)
            .filter(move |s| s.matches_remote_connection(remote_connection))
    }

    fn reload(&mut self, cx: &mut Context<Self>) -> Shared<Task<()>> {
        let db = self.db.clone();
        self.reload_task.take();

        let list_task = cx
            .background_spawn(async move { db.list().context("Failed to fetch sidebar metadata") });

        let reload_task = cx
            .spawn(async move |this, cx| {
                let Some(rows) = list_task.await.log_err() else {
                    return;
                };

                this.update(cx, |this, cx| {
                    this.threads.clear();
                    this.threads_by_paths.clear();
                    this.threads_by_main_paths.clear();
                    this.threads_by_session.clear();

                    for row in rows {
                        this.cache_thread_metadata(row);
                    }

                    cx.notify();
                })
                .ok();
            })
            .shared();
        self.reload_task = Some(reload_task.clone());
        reload_task
    }

    pub fn save_all(&mut self, metadata: Vec<ThreadMetadata>, cx: &mut Context<Self>) {
        for metadata in metadata {
            self.save_internal(metadata);
        }
        cx.notify();
    }

    pub fn save(&mut self, metadata: ThreadMetadata, cx: &mut Context<Self>) {
        self.save_internal(metadata);
        cx.notify();
    }

    /// Set or clear the user-supplied title for a thread.
    pub fn set_title_override(
        &mut self,
        thread_id: ThreadId,
        title_override: SharedString,
        cx: &mut Context<Self>,
    ) {
        let Some(existing) = self.entry(thread_id) else {
            return;
        };
        if existing.title_override.as_ref() == Some(&title_override) {
            return;
        }
        let metadata = ThreadMetadata {
            title_override: Some(title_override),
            ..existing.clone()
        };
        self.save(metadata, cx);
    }

    pub fn set_generated_title(
        &mut self,
        thread_id: ThreadId,
        title: SharedString,
        cx: &mut Context<Self>,
    ) {
        let Some(existing) = self.entry(thread_id) else {
            return;
        };
        if existing.title.as_ref() == Some(&title) && existing.title_override.is_none() {
            return;
        }
        let metadata = ThreadMetadata {
            title: Some(title),
            title_override: None,
            ..existing.clone()
        };
        self.save(metadata, cx);
    }
}
