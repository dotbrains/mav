use super::*;

impl ThreadMetadataDb {
    #[allow(dead_code)]
    pub fn list_ids(&self) -> anyhow::Result<Vec<ThreadId>> {
        self.select::<ThreadId>(
            "SELECT thread_id FROM sidebar_threads \
             ORDER BY updated_at DESC",
        )?()
    }

    const LIST_QUERY: &str = "SELECT thread_id, session_id, agent_id, title, updated_at, \
        created_at, interacted_at, folder_paths, folder_paths_order, archived, main_worktree_paths, \
        main_worktree_paths_order, remote_connection, title_override \
        FROM sidebar_threads \
        ORDER BY updated_at DESC";

    /// List all sidebar thread metadata, ordered by updated_at descending.
    ///
    /// Only returns threads that have a `session_id`.
    pub fn list(&self) -> anyhow::Result<Vec<ThreadMetadata>> {
        self.select::<ThreadMetadata>(Self::LIST_QUERY)?()
    }

    /// Upsert metadata for a thread.
    ///
    /// Drafts are persisted with `session_id = None`. They get a real
    /// session_id on promotion (when the first message is sent) and
    /// then flow through this same upsert path.
    pub async fn save(&self, row: ThreadMetadata) -> anyhow::Result<()> {
        let session_id = row.session_id.as_ref().map(|s| s.0.clone());
        let agent_id = if row.agent_id.as_ref() == MAV_AGENT_ID.as_ref() {
            None
        } else {
            Some(row.agent_id.to_string())
        };
        let title = row
            .title
            .as_ref()
            .map(|t| t.to_string())
            .unwrap_or_default();
        let updated_at = row.updated_at.to_rfc3339();
        let created_at = row.created_at.map(|dt| dt.to_rfc3339());
        let interacted_at = row.interacted_at.map(|dt| dt.to_rfc3339());
        let serialized = row.folder_paths().serialize();
        let (folder_paths, folder_paths_order) = if row.folder_paths().is_empty() {
            (None, None)
        } else {
            (Some(serialized.paths), Some(serialized.order))
        };
        let main_serialized = row.main_worktree_paths().serialize();
        let (main_worktree_paths, main_worktree_paths_order) =
            if row.main_worktree_paths().is_empty() {
                (None, None)
            } else {
                (Some(main_serialized.paths), Some(main_serialized.order))
            };
        let remote_connection = row
            .remote_connection
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .context("serialize thread metadata remote connection")?;
        let title_override = row.title_override.as_ref().map(|t| t.to_string());
        let thread_id = row.thread_id;
        let archived = row.archived;

        self.write(move |conn| {
            let sql = "INSERT INTO sidebar_threads(thread_id, session_id, agent_id, title, updated_at, created_at, interacted_at, folder_paths, folder_paths_order, archived, main_worktree_paths, main_worktree_paths_order, remote_connection, title_override) \
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
                       ON CONFLICT(thread_id) DO UPDATE SET \
                           session_id = excluded.session_id, \
                           agent_id = excluded.agent_id, \
                           title = excluded.title, \
                           updated_at = excluded.updated_at, \
                           created_at = excluded.created_at, \
                           interacted_at = excluded.interacted_at, \
                           folder_paths = excluded.folder_paths, \
                           folder_paths_order = excluded.folder_paths_order, \
                           archived = excluded.archived, \
                           main_worktree_paths = excluded.main_worktree_paths, \
                           main_worktree_paths_order = excluded.main_worktree_paths_order, \
                           remote_connection = excluded.remote_connection, \
                           title_override = excluded.title_override";
            let mut stmt = Statement::prepare(conn, sql)?;
            let mut i = stmt.bind(&thread_id, 1)?;
            i = stmt.bind(&session_id, i)?;
            i = stmt.bind(&agent_id, i)?;
            i = stmt.bind(&title, i)?;
            i = stmt.bind(&updated_at, i)?;
            i = stmt.bind(&created_at, i)?;
            i = stmt.bind(&interacted_at, i)?;
            i = stmt.bind(&folder_paths, i)?;
            i = stmt.bind(&folder_paths_order, i)?;
            i = stmt.bind(&archived, i)?;
            i = stmt.bind(&main_worktree_paths, i)?;
            i = stmt.bind(&main_worktree_paths_order, i)?;
            i = stmt.bind(&remote_connection, i)?;
            stmt.bind(&title_override, i)?;
            stmt.exec()
        })
        .await
    }

