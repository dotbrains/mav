use super::*;

impl RepositorySnapshot {
    pub(super) fn empty(
        id: RepositoryId,
        work_directory_abs_path: Arc<Path>,
        repository_dir_abs_path: Option<Arc<Path>>,
        dot_git_abs_path: Option<Arc<Path>>,
        common_dir_abs_path: Option<Arc<Path>>,
        path_style: PathStyle,
    ) -> Self {
        let repository_dir_abs_path =
            repository_dir_abs_path.unwrap_or_else(|| work_directory_abs_path.join(".git").into());
        let dot_git_abs_path =
            dot_git_abs_path.unwrap_or_else(|| work_directory_abs_path.join(".git").into());
        let common_dir_abs_path =
            common_dir_abs_path.unwrap_or_else(|| repository_dir_abs_path.clone());

        Self {
            id,
            statuses_by_path: Default::default(),
            repository_dir_abs_path,
            dot_git_abs_path,
            common_dir_abs_path,
            work_directory_abs_path,
            branch: None,
            branch_list: Arc::from([]),
            branch_list_error: None,
            head_commit: None,
            scan_id: 0,
            merge: Default::default(),
            remote_origin_url: None,
            remote_upstream_url: None,
            stash_entries: Default::default(),
            linked_worktrees: Arc::from([]),
            path_style,
        }
    }

    pub(super) fn initial_update(&self, project_id: u64) -> proto::UpdateRepository {
        proto::UpdateRepository {
            branch_summary: self.branch.as_ref().map(branch_to_proto),
            branch_list: self.branch_list.iter().map(branch_to_proto).collect(),
            branch_list_error: self
                .branch_list_error
                .as_ref()
                .map(|error| error.to_string()),
            head_commit_details: self.head_commit.as_ref().map(commit_details_to_proto),
            updated_statuses: self
                .statuses_by_path
                .iter()
                .map(|entry| entry.to_proto())
                .collect(),
            removed_statuses: Default::default(),
            current_merge_conflicts: self
                .merge
                .merge_heads_by_conflicted_path
                .iter()
                .map(|(repo_path, _)| repo_path.to_proto())
                .collect(),
            merge_message: self.merge.message.as_ref().map(|msg| msg.to_string()),
            project_id,
            id: self.id.to_proto(),
            abs_path: self.work_directory_abs_path.to_string_lossy().into_owned(),
            entry_ids: vec![self.id.to_proto()],
            scan_id: self.scan_id,
            is_last_update: true,
            stash_entries: self
                .stash_entries
                .entries
                .iter()
                .map(stash_to_proto)
                .collect(),
            remote_upstream_url: self.remote_upstream_url.clone(),
            remote_origin_url: self.remote_origin_url.clone(),
            repository_dir_abs_path: Some(
                self.repository_dir_abs_path.to_string_lossy().into_owned(),
            ),
            common_dir_abs_path: Some(self.common_dir_abs_path.to_string_lossy().into_owned()),
            linked_worktrees: self
                .linked_worktrees
                .iter()
                .map(worktree_to_proto)
                .collect(),
        }
    }

    pub(super) fn build_update(&self, old: &Self, project_id: u64) -> proto::UpdateRepository {
        let mut updated_statuses: Vec<proto::StatusEntry> = Vec::new();
        let mut removed_statuses: Vec<String> = Vec::new();

        let mut new_statuses = self.statuses_by_path.iter().peekable();
        let mut old_statuses = old.statuses_by_path.iter().peekable();

        let mut current_new_entry = new_statuses.next();
        let mut current_old_entry = old_statuses.next();
        loop {
            match (current_new_entry, current_old_entry) {
                (Some(new_entry), Some(old_entry)) => {
                    match new_entry.repo_path.cmp(&old_entry.repo_path) {
                        Ordering::Less => {
                            updated_statuses.push(new_entry.to_proto());
                            current_new_entry = new_statuses.next();
                        }
                        Ordering::Equal => {
                            if new_entry.status != old_entry.status
                                || new_entry.diff_stat != old_entry.diff_stat
                            {
                                updated_statuses.push(new_entry.to_proto());
                            }
                            current_old_entry = old_statuses.next();
                            current_new_entry = new_statuses.next();
                        }
                        Ordering::Greater => {
                            removed_statuses.push(old_entry.repo_path.to_proto());
                            current_old_entry = old_statuses.next();
                        }
                    }
                }
                (None, Some(old_entry)) => {
                    removed_statuses.push(old_entry.repo_path.to_proto());
                    current_old_entry = old_statuses.next();
                }
                (Some(new_entry), None) => {
                    updated_statuses.push(new_entry.to_proto());
                    current_new_entry = new_statuses.next();
                }
                (None, None) => break,
            }
        }

        proto::UpdateRepository {
            branch_summary: self.branch.as_ref().map(branch_to_proto),
            branch_list: self.branch_list.iter().map(branch_to_proto).collect(),
            branch_list_error: self
                .branch_list_error
                .as_ref()
                .map(|error| error.to_string()),
            head_commit_details: self.head_commit.as_ref().map(commit_details_to_proto),
            updated_statuses,
            removed_statuses,
            current_merge_conflicts: self
                .merge
                .merge_heads_by_conflicted_path
                .iter()
                .map(|(path, _)| path.to_proto())
                .collect(),
            merge_message: self.merge.message.as_ref().map(|msg| msg.to_string()),
            project_id,
            id: self.id.to_proto(),
            abs_path: self.work_directory_abs_path.to_string_lossy().into_owned(),
            entry_ids: vec![],
            scan_id: self.scan_id,
            is_last_update: true,
            stash_entries: self
                .stash_entries
                .entries
                .iter()
                .map(stash_to_proto)
                .collect(),
            remote_upstream_url: self.remote_upstream_url.clone(),
            remote_origin_url: self.remote_origin_url.clone(),
            repository_dir_abs_path: Some(
                self.repository_dir_abs_path.to_string_lossy().into_owned(),
            ),
            common_dir_abs_path: Some(self.common_dir_abs_path.to_string_lossy().into_owned()),
            linked_worktrees: self
                .linked_worktrees
                .iter()
                .map(worktree_to_proto)
                .collect(),
        }
    }

