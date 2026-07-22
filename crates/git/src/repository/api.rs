use super::*;

pub const REMOTE_CANCELLED_BY_USER: &str = "Operation cancelled by user";

/// Format string used in graph log to get initial data for the git graph
/// %H - Full commit hash
/// %P - Parent hashes
/// %D - Ref names
/// %x00 - Null byte separator, used to split up commit data
pub(super) static GRAPH_COMMIT_FORMAT: &str = "--format=%H%x00%P%x00%D";

/// Used to get commits that match with a search
/// %H - Full commit hash
pub(super) static SEARCH_COMMIT_FORMAT: &str = "--format=%H";

/// Number of commits to load per chunk for the git graph.
pub const GRAPH_CHUNK_SIZE: usize = 1000;

/// Default value for the `git.worktree_directory` setting.
pub const DEFAULT_WORKTREE_DIRECTORY: &str = "../worktrees";

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Remote {
    pub name: SharedString,
}

pub enum ResetMode {
    /// Reset the branch pointer, leave index and worktree unchanged (this will make it look like things that were
    /// committed are now staged).
    Soft,
    /// Reset the branch pointer and index, leave worktree unchanged (this makes it look as though things that were
    /// committed are now unstaged).
    Mixed,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum FetchOptions {
    All,
    Remote(Remote),
}

impl FetchOptions {
    pub fn to_proto(&self) -> Option<String> {
        match self {
            FetchOptions::All => None,
            FetchOptions::Remote(remote) => Some(remote.clone().name.into()),
        }
    }

    pub fn from_proto(remote_name: Option<String>) -> Self {
        match remote_name {
            Some(name) => FetchOptions::Remote(Remote { name: name.into() }),
            None => FetchOptions::All,
        }
    }

    pub fn name(&self) -> SharedString {
        match self {
            Self::All => "Fetch all remotes".into(),
            Self::Remote(remote) => remote.name.clone(),
        }
    }
}

impl std::fmt::Display for FetchOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchOptions::All => write!(f, "--all"),
            FetchOptions::Remote(remote) => write!(f, "{}", remote.name),
        }
    }
}

pub trait GitRepository: Send + Sync {
    /// Returns the contents of an entry in the repository's index, or None if there is no entry for the given path.
    ///
    /// Also returns `None` for symlinks.
    fn load_index_text(&self, path: RepoPath) -> BoxFuture<'_, Option<String>>;

    /// Returns the contents of an entry in the repository's HEAD, or None if HEAD does not exist or has no entry for the given path.
    ///
    /// Also returns `None` for symlinks.
    fn load_committed_text(&self, path: RepoPath) -> BoxFuture<'_, Option<String>>;
    fn load_blob_content(&self, oid: Oid) -> BoxFuture<'_, Result<String>>;

    fn set_index_text(
        &self,
        path: RepoPath,
        content: Option<String>,
        env: Arc<HashMap<String, String>>,
        is_executable: bool,
    ) -> BoxFuture<'_, anyhow::Result<()>>;

