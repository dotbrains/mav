use super::*;

impl GitRepository for RealGitRepository {
    fn path(&self) -> PathBuf {
        self.repository_path()
    }

    fn main_repository_path(&self) -> PathBuf {
        self.repository_main_repository_path()
    }

    fn show(&self, commit: String) -> BoxFuture<'_, Result<CommitDetails>> {
        self.repository_show(commit)
    }

    fn load_commit(&self, commit: String, cx: AsyncApp) -> BoxFuture<'_, Result<CommitDiff>> {
        self.repository_load_commit(commit, cx)
    }

    fn reset(
        &self,
        commit: String,
        mode: ResetMode,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_reset(commit, mode, env)
    }

    fn checkout_files(
        &self,
        commit: String,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_checkout_files(commit, paths, env)
    }

    fn load_index_text(&self, path: RepoPath) -> BoxFuture<'_, Option<String>> {
        self.repository_load_index_text(path)
    }

    fn load_committed_text(&self, path: RepoPath) -> BoxFuture<'_, Option<String>> {
        self.repository_load_committed_text(path)
    }

    fn load_blob_content(&self, oid: Oid) -> BoxFuture<'_, Result<String>> {
        self.repository_load_blob_content(oid)
    }

    fn load_commit_template(&self) -> BoxFuture<'_, Result<Option<GitCommitTemplate>>> {
        self.repository_load_commit_template()
    }

    fn set_index_text(
        &self,
        path: RepoPath,
        content: Option<String>,
        env: Arc<HashMap<String, String>>,
        is_executable: bool,
    ) -> BoxFuture<'_, anyhow::Result<()>> {
        self.repository_set_index_text(path, content, env, is_executable)
    }

    fn remote_urls(&self) -> BoxFuture<'_, HashMap<String, String>> {
        self.repository_remote_urls()
    }

    fn revparse_batch(&self, revs: Vec<String>) -> BoxFuture<'_, Result<Vec<Option<String>>>> {
        self.repository_revparse_batch(revs)
    }

    fn merge_message(&self) -> BoxFuture<'_, Option<String>> {
        self.repository_merge_message()
    }

    fn status(&self, path_prefixes: &[RepoPath]) -> Task<Result<GitStatus>> {
        self.repository_status(path_prefixes)
    }

    fn check_access(&self) -> BoxFuture<'_, Result<()>> {
        self.repository_check_access()
    }

    fn diff_tree(&self, request: DiffTreeType) -> BoxFuture<'_, Result<TreeDiff>> {
        self.repository_diff_tree(request)
    }

    fn stash_entries(&self) -> BoxFuture<'static, Result<GitStash>> {
        self.repository_stash_entries()
    }

    fn branches(&self) -> BoxFuture<'_, Result<BranchesScanResult>> {
        self.repository_branches()
    }

    fn worktrees(&self) -> BoxFuture<'_, Result<Vec<Worktree>>> {
        self.repository_worktrees()
    }

    fn worktree_created_at(
        &self,
        worktree_path: PathBuf,
    ) -> BoxFuture<'_, Result<Option<SystemTime>>> {
        self.repository_worktree_created_at(worktree_path)
    }

    fn create_worktree(
        &self,
        target: CreateWorktreeTarget,
        path: PathBuf,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_create_worktree(target, path)
    }

    fn remove_worktree(&self, path: PathBuf, force: bool) -> BoxFuture<'_, Result<()>> {
        self.repository_remove_worktree(path, force)
    }

    fn rename_worktree(&self, old_path: PathBuf, new_path: PathBuf) -> BoxFuture<'_, Result<()>> {
        self.repository_rename_worktree(old_path, new_path)
    }

    fn checkout_branch_in_worktree(
        &self,
        branch_name: String,
        worktree_path: PathBuf,
        create: bool,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_checkout_branch_in_worktree(branch_name, worktree_path, create)
    }

    fn change_branch(&self, name: String) -> BoxFuture<'_, Result<()>> {
        self.repository_change_branch(name)
    }

    fn create_branch(
        &self,
        name: String,
        base_branch: Option<String>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_create_branch(name, base_branch)
    }

    fn rename_branch(&self, branch: String, new_name: String) -> BoxFuture<'_, Result<()>> {
        self.repository_rename_branch(branch, new_name)
    }

    fn delete_branch(
        &self,
        is_remote: bool,
        name: String,
        force: bool,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_delete_branch(is_remote, name, force)
    }

    fn blame(
        &self,
        path: RepoPath,
        content: Rope,
        line_ending: LineEnding,
    ) -> BoxFuture<'_, Result<crate::blame::Blame>> {
        self.repository_blame(path, content, line_ending)
    }

    fn diff(&self, diff: DiffType) -> BoxFuture<'_, Result<String>> {
        self.repository_diff(diff)
    }

    fn diff_stat(
        &self,
        path_prefixes: &[RepoPath],
    ) -> BoxFuture<'static, Result<crate::status::GitDiffStat>> {
        self.repository_diff_stat(path_prefixes)
    }

    fn stage_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_stage_paths(paths, env)
    }

    fn unstage_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_unstage_paths(paths, env)
    }

    fn stash_paths(
        &self,
        paths: Vec<RepoPath>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_stash_paths(paths, env)
    }

    fn stash_pop(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_stash_pop(index, env)
    }

    fn stash_apply(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_stash_apply(index, env)
    }

    fn stash_drop(
        &self,
        index: Option<usize>,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_stash_drop(index, env)
    }

    fn commit(
        &self,
        message: SharedString,
        name_and_email: Option<(SharedString, SharedString)>,
        options: CommitOptions,
        ask_pass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_commit(message, name_and_email, options, ask_pass, env)
    }

    fn update_ref(&self, ref_name: String, commit: String) -> BoxFuture<'_, Result<()>> {
        self.repository_update_ref(ref_name, commit)
    }

    fn delete_ref(&self, ref_name: String) -> BoxFuture<'_, Result<()>> {
        self.repository_delete_ref(ref_name)
    }

    fn repair_worktrees(&self) -> BoxFuture<'_, Result<()>> {
        self.repository_repair_worktrees()
    }

    fn push(
        &self,
        branch_name: String,
        remote_branch_name: String,
        remote_name: String,
        options: Option<PushOptions>,
        ask_pass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        self.repository_push(
            branch_name,
            remote_branch_name,
            remote_name,
            options,
            ask_pass,
            env,
            cx,
        )
    }

    fn pull(
        &self,
        branch_name: Option<String>,
        remote_name: String,
        rebase: bool,
        ask_pass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        self.repository_pull(branch_name, remote_name, rebase, ask_pass, env, cx)
    }

    fn fetch(
        &self,
        fetch_options: FetchOptions,
        ask_pass: AskPassDelegate,
        env: Arc<HashMap<String, String>>,
        cx: AsyncApp,
    ) -> BoxFuture<'_, Result<RemoteCommandOutput>> {
        self.repository_fetch(fetch_options, ask_pass, env, cx)
    }

    fn get_push_remote(&self, branch: String) -> BoxFuture<'_, Result<Option<Remote>>> {
        self.repository_get_push_remote(branch)
    }

    fn get_branch_remote(&self, branch: String) -> BoxFuture<'_, Result<Option<Remote>>> {
        self.repository_get_branch_remote(branch)
    }

    fn get_all_remotes(&self) -> BoxFuture<'_, Result<Vec<Remote>>> {
        self.repository_get_all_remotes()
    }

    fn remove_remote(&self, name: String) -> BoxFuture<'_, Result<()>> {
        self.repository_remove_remote(name)
    }

    fn create_remote(&self, name: String, url: String) -> BoxFuture<'_, Result<()>> {
        self.repository_create_remote(name, url)
    }

    fn check_for_pushed_commit(&self) -> BoxFuture<'_, Result<Vec<SharedString>>> {
        self.repository_check_for_pushed_commit()
    }

    fn checkpoint(&self) -> BoxFuture<'static, Result<GitRepositoryCheckpoint>> {
        self.repository_checkpoint()
    }

    fn restore_checkpoint(&self, checkpoint: GitRepositoryCheckpoint) -> BoxFuture<'_, Result<()>> {
        self.repository_restore_checkpoint(checkpoint)
    }

    fn create_archive_checkpoint(&self) -> BoxFuture<'_, Result<(String, String)>> {
        self.repository_create_archive_checkpoint()
    }

    fn restore_archive_checkpoint(
        &self,
        staged_sha: String,
        unstaged_sha: String,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_restore_archive_checkpoint(staged_sha, unstaged_sha)
    }

    fn compare_checkpoints(
        &self,
        left: GitRepositoryCheckpoint,
        right: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<bool>> {
        self.repository_compare_checkpoints(left, right)
    }

    fn diff_checkpoints(
        &self,
        base_checkpoint: GitRepositoryCheckpoint,
        target_checkpoint: GitRepositoryCheckpoint,
    ) -> BoxFuture<'_, Result<String>> {
        self.repository_diff_checkpoints(base_checkpoint, target_checkpoint)
    }

    fn default_branch(
        &self,
        include_remote_name: bool,
    ) -> BoxFuture<'_, Result<Option<SharedString>>> {
        self.repository_default_branch(include_remote_name)
    }

    fn run_hook(
        &self,
        hook: RunHook,
        env: Arc<HashMap<String, String>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_run_hook(hook, env)
    }

    fn initial_graph_data(
        &self,
        log_source: LogSource,
        log_order: LogOrder,
        request_tx: Sender<Vec<Arc<InitialGraphCommitData>>>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_initial_graph_data(log_source, log_order, request_tx)
    }

    fn search_commits(
        &self,
        log_source: LogSource,
        search_args: SearchCommitArgs,
        request_tx: Sender<Oid>,
    ) -> BoxFuture<'_, Result<()>> {
        self.repository_search_commits(log_source, search_args, request_tx)
    }

    fn file_history_changed_files(
        &self,
        paths: Vec<RepoPath>,
        commit_limit: usize,
    ) -> BoxFuture<'_, Result<Vec<FileHistoryChangedFileSets>>> {
        self.repository_file_history_changed_files(paths, commit_limit)
    }

    fn commit_data_reader(&self) -> Result<CommitDataReader> {
        self.repository_commit_data_reader()
    }

    fn set_trusted(&self, trusted: bool) {
        self.repository_set_trusted(trusted)
    }

    fn is_trusted(&self) -> bool {
        self.repository_is_trusted()
    }
}