    /// Returns the main worktree path for this repository, if one exists.
    ///
    /// Linked worktrees attached to bare repositories do not have a main
    /// worktree. For linked worktrees attached to a non-bare repository, the
    /// common Git directory is the main worktree's `.git` directory.
    pub fn main_worktree_abs_path(&self) -> Option<&Path> {
        if self.is_linked_worktree() {
            if self.common_dir_abs_path.file_name()? == std::ffi::OsStr::new(".git") {
                self.common_dir_abs_path.parent()
            } else {
                None
            }
        } else {
            Some(self.work_directory_abs_path.as_ref())
        }
    }

    /// The main worktree is the original checkout that other worktrees were
    /// created from.
    ///
    /// For example, if you had both `~/code/mav` and `~/code/worktrees/mav-2`,
    /// then `~/code/mav` is the main worktree and `~/code/worktrees/mav-2` is a linked worktree.
    ///
    /// Submodules also return `true` here, since they are not linked worktrees.
    pub fn is_main_worktree(&self) -> bool {
        !self.is_linked_worktree()
    }

    /// Returns true if this repository is a linked worktree, that is, one that
    /// was created from another worktree.
    ///
    /// Returns `false` for both the main worktree and submodules.
    pub fn is_linked_worktree(&self) -> bool {
        self.repository_dir_abs_path != self.common_dir_abs_path
    }

    pub fn linked_worktrees(&self) -> &[GitWorktree] {
        &self.linked_worktrees
    }

    pub fn status(&self) -> impl Iterator<Item = StatusEntry> + '_ {
        self.statuses_by_path.iter().cloned()
    }

    pub fn status_summary(&self) -> GitSummary {
        self.statuses_by_path.summary().item_summary
    }

    pub fn status_for_path(&self, path: &RepoPath) -> Option<StatusEntry> {
        self.statuses_by_path
            .get(&PathKey(path.as_ref().clone()), ())
            .cloned()
    }

    pub fn diff_stat_for_path(&self, path: &RepoPath) -> Option<DiffStat> {
        self.statuses_by_path
            .get(&PathKey(path.as_ref().clone()), ())
            .and_then(|entry| entry.diff_stat)
    }

    pub fn abs_path_to_repo_path(&self, abs_path: &Path) -> Option<RepoPath> {
        Self::abs_path_to_repo_path_inner(&self.work_directory_abs_path, abs_path, self.path_style)
    }

    pub(super) fn repo_path_to_abs_path(&self, repo_path: &RepoPath) -> PathBuf {
        let repo_path = repo_path.display(self.path_style);
        PathBuf::from(
            self.path_style
                .join(&self.work_directory_abs_path, repo_path.as_ref())
                .unwrap(),
        )
    }

    #[inline]
    pub(super) fn abs_path_to_repo_path_inner(
        work_directory_abs_path: &Path,
        abs_path: &Path,
        path_style: PathStyle,
    ) -> Option<RepoPath> {
        let rel_path = path_style.strip_prefix(abs_path, work_directory_abs_path)?;
        Some(RepoPath::from_rel_path(&rel_path))
    }

    pub fn had_conflict_on_last_merge_head_change(&self, repo_path: &RepoPath) -> bool {
        self.merge
            .merge_heads_by_conflicted_path
            .contains_key(repo_path)
    }

    pub fn has_conflict(&self, repo_path: &RepoPath) -> bool {
        let had_conflict_on_last_merge_head_change = self
            .merge
            .merge_heads_by_conflicted_path
            .contains_key(repo_path);
        let has_conflict_currently = self
            .status_for_path(repo_path)
            .is_some_and(|entry| entry.status.is_conflicted());
        had_conflict_on_last_merge_head_change || has_conflict_currently
    }

    /// This is the name that will be displayed in the repository selector for this repository.
    pub fn display_name(&self) -> SharedString {
        self.work_directory_abs_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            .into()
    }
}
