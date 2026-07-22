use super::*;

impl Column for ThreadMetadata {
    fn column(statement: &mut Statement, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (thread_id_uuid, next): (uuid::Uuid, i32) = Column::column(statement, start_index)?;
        let (id, next): (Option<Arc<str>>, i32) = Column::column(statement, next)?;
        let (agent_id, next): (Option<String>, i32) = Column::column(statement, next)?;
        let (title, next): (String, i32) = Column::column(statement, next)?;
        let (updated_at_str, next): (String, i32) = Column::column(statement, next)?;
        let (created_at_str, next): (Option<String>, i32) = Column::column(statement, next)?;
        let (interacted_at_str, next): (Option<String>, i32) = Column::column(statement, next)?;
        let (folder_paths_str, next): (Option<String>, i32) = Column::column(statement, next)?;
        let (folder_paths_order_str, next): (Option<String>, i32) =
            Column::column(statement, next)?;
        let (archived, next): (bool, i32) = Column::column(statement, next)?;
        let (main_worktree_paths_str, next): (Option<String>, i32) =
            Column::column(statement, next)?;
        let (main_worktree_paths_order_str, next): (Option<String>, i32) =
            Column::column(statement, next)?;
        let (remote_connection_json, next): (Option<String>, i32) =
            Column::column(statement, next)?;
        let (title_override, next): (Option<String>, i32) = Column::column(statement, next)?;

        let agent_id = agent_id
            .map(|id| AgentId::new(id))
            .unwrap_or(MAV_AGENT_ID.clone());

        let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)?.with_timezone(&Utc);
        let created_at = created_at_str
            .as_deref()
            .map(DateTime::parse_from_rfc3339)
            .transpose()?
            .map(|dt| dt.with_timezone(&Utc));

        let interacted_at = interacted_at_str
            .as_deref()
            .map(DateTime::parse_from_rfc3339)
            .transpose()?
            .map(|dt| dt.with_timezone(&Utc));

        let folder_paths = folder_paths_str
            .map(|paths| {
                PathList::deserialize(&util::path_list::SerializedPathList {
                    paths,
                    order: folder_paths_order_str.unwrap_or_default(),
                })
            })
            .unwrap_or_default();

        let main_worktree_paths = main_worktree_paths_str
            .map(|paths| {
                PathList::deserialize(&util::path_list::SerializedPathList {
                    paths,
                    order: main_worktree_paths_order_str.unwrap_or_default(),
                })
            })
            .unwrap_or_default();

        let remote_connection = remote_connection_json
            .as_deref()
            .map(serde_json::from_str::<RemoteConnectionOptions>)
            .transpose()
            .context("deserialize thread metadata remote connection")?;

        let worktree_paths = WorktreePaths::from_path_lists(main_worktree_paths, folder_paths)
            .unwrap_or_else(|_| WorktreePaths::default());

        let thread_id = ThreadId(thread_id_uuid);

        Ok((
            ThreadMetadata {
                thread_id,
                session_id: id.map(acp::SessionId::new),
                agent_id,
                title: if title.is_empty() || title == DEFAULT_THREAD_TITLE {
                    None
                } else {
                    Some(title.into())
                },
                title_override: title_override
                    .filter(|t| !t.is_empty())
                    .map(SharedString::from),
                updated_at,
                created_at,
                interacted_at,
                worktree_paths,
                remote_connection,
                archived,
            },
            next,
        ))
    }
}

impl Column for ArchivedGitWorktree {
    fn column(statement: &mut Statement, start_index: i32) -> anyhow::Result<(Self, i32)> {
        let (id, next): (i64, i32) = Column::column(statement, start_index)?;
        let (worktree_path_str, next): (String, i32) = Column::column(statement, next)?;
        let (main_repo_path_str, next): (String, i32) = Column::column(statement, next)?;
        let (branch_name, next): (Option<String>, i32) = Column::column(statement, next)?;
        let (staged_commit_hash, next): (String, i32) = Column::column(statement, next)?;
        let (unstaged_commit_hash, next): (String, i32) = Column::column(statement, next)?;
        let (original_commit_hash, next): (String, i32) = Column::column(statement, next)?;

        Ok((
            ArchivedGitWorktree {
                id,
                worktree_path: PathBuf::from(worktree_path_str),
                main_repo_path: PathBuf::from(main_repo_path_str),
                branch_name,
                staged_commit_hash,
                unstaged_commit_hash,
                original_commit_hash,
            },
            next,
        ))
    }
}