    /// Returns the URL of the remote with the given name.
    fn remote_url(&self, name: &str) -> BoxFuture<'_, Option<String>> {
        let name = name.to_string();
        let fut = self.remote_urls();
        async move { fut.await.remove(&name) }.boxed()
    }

    /// Returns the URL of all remotes.
    fn remote_urls(&self) -> BoxFuture<'_, HashMap<String, String>>;

    /// Resolve a list of refs to SHAs.
    fn revparse_batch(&self, revs: Vec<String>) -> BoxFuture<'_, Result<Vec<Option<String>>>>;

    fn head_sha(&self) -> BoxFuture<'_, Option<String>> {
        async move {
            self.revparse_batch(vec!["HEAD".into()])
                .await
                .unwrap_or_default()
                .into_iter()
                .next()
                .flatten()
        }
        .boxed()
    }

    fn merge_message(&self) -> BoxFuture<'_, Option<String>>;

    fn status(&self, path_prefixes: &[RepoPath]) -> Task<Result<GitStatus>>;
    fn diff_tree(&self, request: DiffTreeType) -> BoxFuture<'_, Result<TreeDiff>>;

    fn stash_entries(&self) -> BoxFuture<'static, Result<GitStash>>;

    fn check_access(&self) -> BoxFuture<'_, Result<()>> {
        async move { Ok(()) }.boxed()
    }

    fn branches(&self) -> BoxFuture<'_, Result<BranchesScanResult>>;

    fn change_branch(&self, name: String) -> BoxFuture<'_, Result<()>>;
    fn create_branch(&self, name: String, base_branch: Option<String>)
    -> BoxFuture<'_, Result<()>>;
    fn rename_branch(&self, branch: String, new_name: String) -> BoxFuture<'_, Result<()>>;

    fn delete_branch(
        &self,
        is_remote: bool,
        name: String,
        force: bool,
    ) -> BoxFuture<'_, Result<()>>;

    fn worktrees(&self) -> BoxFuture<'_, Result<Vec<Worktree>>>;

    /// Returns the creation time of a linked worktree's git metadata
    /// directory (`.git/worktrees/<name>/`), resolved via the worktree's
    /// `.git` file.
    ///
    /// The metadata directory is created by `git worktree add` and removed
    /// by `git worktree remove`, so its creation time identifies a
    /// particular incarnation of the worktree: if the worktree is removed
    /// and recreated at the same path, the creation time changes.
    ///
    /// Returns `Ok(None)` when the worktree directory does not exist at
    /// all, and an error when the directory exists but the time cannot be
    /// determined (e.g. on filesystems without birthtime support); callers
    /// should fail safe in the error case.
    fn worktree_created_at(
        &self,
        worktree_path: PathBuf,
    ) -> BoxFuture<'_, Result<Option<SystemTime>>>;

    fn create_worktree(
        &self,
        target: CreateWorktreeTarget,
        path: PathBuf,
    ) -> BoxFuture<'_, Result<()>>;

    fn checkout_branch_in_worktree(
        &self,
        branch_name: String,
        worktree_path: PathBuf,
        create: bool,
    ) -> BoxFuture<'_, Result<()>>;

    fn remove_worktree(&self, path: PathBuf, force: bool) -> BoxFuture<'_, Result<()>>;

    fn rename_worktree(&self, old_path: PathBuf, new_path: PathBuf) -> BoxFuture<'_, Result<()>>;

    fn reset(
        &self,
        commit: String,
        mode: ResetMode,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn checkout_files(
        &self,
        commit: String,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn show(&self, commit: String) -> BoxFuture<'_, Result<CommitDetails>>;

    fn load_commit(&self, commit: String, cx: AsyncApp) -> BoxFuture<'_, Result<CommitDiff>>;
    fn blame(
        &self,
        path: RepoPath,
        content: Rope,
        line_ending: LineEnding,
    ) -> BoxFuture<'_, Result<crate::blame::Blame>>;

    /// Returns the absolute path to the repository. For worktrees, this will be the path to the
    /// worktree's gitdir within the main repository (typically `.git/worktrees/<name>`).
    fn path(&self) -> PathBuf;

    fn main_repository_path(&self) -> PathBuf;

    /// Updates the index to match the worktree at the given paths.
    ///
    /// If any of the paths have been deleted from the worktree, they will be removed from the index if found there.
    fn stage_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;
    /// Updates the index to match HEAD at the given paths.
    ///
    /// If any of the paths were previously staged but do not exist in HEAD, they will be removed from the index.
    fn unstage_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn run_hook(
        &self,
        hook: RunHook,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn commit(
        &self,
        message: SharedString,
        name_and_email: Option<(SharedString, SharedString)>,
        options: CommitOptions,
        askpass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn stash_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn stash_pop(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn stash_apply(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn stash_drop(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn push(
        &self,
        branch_name: String,
        remote_branch_name: String,
        upstream_name: String,
        options: Option<PushOptions>,
        askpass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        // This method takes an AsyncApp to ensure it's invoked on the main thread,
        // otherwise git-credentials-manager won't work.
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>>;

    fn pull(
        &self,
        branch_name: Option<String>,
        upstream_name: String,
        rebase: bool,
        askpass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        // This method takes an AsyncApp to ensure it's invoked on the main thread,
        // otherwise git-credentials-manager won't work.
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>>;

    fn fetch(
        &self,
        fetch_options: FetchOptions,
        askpass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        // This method takes an AsyncApp to ensure it's invoked on the main thread,
        // otherwise git-credentials-manager won't work.
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>>;

    fn get_push_remote(&self, branch: String) -> BoxFuture<'_, Result<Option<Remote>>>;

    fn get_branch_remote(&self, branch: String) -> BoxFuture<'_, Result<Option<Remote>>>;

    fn get_all_remotes(&self) -> BoxFuture<'_, Result<Vec<Remote>>>;

    fn remove_remote(&self, name: String) -> BoxFuture<'_, Result<()>>;

    fn create_remote(&self, name: String, url: String) -> BoxFuture<'_, Result<()>>;

    /// returns a list of remote branches that contain HEAD
    fn check_for_pushed_commit(&self) -> BoxFuture<'_, Result<Vec<SharedString>>>;

    /// Run git diff
    fn diff(&self, diff: DiffType) -> BoxFuture<'_, Result<String>>;

    fn diff_stat(
        &self,
        path_prefixes: &[RepoPath],
    ) -> BoxFuture<'static, Result<crate::status::GitDiffStat>>;

    /// Creates a checkpoint for the repository.
    fn checkpoint(&self) -> BoxFuture<'static, Result<GitRepositoryCheckpoint>>;

    /// Resets to a previously-created checkpoint.
    fn restore_checkpoint(&self, checkpoint: GitRepositoryCheckpoint) -> BoxFuture<'_, Result<()>>;

    /// Creates two detached commits capturing the current staged and unstaged
    /// state without moving any branch. Returns (staged_sha, unstaged_sha).
    fn create_archive_checkpoint(&self) -> BoxFuture<'_, Result<(String, String)>>;

    /// Restores the working directory and index from archive checkpoint SHAs.
    /// Assumes HEAD is already at the correct commit (original_commit_hash).
    /// Restores the index to match staged_sha's tree, and the working
    /// directory to match unstaged_sha's tree.
    fn restore_archive_checkpoint(
        &self,
        staged_sha: String,
        unstaged_sha: String,
    ) -> BoxFuture<'_, Result<()>>;

    /// Compares two checkpoints, returning true if they are equal
    fn compare_checkpoints(
        &self,
        left: GitRepositoryCheckpoint,
        right: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<bool>>;

    /// Computes a diff between two checkpoints.
    fn diff_checkpoints(
        &self,
        base_checkpoint: GitRepositoryCheckpoint,
        target_checkpoint: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<String>>;

    fn load_commit_template(&self) -> BoxFuture<'_, Result<Option<GitCommitTemplate>>>;

    fn default_branch(
        &self,
        include_remote_name: bool,
    ) -> BoxFuture<'_, Result<Option<SharedString>>>;

    /// Runs `git rev-list --parents` to get the commit graph structure.
    /// Returns commit SHAs and their parent SHAs for building the graph visualization.
    fn initial_graph_data(
        &self,
        log_source: LogSource,
        log_order: LogOrder,
        request_tx: Sender<Vec<Arc<InitialGraphCommitData>>>,
    ) -> BoxFuture<'_, Result<()>>;

    fn search_commits(
        &self,
        log_source: LogSource,
        search_args: SearchCommitArgs,
        request_tx: Sender<Oid>,
    ) -> BoxFuture<'_, Result<()>>;

    fn file_history_changed_files(
        &self,
        paths: Vec<RepoPath>,
        commit_limit: usize,
    ) -> BoxFuture<'_, Result<Vec<FileHistoryChangedFileSets>>>;

    fn commit_data_reader(&self) -> Result<CommitDataReader>;

    fn update_ref(&self, ref_name: String, commit: String) -> BoxFuture<'_, Result<()>>;

    fn delete_ref(&self, ref_name: String) -> BoxFuture<'_, Result<()>>;

    fn repair_worktrees(&self) -> BoxFuture<'_, Result<()>>;

    fn set_trusted(&self, trusted: bool);
    fn is_trusted(&self) -> bool;
}

pub enum DiffType {
    HeadToIndex,
    HeadToWorktree,
    MergeBase { base_ref: SharedString },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
pub enum PushOptions {
    SetUpstream,
    Force,
}

impl std::fmt::Debug for dyn GitRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("dyn GitRepository<...>").finish()
    }
}

pub struct RealGitRepository {
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    /// `None` only for bare repositories, which do not have a working directory.
    pub working_directory: Option<PathBuf>,
    pub system_git_binary_path: Option<PathBuf>,
    pub any_git_binary_path: PathBuf,
    pub(super) any_git_binary_help_output: Arc<Mutex<Option<SharedString>>>,
    pub(super) executor: BackgroundExecutor,
    pub(super) is_trusted: Arc<AtomicBool>,
}

#[derive(Debug)]
pub enum RefEdit {
    Update { ref_name: String, commit: String },
    Delete { ref_name: String },
}

impl RefEdit {
    pub(super) fn into_args(self) -> Vec<OsString> {
        match self {
            Self::Update { ref_name, commit } => {
                vec!["update-ref".into(), ref_name.into(), commit.into()]
            }
            Self::Delete { ref_name } => {
                vec!["update-ref".into(), "-d".into(), ref_name.into()]
            }
        }
    }
}
