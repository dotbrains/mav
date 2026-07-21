pub mod branch_diff;
mod branch_remote_handlers;
mod buffer_git_state;
mod buffer_git_state_recalculation;
mod checkpoint_handlers;
mod commit_data;
#[cfg(test)]
mod commit_data_tests;
mod commit_handlers;
mod conflict_set;
mod diff_opening;
mod diff_request_handlers;
mod git_access;
mod git_job;
pub use git_access::GitAccess;
use git_job::{GitJob, GitJobKey};
mod git_store_handlers;
mod git_store_lifecycle;
mod git_store_operations;
mod git_store_queries;
mod git_store_repository_events;
mod git_store_repository_paths;
mod git_store_worktree_events;
pub mod git_traversal;
mod graph_data;
pub mod job_debug_queue;
pub mod pending_op;
mod remote_delegate;
mod repository_access;
mod repository_branch_ops;
mod repository_checkpoints;
mod repository_core;
mod repository_diff_bases;
mod repository_helpers;
mod repository_jobs;
mod repository_lifecycle;
mod repository_misc;
mod repository_remote_commands;
mod repository_remote_updates;
mod repository_request_handlers;
mod repository_snapshot;
mod repository_snapshot_compute;
mod repository_staging;
mod repository_stash;
mod repository_status_tracking;
mod repository_text_loaders;
mod repository_workers;
mod repository_worktrees;
mod status_entry;
#[cfg(any(test, feature = "test-support"))]
mod test_support;
#[cfg(test)]
mod tests;
mod worktree_paths;

mod serialization;
mod staging_stash_handlers;
mod worktree_handlers;

use serialization::*;
use worktree_paths::{get_permalink_in_rust_registry_src, remove_empty_managed_worktree_ancestors};
pub use worktree_paths::{
    linked_worktree_short_name, repo_identity_path, resolve_git_worktree_to_main_repo,
    worktrees_directory_for_repo,
};

use crate::{
    ProjectEnvironment, ProjectItem, ProjectPath,
    buffer_store::{BufferStore, BufferStoreEvent},
    project_settings::ProjectSettings,
    trusted_worktrees::{
        PathTrust, TrustedWorktrees, TrustedWorktreesEvent, TrustedWorktreesStore,
    },
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};
use anyhow::{Context as _, Result, anyhow, bail};
use askpass::{AskPassDelegate, EncryptedPassword, IKnowWhatIAmDoingAndIHaveReadTheDocs};
use buffer_diff::{BufferDiff, BufferDiffEvent};
use client::ProjectId;
use collections::HashMap;
pub use conflict_set::{ConflictRegion, ConflictSet, ConflictSetSnapshot, ConflictSetUpdate};
use fs::{Fs, RemoveOptions};
use futures::{
    FutureExt, SinkExt, Stream, StreamExt,
    channel::{
        mpsc,
        oneshot::{self, Canceled},
    },
    future::{self, BoxFuture, Shared},
    stream::{FuturesOrdered, FuturesUnordered},
};
use git::{
    BuildPermalinkParams, GitHostingProviderRegistry, Oid, RunHook,
    blame::Blame,
    parse_git_remote_url,
    repository::{
        Branch, BranchesScanResult, CommitData, CommitDetails, CommitDiff, CommitFile,
        CommitOptions, CreateWorktreeTarget, DiffType, FetchOptions, FileHistoryChangedFileSets,
        GitCommitTemplate, GitRepository, GitRepositoryCheckpoint, InitialGraphCommitData,
        LogOrder, LogSource, PushOptions, Remote, RemoteCommandOutput, RepoPath, ResetMode,
        SearchCommitArgs, UpstreamTrackingStatus, Worktree as GitWorktree, delete_branch_flag,
    },
    stash::{GitStash, StashEntry},
    status::{
        self, DiffStat, DiffTreeType, FileStatus, GitSummary, StatusCode, TrackedStatus, TreeDiff,
        TreeDiffStatus, UnmergedStatus, UnmergedStatusCode,
    },
};
use gpui::{
    App, AppContext, AsyncApp, BackgroundExecutor, Context, Entity, EventEmitter, SharedString,
    Subscription, Task, TaskExt, WeakEntity,
};
use language::{
    Buffer, BufferEvent, Capability, Language, LanguageRegistry,
    proto::{deserialize_version, serialize_version},
};
use parking_lot::Mutex;
use paths::{config_dir, home_dir};
use pending_op::{PendingOp, PendingOpId, PendingOps, PendingOpsSummary};
use postage::stream::Stream as _;
use rpc::{
    AnyProtoClient, TypedEnvelope,
    proto::{self, git_reset, split_repository_update},
};
use serde::Deserialize;
use settings::{Settings, WorktreeId};
use smallvec::SmallVec;
use smol::future::yield_now;
use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashSet, VecDeque, hash_map::Entry},
    future::Future,
    mem,
    ops::Range,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        Arc,
        atomic::{self, AtomicU64},
    },
    time::{Duration, Instant, SystemTime},
};
use sum_tree::{Edit, SumTree, TreeMap};
use task::Shell;
use text::{Bias, BufferId};
use util::{
    ResultExt, debug_panic,
    paths::{PathStyle, SanitizedPath},
    post_inc,
    rel_path::RelPath,
};
use worktree::{
    File, PathChange, PathKey, PathProgress, PathSummary, PathTarget, ProjectEntryId,
    UpdatedGitRepositoriesSet, UpdatedGitRepository, Worktree,
};
use zeroize::Zeroize;

