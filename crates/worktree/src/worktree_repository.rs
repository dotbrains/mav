use super::*;

#[derive(Debug, Clone)]
pub(crate) struct LocalRepositoryEntry {
    pub(crate) work_directory_id: ProjectEntryId,
    pub(crate) work_directory: WorkDirectory,
    pub(crate) work_directory_abs_path: Arc<Path>,
    pub(crate) git_dir_scan_id: usize,
    /// Absolute path to the original .git entry that caused us to create this repository.
    ///
    /// This is normally a directory, but may be a "gitfile" that points to a directory elsewhere
    /// (whose path we then store in `repository_dir_abs_path`).
    pub(crate) dot_git_abs_path: Arc<Path>,
    /// Absolute path to the "commondir" for this repository.
    ///
    /// This is always a directory. For a normal repository, this is the same as
    /// `dot_git_abs_path`. For a linked worktree, this is the main repo's `.git`
    /// directory (resolved from the worktree's `commondir` file). For a submodule,
    /// this equals `repository_dir_abs_path` (submodules don't have a `commondir`
    /// file).
    pub(crate) common_dir_abs_path: Arc<Path>,
    /// Absolute path to the directory holding the repository's state.
    ///
    /// For a normal repository, this is a directory and coincides with `dot_git_abs_path` and
    /// `common_dir_abs_path`. For a submodule or worktree, this is some subdirectory of the
    /// commondir like `/project/.git/modules/foo`.
    pub(crate) repository_dir_abs_path: Arc<Path>,
}

impl sum_tree::Item for LocalRepositoryEntry {
    type Summary = PathSummary<sum_tree::NoSummary>;

    fn summary(&self, _: <Self::Summary as Summary>::Context<'_>) -> Self::Summary {
        PathSummary {
            max_path: self.work_directory.path_key().0,
            item_summary: sum_tree::NoSummary,
        }
    }
}

impl KeyedItem for LocalRepositoryEntry {
    type Key = PathKey;

    fn key(&self) -> Self::Key {
        self.work_directory.path_key()
    }
}
