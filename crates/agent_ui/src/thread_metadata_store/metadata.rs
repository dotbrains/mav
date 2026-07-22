use super::*;

pub struct ThreadMetadata {
    pub thread_id: ThreadId,
    pub session_id: Option<acp::SessionId>,
    pub agent_id: AgentId,
    pub title: Option<SharedString>,
    /// User-supplied title that takes precedence over `title`. Set when the
    /// user renames a thread, so that subsequent agent-driven title updates
    /// (e.g. from `SessionInfoUpdate`) don't clobber the user's choice.
    pub title_override: Option<SharedString>,
    pub updated_at: DateTime<Utc>,
    pub created_at: Option<DateTime<Utc>>,
    /// When a user last interacted to send a message (including queueing).
    /// Doesn't include the time when a queued message is fired.
    pub interacted_at: Option<DateTime<Utc>>,
    pub worktree_paths: WorktreePaths,
    pub remote_connection: Option<RemoteConnectionOptions>,
    pub archived: bool,
}

impl ThreadMetadata {
    /// Draft metadata stays sessionless until its first message is sent.
    pub fn is_draft(&self) -> bool {
        self.session_id.is_none()
    }

    pub fn display_title(&self) -> SharedString {
        self.title()
            .unwrap_or_else(|| crate::DEFAULT_THREAD_TITLE.into())
    }

    pub fn title(&self) -> Option<SharedString> {
        self.title_override.clone().or_else(|| self.title.clone())
    }

    pub fn folder_paths(&self) -> &PathList {
        self.worktree_paths.folder_path_list()
    }
    pub fn main_worktree_paths(&self) -> &PathList {
        self.worktree_paths.main_worktree_path_list()
    }

    pub fn references_folder_path(&self, path: &Path) -> bool {
        self.folder_paths()
            .paths()
            .iter()
            .any(|folder_path| folder_path.as_path() == path)
    }

    pub fn matches_remote_connection(
        &self,
        remote_connection: Option<&RemoteConnectionOptions>,
    ) -> bool {
        same_remote_connection_identity(self.remote_connection.as_ref(), remote_connection)
    }
}

/// Derives worktree display info from a thread's stored path list.
///
/// For each path in the thread's `folder_paths`, produces a
/// [`ThreadItemWorktreeInfo`] with a short display name, full path, and whether
/// the worktree is the main checkout or a linked git worktree. When
/// multiple main paths exist and a linked worktree's short name alone
/// wouldn't identify which main project it belongs to, the main project
/// name is prefixed for disambiguation (e.g. `project:feature`).
pub fn worktree_info_from_thread_paths<S: std::hash::BuildHasher>(
    worktree_paths: &WorktreePaths,
    branch_names: &std::collections::HashMap<PathBuf, SharedString, S>,
) -> Vec<ThreadItemWorktreeInfo> {
    let mut infos: Vec<ThreadItemWorktreeInfo> = Vec::new();
    let mut linked_short_names: Vec<(SharedString, SharedString)> = Vec::new();
    let mut unique_main_count = HashSet::default();

    for (main_path, folder_path) in worktree_paths.ordered_pairs() {
        unique_main_count.insert(main_path.clone());
        let is_linked = main_path != folder_path;

        if is_linked {
            let short_name = linked_worktree_short_name(main_path, folder_path).unwrap_or_default();
            let project_name = main_path
                .file_name()
                .map(|n| SharedString::from(n.to_string_lossy().to_string()))
                .unwrap_or_default();
            linked_short_names.push((short_name.clone(), project_name));
            infos.push(ThreadItemWorktreeInfo {
                worktree_name: Some(short_name),
                full_path: SharedString::from(folder_path.display().to_string()),
                highlight_positions: Vec::new(),
                kind: WorktreeKind::Linked,
                branch_name: branch_names.get(folder_path).cloned(),
            });
        } else {
            let Some(name) = folder_path.file_name() else {
                continue;
            };
            infos.push(ThreadItemWorktreeInfo {
                worktree_name: Some(SharedString::from(name.to_string_lossy().to_string())),
                full_path: SharedString::from(folder_path.display().to_string()),
                highlight_positions: Vec::new(),
                kind: WorktreeKind::Main,
                branch_name: branch_names.get(folder_path).cloned(),
            });
        }
    }

    // When the group has multiple main worktree paths and the thread's
    // folder paths don't all share the same short name, prefix each
    // linked worktree chip with its main project name so the user knows
    // which project it belongs to.
    let all_same_name = infos.len() > 1
        && infos
            .iter()
            .all(|i| i.worktree_name == infos[0].worktree_name);

    if unique_main_count.len() > 1 && !all_same_name {
        for (info, (_short_name, project_name)) in infos
            .iter_mut()
            .filter(|i| i.kind == WorktreeKind::Linked)
            .zip(linked_short_names.iter())
        {
            if let Some(name) = &info.worktree_name {
                info.worktree_name = Some(SharedString::from(format!("{}:{}", project_name, name)));
            }
        }
    }

    infos
}