pub struct GitStore {
    state: GitStoreState,
    buffer_store: Entity<BufferStore>,
    worktree_store: Entity<WorktreeStore>,
    repositories: HashMap<RepositoryId, Entity<Repository>>,
    worktree_ids: HashMap<RepositoryId, HashSet<WorktreeId>>,
    active_repo_id: Option<RepositoryId>,
    #[allow(clippy::type_complexity)]
    loading_diffs:
        HashMap<(BufferId, DiffKind), Shared<Task<Result<Entity<BufferDiff>, Arc<anyhow::Error>>>>>,
    diffs: HashMap<BufferId, Entity<BufferGitState>>,
    shared_diffs: HashMap<proto::PeerId, HashMap<BufferId, SharedDiffs>>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Default)]
struct SharedDiffs {
    unstaged: Option<Entity<BufferDiff>>,
    uncommitted: Option<Entity<BufferDiff>>,
}

struct BufferGitState {
    unstaged_diff: Option<WeakEntity<BufferDiff>>,
    staged_diff: Option<(WeakEntity<BufferDiff>, Entity<Buffer>)>,
    uncommitted_diff: Option<WeakEntity<BufferDiff>>,
    oid_diffs: HashMap<Option<git::Oid>, WeakEntity<BufferDiff>>,
    conflict_set: Option<WeakEntity<ConflictSet>>,
    recalculate_diff_task: Option<Task<Result<()>>>,
    reparse_conflict_markers_task: Option<Task<Result<()>>>,
    language: Option<Arc<Language>>,
    language_registry: Option<Arc<LanguageRegistry>>,
    conflict_updated_futures: Vec<oneshot::Sender<()>>,
    recalculating_tx: postage::watch::Sender<bool>,

    /// These operation counts are used to ensure that head and index text
    /// values read from the git repository are up-to-date with any hunk staging
    /// operations that have been performed on the BufferDiff.
    ///
    /// The operation count is incremented immediately when the user initiates a
    /// hunk stage/unstage operation. Then, upon finishing writing the new index
    /// text do disk, the `operation count as of write` is updated to reflect
    /// the operation count that prompted the write.
    hunk_staging_operation_count: usize,
    hunk_staging_operation_count_as_of_write: usize,

    head_text: Option<Arc<str>>,
    index_text: Option<Arc<str>>,
    oid_texts: HashMap<git::Oid, Arc<str>>,
    head_text_buffer: WeakEntity<Buffer>,
    index_text_buffer: WeakEntity<Buffer>,
    index_text_buffer_language_enabled: bool,
    head_changed: bool,
    index_changed: bool,
    language_changed: bool,
}

