use super::*;

impl ThreadMetadataStore {
    pub fn create_archived_worktree(
        &self,
        worktree_path: String,
        main_repo_path: String,
        branch_name: Option<String>,
        staged_commit_hash: String,
        unstaged_commit_hash: String,
        original_commit_hash: String,
        cx: &App,
    ) -> Task<anyhow::Result<i64>> {
        let db = self.db.clone();
        cx.background_spawn(async move {
            db.create_archived_worktree(
                worktree_path,
                main_repo_path,
                branch_name,
                staged_commit_hash,
                unstaged_commit_hash,
                original_commit_hash,
            )
            .await
        })
    }

    pub fn link_thread_to_archived_worktree(
        &self,
        thread_id: ThreadId,
        archived_worktree_id: i64,
        cx: &App,
    ) -> Task<anyhow::Result<()>> {
        let db = self.db.clone();
        cx.background_spawn(async move {
            db.link_thread_to_archived_worktree(thread_id, archived_worktree_id)
                .await
        })
    }

    pub fn get_archived_worktrees_for_thread(
        &self,
        thread_id: ThreadId,
        cx: &App,
    ) -> Task<anyhow::Result<Vec<ArchivedGitWorktree>>> {
        let db = self.db.clone();
        cx.background_spawn(async move { db.get_archived_worktrees_for_thread(thread_id).await })
    }

    pub fn delete_archived_worktree(&self, id: i64, cx: &App) -> Task<anyhow::Result<()>> {
        let db = self.db.clone();
        cx.background_spawn(async move { db.delete_archived_worktree(id).await })
    }

    pub fn unlink_thread_from_all_archived_worktrees(
        &self,
        thread_id: ThreadId,
        cx: &App,
    ) -> Task<anyhow::Result<()>> {
        let db = self.db.clone();
        cx.background_spawn(async move {
            db.unlink_thread_from_all_archived_worktrees(thread_id)
                .await
        })
    }

    pub fn is_archived_worktree_referenced(
        &self,
        archived_worktree_id: i64,
        cx: &App,
    ) -> Task<anyhow::Result<bool>> {
        let db = self.db.clone();
        cx.background_spawn(async move {
            db.is_archived_worktree_referenced(archived_worktree_id)
                .await
        })
    }

    pub fn get_all_archived_branch_names(
        &self,
        cx: &App,
    ) -> Task<anyhow::Result<HashMap<ThreadId, HashMap<PathBuf, String>>>> {
        let db = self.db.clone();
        cx.background_spawn(async move { db.get_all_archived_branch_names() })
    }

    fn update_archived(&mut self, thread_id: ThreadId, archived: bool, cx: &mut Context<Self>) {
        if let Some(thread) = self.threads.get(&thread_id) {
            self.save_internal(ThreadMetadata {
                archived,
                ..thread.clone()
            });
            cx.notify();
        }
    }

    pub fn delete(&mut self, thread_id: ThreadId, cx: &mut Context<Self>) {
        if let Some(thread) = self.threads.get(&thread_id) {
            if let Some(sid) = &thread.session_id {
                self.threads_by_session.remove(sid);
            }
            if let Some(thread_ids) = self.threads_by_paths.get_mut(thread.folder_paths()) {
                thread_ids.remove(&thread_id);
            }
            if !thread.main_worktree_paths().is_empty() {
                if let Some(thread_ids) = self
                    .threads_by_main_paths
                    .get_mut(thread.main_worktree_paths())
                {
                    thread_ids.remove(&thread_id);
                }
            }
        }
        self.threads.remove(&thread_id);
        self.pending_thread_ops_tx
            .try_send(DbOperation::Delete(thread_id))
            .log_err();
        crate::draft_prompt_store::delete(thread_id, cx).detach_and_log_err(cx);
        cx.notify();
    }

    pub fn unarchived_draft_ids_matching(
        &self,
        matches: impl Fn(&ThreadMetadata) -> bool,
    ) -> Vec<ThreadId> {
        self.entries()
            .filter(|thread| thread.is_draft() && !thread.archived && matches(thread))
            .map(|thread| thread.thread_id)
            .collect()
    }

    pub fn delete_all(
        &mut self,
        thread_ids: impl IntoIterator<Item = ThreadId>,
        cx: &mut Context<Self>,
    ) {
        for thread_id in thread_ids {
            self.delete(thread_id, cx);
        }
    }
}