impl From<&ThreadMetadata> for acp_thread::AgentSessionInfo {
    fn from(meta: &ThreadMetadata) -> Self {
        let session_id = meta
            .session_id
            .clone()
            .unwrap_or_else(|| acp::SessionId::new(meta.thread_id.0.to_string()));
        Self {
            session_id,
            work_dirs: Some(meta.folder_paths().clone()),
            title: meta.title(),
            updated_at: Some(meta.updated_at),
            created_at: meta.created_at,
            meta: None,
        }
    }
}

/// Record of a git worktree that was archived (deleted from disk) when its
/// last thread was archived.
pub struct ArchivedGitWorktree {
    /// Auto-incrementing primary key.
    pub id: i64,
    /// Absolute path to the directory of the worktree before it was deleted.
    /// Used when restoring, to put the recreated worktree back where it was.
    /// If the path already exists on disk, the worktree is assumed to be
    /// already restored and is used as-is.
    pub worktree_path: PathBuf,
    /// Absolute path of the main repository ("main worktree") that owned this worktree.
    /// Used when restoring, to reattach the recreated worktree to the correct main repo.
    /// If the main repo isn't found on disk, unarchiving fails because we only store
    /// commit hashes, and without the actual git repo being available, we can't restore
    /// the files.
    pub main_repo_path: PathBuf,
    /// Branch that was checked out in the worktree at archive time. `None` if
    /// the worktree was in detached HEAD state, which isn't supported in Mav, but
    /// could happen if the user made a detached one outside of Mav.
    /// On restore, we try to switch to this branch. If that fails (e.g. it's
    /// checked out elsewhere), we auto-generate a new one.
    pub branch_name: Option<String>,
    /// SHA of the WIP commit that captures files that were staged (but not yet
    /// committed) at the time of archiving. This commit can be empty if the
    /// user had no staged files at the time. It sits directly on top of whatever
    /// the user's last actual commit was.
    pub staged_commit_hash: String,
    /// SHA of the WIP commit that captures files that were unstaged (including
    /// untracked) at the time of archiving. This commit can be empty if the user
    /// had no unstaged files at the time. It sits on top of `staged_commit_hash`.
    /// After doing `git reset` past both of these commits, we're back in the state
    /// we had before archiving, including what was staged, what was unstaged, and
    /// what was committed.
    pub unstaged_commit_hash: String,
    /// SHA of the commit that HEAD pointed at before we created the two WIP
    /// commits during archival. After resetting past the WIP commits during
    /// restore, HEAD should land back on this commit. It also serves as a
    /// pre-restore sanity check (abort if this commit no longer exists in the
    /// repo) and as a fallback target if the WIP resets fail.
    pub original_commit_hash: String,
}
