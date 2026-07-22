use super::*;

impl ThreadMetadataStore {
    fn save_internal(&mut self, metadata: ThreadMetadata) {
        if let Some(thread) = self.threads.get(&metadata.thread_id) {
            if thread.folder_paths() != metadata.folder_paths() {
                if let Some(thread_ids) = self.threads_by_paths.get_mut(thread.folder_paths()) {
                    thread_ids.remove(&metadata.thread_id);
                }
            }
            if thread.main_worktree_paths() != metadata.main_worktree_paths()
                && !thread.main_worktree_paths().is_empty()
            {
                if let Some(thread_ids) = self
                    .threads_by_main_paths
                    .get_mut(thread.main_worktree_paths())
                {
                    thread_ids.remove(&metadata.thread_id);
                }
            }
        }

        self.cache_thread_metadata(metadata.clone());
        self.pending_thread_ops_tx
            .try_send(DbOperation::Upsert(metadata))
            .log_err();
    }

    fn cache_thread_metadata(&mut self, metadata: ThreadMetadata) {
        // Drafts may not have a session_id yet; only index by session
        // when one is present.
        if let Some(session_id) = metadata.session_id.as_ref() {
            self.threads_by_session
                .insert(session_id.clone(), metadata.thread_id);
        }

        self.threads.insert(metadata.thread_id, metadata.clone());

        self.threads_by_paths
            .entry(metadata.folder_paths().clone())
            .or_default()
            .insert(metadata.thread_id);

        if !metadata.main_worktree_paths().is_empty() {
            self.threads_by_main_paths
                .entry(metadata.main_worktree_paths().clone())
                .or_default()
                .insert(metadata.thread_id);
        }
    }
}