#[derive(Clone, Debug)]
enum DiffBasesChange {
    SetIndex(Option<String>),
    SetHead(Option<String>),
    SetEach {
        index: Option<String>,
        head: Option<String>,
    },
    SetBoth(Option<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DiffKind {
    Unstaged,
    Staged,
    Uncommitted,
    SinceOid(Option<git::Oid>),
}

enum GitStoreState {
    Local {
        next_repository_id: Arc<AtomicU64>,
        downstream: Option<LocalDownstreamState>,
        project_environment: Entity<ProjectEnvironment>,
        fs: Arc<dyn Fs>,
        _fs_watches: Box<[Task<()>]>,
    },
    Remote {
        upstream_client: AnyProtoClient,
        upstream_project_id: u64,
        downstream: Option<(AnyProtoClient, ProjectId)>,
    },
}

enum DownstreamUpdate {
    UpdateRepository(RepositorySnapshot),
    RemoveRepository(RepositoryId),
}

struct LocalDownstreamState {
    client: AnyProtoClient,
    project_id: ProjectId,
    updates_tx: mpsc::UnboundedSender<DownstreamUpdate>,
    _task: Task<Result<()>>,
}

#[derive(Clone, Debug)]
pub struct GitStoreCheckpoint {
    checkpoints_by_work_dir_abs_path: HashMap<Arc<Path>, GitRepositoryCheckpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusEntry {
    pub repo_path: RepoPath,
    pub status: FileStatus,
    pub diff_stat: Option<DiffStat>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepositoryId(pub u64);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeDetails {
    pub merge_heads_by_conflicted_path: TreeMap<RepoPath, Vec<Option<SharedString>>>,
    pub message: Option<SharedString>,
}

#[derive(Clone)]
pub enum CommitDataState {
    Loading(Option<Shared<oneshot::Receiver<Arc<CommitData>>>>),
    Loaded(Arc<CommitData>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub id: RepositoryId,
    pub statuses_by_path: SumTree<StatusEntry>,
    pub work_directory_abs_path: Arc<Path>,
    pub dot_git_abs_path: Arc<Path>,
    /// Absolute path to the directory holding this worktree's Git state.
    ///
    /// For a linked worktree this is the worktree-specific directory under the
    /// common Git directory, such as `<main>/.git/worktrees/<name>`.
    pub repository_dir_abs_path: Arc<Path>,
    /// Absolute path to the repository's common Git directory.
    ///
    /// For a normal checkout this is `<work_directory>/.git`. For a linked
    /// worktree this is the common Git directory shared by all worktrees. If
    /// that common directory is a bare repository, there may be no main
    /// worktree path to derive from it.
    pub common_dir_abs_path: Arc<Path>,
    pub path_style: PathStyle,
    pub branch: Option<Branch>,
    pub branch_list: Arc<[Branch]>,
    pub branch_list_error: Option<SharedString>,
    pub head_commit: Option<CommitDetails>,
    pub scan_id: u64,
    pub merge: MergeDetails,
    pub remote_origin_url: Option<String>,
    pub remote_upstream_url: Option<String>,
    pub stash_entries: GitStash,
    pub linked_worktrees: Arc<[GitWorktree]>,
}

type JobId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobInfo {
    pub start: Instant,
    pub message: SharedString,
}

struct CommitDataHandler {
    _task: Task<()>,
    commit_data_request: async_channel::Sender<Oid>,
    completion_senders: HashMap<Oid, oneshot::Sender<Arc<CommitData>>>,
    pending_requests: HashSet<Oid>,
}

/// Represents the handler of a git cat-file --batch process within Mav
/// It's used to lazily fetch commit data as needed (whatever a user is viewing)
enum CommitDataHandlerState {
    /// The handler is open and processing requests
    Open(CommitDataHandler),
    /// The handler closed because it didn't receive any requests in the last 10s
    /// or hasn't been open before
    Closed,
}

enum NextCommitDataRequest {
    Request(BoxFuture<'static, Result<proto::GetCommitDataResponse>>),
    Idle,
    Closed,
}

pub struct InitialGitGraphData {
    fetch_task: Task<()>,
    pub error: Option<SharedString>,
    pub commit_data: Vec<Arc<InitialGraphCommitData>>,
    pub commit_oid_to_index: HashMap<Oid, usize>,
    subscribers: Vec<async_channel::Sender<Result<Vec<Arc<InitialGraphCommitData>>, SharedString>>>,
}

pub struct GraphDataResponse<'a> {
    pub commits: &'a [Arc<InitialGraphCommitData>],
    pub is_loading: bool,
    pub error: Option<SharedString>,
}

pub struct Repository {
    this: WeakEntity<Self>,
    snapshot: RepositorySnapshot,
    commit_message_buffer: Option<Entity<Buffer>>,
    git_store: WeakEntity<GitStore>,
    // For a local repository, holds paths that have had worktree events since the last status scan completed,
    // and that should be examined during the next status scan.
    paths_needing_status_update: Vec<Vec<RepoPath>>,
    job_sender: mpsc::UnboundedSender<GitJob>,
    _worker_task: Task<()>,
    active_jobs: HashMap<JobId, JobInfo>,
    job_debug_queue: job_debug_queue::GitJobDebugQueue,
    pending_ops: SumTree<PendingOps>,
    job_id: JobId,
    askpass_delegates: Arc<Mutex<HashMap<u64, AskPassDelegate>>>,
    latest_askpass_id: u64,
    repository_state: Shared<Task<Result<RepositoryState, String>>>,
    initial_graph_data: HashMap<(LogSource, LogOrder), InitialGitGraphData>,
    commit_data_handler: CommitDataHandlerState,
    commit_data: HashMap<Oid, CommitDataState>,
}

impl std::ops::Deref for Repository {
    type Target = RepositorySnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snapshot
    }
}

#[derive(Clone)]
pub struct LocalRepositoryState {
    pub fs: Arc<dyn Fs>,
    pub backend: Arc<dyn GitRepository>,
    pub environment: Arc<HashMap<String, String>>,
}

impl LocalRepositoryState {
    async fn new(
        work_directory_abs_path: Arc<Path>,
        dot_git_abs_path: Arc<Path>,
        project_environment: WeakEntity<ProjectEnvironment>,
        fs: Arc<dyn Fs>,
        is_trusted: bool,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<Self> {
        let environment = project_environment
                .update(cx, |project_environment, cx| {
                    project_environment.local_directory_environment(&Shell::System, work_directory_abs_path.clone(), cx)
                })?
                .await
                .unwrap_or_else(|| {
                    log::error!("failed to get working directory environment for repository {work_directory_abs_path:?}");
                    HashMap::default()
                });
        let search_paths = environment.get("PATH").map(|val| val.to_owned());
        let backend = cx
            .background_spawn({
                let fs = fs.clone();
                async move {
                    let system_git_binary_path = search_paths
                        .and_then(|search_paths| {
                            which::which_in("git", Some(search_paths), &work_directory_abs_path)
                                .ok()
                        })
                        .or_else(|| which::which("git").ok());
                    fs.open_repo(&dot_git_abs_path, system_git_binary_path.as_deref())
                        .with_context(|| format!("opening repository at {dot_git_abs_path:?}"))
                }
            })
            .await?;
        backend.set_trusted(is_trusted);
        Ok(LocalRepositoryState {
            backend,
            environment: Arc::new(environment),
            fs,
        })
    }
}

#[derive(Clone)]
pub struct RemoteRepositoryState {
    pub project_id: ProjectId,
    pub client: AnyProtoClient,
}

#[derive(Clone)]
pub enum RepositoryState {
    Local(LocalRepositoryState),
    Remote(RemoteRepositoryState),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitGraphEvent {
    CountUpdated(usize),
    FullyLoaded,
    LoadingError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepositoryEvent {
    StatusesChanged,
    HeadChanged,
    BranchListChanged,
    StashEntriesChanged,
    GitWorktreeListChanged,
    PendingOpsChanged { pending_ops: SumTree<PendingOps> },
    GraphEvent((LogSource, LogOrder), GitGraphEvent),
}

#[derive(Clone, Debug)]
pub struct JobsUpdated;

#[derive(Debug)]
pub enum GitStoreEvent {
    ActiveRepositoryChanged(Option<RepositoryId>),
    /// Bool is true when the repository that's updated is the active repository
    RepositoryUpdated(RepositoryId, RepositoryEvent, bool),
    RepositoryAdded,
    RepositoryRemoved(RepositoryId),
    IndexWriteError(anyhow::Error),
    JobsUpdated,
    ConflictsUpdated,
    GlobalConfigurationUpdated,
}

impl EventEmitter<RepositoryEvent> for Repository {}
impl EventEmitter<JobsUpdated> for Repository {}
impl EventEmitter<GitStoreEvent> for GitStore {}