    /// Delete metadata for a single thread.
    pub async fn delete(&self, thread_id: ThreadId) -> anyhow::Result<()> {
        self.write(move |conn| {
            let mut stmt =
                Statement::prepare(conn, "DELETE FROM sidebar_threads WHERE thread_id = ?")?;
            stmt.bind(&thread_id, 1)?;
            stmt.exec()
        })
        .await
    }

    pub async fn create_archived_worktree(
        &self,
        worktree_path: String,
        main_repo_path: String,
        branch_name: Option<String>,
        staged_commit_hash: String,
        unstaged_commit_hash: String,
        original_commit_hash: String,
    ) -> anyhow::Result<i64> {
        self.write(move |conn| {
            let mut stmt = Statement::prepare(
                conn,
                "INSERT INTO archived_git_worktrees(worktree_path, main_repo_path, branch_name, staged_commit_hash, unstaged_commit_hash, original_commit_hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 RETURNING id",
            )?;
            let mut i = stmt.bind(&worktree_path, 1)?;
            i = stmt.bind(&main_repo_path, i)?;
            i = stmt.bind(&branch_name, i)?;
            i = stmt.bind(&staged_commit_hash, i)?;
            i = stmt.bind(&unstaged_commit_hash, i)?;
            stmt.bind(&original_commit_hash, i)?;
            stmt.maybe_row::<i64>()?.context("expected RETURNING id")
        })
        .await
    }

    pub async fn link_thread_to_archived_worktree(
        &self,
        thread_id: ThreadId,
        archived_worktree_id: i64,
    ) -> anyhow::Result<()> {
        self.write(move |conn| {
            let mut stmt = Statement::prepare(
                conn,
                "INSERT INTO thread_archived_worktrees(thread_id, archived_worktree_id) \
                 VALUES (?1, ?2)",
            )?;
            let i = stmt.bind(&thread_id, 1)?;
            stmt.bind(&archived_worktree_id, i)?;
            stmt.exec()
        })
        .await
    }

    pub async fn get_archived_worktrees_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> anyhow::Result<Vec<ArchivedGitWorktree>> {
        self.select_bound::<ThreadId, ArchivedGitWorktree>(
            "SELECT a.id, a.worktree_path, a.main_repo_path, a.branch_name, a.staged_commit_hash, a.unstaged_commit_hash, a.original_commit_hash \
             FROM archived_git_worktrees a \
             JOIN thread_archived_worktrees t ON a.id = t.archived_worktree_id \
             WHERE t.thread_id = ?1",
        )?(thread_id)
    }

    pub async fn delete_archived_worktree(&self, id: i64) -> anyhow::Result<()> {
        self.write(move |conn| {
            let mut stmt = Statement::prepare(
                conn,
                "DELETE FROM thread_archived_worktrees WHERE archived_worktree_id = ?",
            )?;
            stmt.bind(&id, 1)?;
            stmt.exec()?;

            let mut stmt =
                Statement::prepare(conn, "DELETE FROM archived_git_worktrees WHERE id = ?")?;
            stmt.bind(&id, 1)?;
            stmt.exec()
        })
        .await
    }

    pub async fn unlink_thread_from_all_archived_worktrees(
        &self,
        thread_id: ThreadId,
    ) -> anyhow::Result<()> {
        self.write(move |conn| {
            let mut stmt = Statement::prepare(
                conn,
                "DELETE FROM thread_archived_worktrees WHERE thread_id = ?",
            )?;
            stmt.bind(&thread_id, 1)?;
            stmt.exec()
        })
        .await
    }

    pub async fn is_archived_worktree_referenced(
        &self,
        archived_worktree_id: i64,
    ) -> anyhow::Result<bool> {
        self.select_row_bound::<i64, i64>(
            "SELECT COUNT(*) FROM thread_archived_worktrees WHERE archived_worktree_id = ?1",
        )?(archived_worktree_id)
        .map(|count| count.unwrap_or(0) > 0)
    }

    pub fn get_all_archived_branch_names(
        &self,
    ) -> anyhow::Result<HashMap<ThreadId, HashMap<PathBuf, String>>> {
        let rows = self.select::<(ThreadId, String, String)>(
            "SELECT t.thread_id, a.worktree_path, a.branch_name \
             FROM thread_archived_worktrees t \
             JOIN archived_git_worktrees a ON a.id = t.archived_worktree_id \
             WHERE a.branch_name IS NOT NULL \
             ORDER BY a.id ASC",
        )?()?;

        let mut result: HashMap<ThreadId, HashMap<PathBuf, String>> = HashMap::default();
        for (thread_id, worktree_path, branch_name) in rows {
            result
                .entry(thread_id)
                .or_default()
                .insert(PathBuf::from(worktree_path), branch_name);
        }
        Ok(result)
    }
}
