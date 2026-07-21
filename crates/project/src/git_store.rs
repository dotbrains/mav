pub mod branch_diff;
mod branch_remote_handlers;
mod buffer_git_state_recalculation;
mod checkpoint_handlers;
mod commit_data;
mod commit_handlers;
mod conflict_set;
mod diff_opening;
mod diff_request_handlers;
pub mod git_traversal;
mod graph_data;
pub mod job_debug_queue;
pub mod pending_op;
mod repository_branch_ops;
mod repository_core;
mod repository_diff_bases;
mod repository_jobs;
mod repository_lifecycle;
mod repository_remote_commands;
mod repository_request_handlers;
mod repository_stash;
mod repository_worktrees;
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

#[derive(Debug, Default, Clone, Copy)]
pub enum GitAccess {
    /// Either:
    /// - the user owns `.git`
    /// - the user doesn't own `.git`, but has both of:
    ///   - OS-level read permissions
    ///   - the directory is marked as safe (git config safe.directory)
    #[default]
    Yes,

    /// The user is not the owner of `.git`, and one of the following is true:
    /// - the directory is not marked as safe (git config safe.directory)
    /// - the user does not have OS-level read permissions to `.git`
    No,
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

impl StatusEntry {
    fn to_proto(&self) -> proto::StatusEntry {
        let simple_status = match self.status {
            FileStatus::Ignored | FileStatus::Untracked => proto::GitStatus::Added as i32,
            FileStatus::Unmerged { .. } => proto::GitStatus::Conflict as i32,
            FileStatus::Tracked(TrackedStatus {
                index_status,
                worktree_status,
            }) => tracked_status_to_proto(if worktree_status != StatusCode::Unmodified {
                worktree_status
            } else {
                index_status
            }),
        };

        proto::StatusEntry {
            repo_path: self.repo_path.to_proto(),
            simple_status,
            status: Some(status_to_proto(self.status)),
            diff_stat_added: self.diff_stat.map(|ds| ds.added),
            diff_stat_deleted: self.diff_stat.map(|ds| ds.deleted),
        }
    }
}

impl TryFrom<proto::StatusEntry> for StatusEntry {
    type Error = anyhow::Error;

    fn try_from(value: proto::StatusEntry) -> Result<Self, Self::Error> {
        let repo_path = RepoPath::from_proto(&value.repo_path).context("invalid repo path")?;
        let status = status_from_proto(value.simple_status, value.status)?;
        let diff_stat = match (value.diff_stat_added, value.diff_stat_deleted) {
            (Some(added), Some(deleted)) => Some(DiffStat { added, deleted }),
            _ => None,
        };
        Ok(Self {
            repo_path,
            status,
            diff_stat,
        })
    }
}

impl sum_tree::Item for StatusEntry {
    type Summary = PathSummary<GitSummary>;

    fn summary(&self, _: <Self::Summary as sum_tree::Summary>::Context<'_>) -> Self::Summary {
        PathSummary {
            max_path: self.repo_path.as_ref().clone(),
            item_summary: self.status.summary(),
        }
    }
}

impl sum_tree::KeyedItem for StatusEntry {
    type Key = PathKey;

    fn key(&self) -> Self::Key {
        PathKey(self.repo_path.as_ref().clone())
    }
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

pub struct GitJob {
    id: JobId,
    job: Box<dyn FnOnce(RepositoryState, &mut AsyncApp) -> Task<()>>,
    key: Option<GitJobKey>,
}

#[derive(PartialEq, Eq)]
enum GitJobKey {
    WriteIndex(Vec<RepoPath>),
    ReloadBufferDiffBases,
    RefreshStatuses,
    ReloadGitState,
}

impl GitStore {
    pub fn local(
        worktree_store: &Entity<WorktreeStore>,
        buffer_store: Entity<BufferStore>,
        environment: Entity<ProjectEnvironment>,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) -> Self {
        let _fs_watches = if fs.is_fake() {
            Box::new([])
        } else {
            [
                config_dir().join("git/config"),
                home_dir().join(".gitconfig"),
            ]
            .into_iter()
            .map(|path| {
                let fs = fs.clone();

                cx.spawn(async move |this, cx| {
                    let watcher = fs.watch(&path, Duration::from_millis(100));
                    let (mut watcher, _) = watcher.await;
                    while let Some(_) = watcher.next().await {
                        let Ok(_) = this.update(cx, |this, cx| {
                            let GitStoreState::Local {
                                project_environment,
                                fs,
                                ..
                            } = &this.state
                            else {
                                return;
                            };
                            let project_environment = project_environment.downgrade();
                            let fs = fs.clone();
                            let repositories_to_respawn = this
                                .repositories
                                .iter()
                                .filter_map(|(repository_id, repo)| {
                                    repo.read(cx)
                                        .job_sender
                                        .is_closed()
                                        .then_some((*repository_id, repo.clone()))
                                })
                                .collect::<Vec<_>>();
                            for (repository_id, repo) in repositories_to_respawn {
                                let is_trusted = this.repository_is_trusted(repository_id, cx);
                                repo.update(cx, |repo, cx| {
                                    repo.respawn_local_worker(
                                        project_environment.clone(),
                                        fs.clone(),
                                        is_trusted,
                                        cx,
                                    );
                                    repo.schedule_scan(None, cx);
                                })
                            }
                            cx.emit(GitStoreEvent::GlobalConfigurationUpdated);
                        }) else {
                            return;
                        };
                    }
                })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice()
        };

        Self::new(
            worktree_store.clone(),
            buffer_store,
            GitStoreState::Local {
                next_repository_id: Arc::new(AtomicU64::new(1)),
                downstream: None,
                project_environment: environment,
                _fs_watches,
                fs,
            },
            cx,
        )
    }

    pub fn remote(
        worktree_store: &Entity<WorktreeStore>,
        buffer_store: Entity<BufferStore>,
        upstream_client: AnyProtoClient,
        project_id: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(
            worktree_store.clone(),
            buffer_store,
            GitStoreState::Remote {
                upstream_client,
                upstream_project_id: project_id,
                downstream: None,
            },
            cx,
        )
    }

    fn new(
        worktree_store: Entity<WorktreeStore>,
        buffer_store: Entity<BufferStore>,
        state: GitStoreState,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut _subscriptions = vec![
            cx.subscribe(&worktree_store, Self::on_worktree_store_event),
            cx.subscribe(&buffer_store, Self::on_buffer_store_event),
        ];

        if let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) {
            _subscriptions.push(cx.subscribe(&trusted_worktrees, Self::on_trusted_worktrees_event));
        }

        GitStore {
            state,
            buffer_store,
            worktree_store,
            repositories: HashMap::default(),
            worktree_ids: HashMap::default(),
            active_repo_id: None,
            _subscriptions,
            loading_diffs: HashMap::default(),
            shared_diffs: HashMap::default(),
            diffs: HashMap::default(),
        }
    }

    pub fn init(client: &AnyProtoClient) {
        client.add_entity_request_handler(Self::handle_get_remotes);
        client.add_entity_request_handler(Self::handle_get_branches);
        client.add_entity_request_handler(Self::handle_get_default_branch);
        client.add_entity_request_handler(Self::handle_change_branch);
        client.add_entity_request_handler(Self::handle_create_branch);
        client.add_entity_request_handler(Self::handle_rename_branch);
        client.add_entity_request_handler(Self::handle_create_remote);
        client.add_entity_request_handler(Self::handle_remove_remote);
        client.add_entity_request_handler(Self::handle_delete_branch);
        client.add_entity_request_handler(Self::handle_git_init);
        client.add_entity_request_handler(Self::handle_push);
        client.add_entity_request_handler(Self::handle_pull);
        client.add_entity_request_handler(Self::handle_fetch);
        client.add_entity_request_handler(Self::handle_stage);
        client.add_entity_request_handler(Self::handle_unstage);
        client.add_entity_request_handler(Self::handle_stash);
        client.add_entity_request_handler(Self::handle_stash_pop);
        client.add_entity_request_handler(Self::handle_stash_apply);
        client.add_entity_request_handler(Self::handle_stash_drop);
        client.add_entity_request_handler(Self::handle_commit);
        client.add_entity_request_handler(Self::handle_run_hook);
        client.add_entity_request_handler(Self::handle_reset);
        client.add_entity_request_handler(Self::handle_show);
        client.add_entity_request_handler(Self::handle_create_checkpoint);
        client.add_entity_request_handler(Self::handle_create_archive_checkpoint);
        client.add_entity_request_handler(Self::handle_restore_checkpoint);
        client.add_entity_request_handler(Self::handle_restore_archive_checkpoint);
        client.add_entity_request_handler(Self::handle_compare_checkpoints);
        client.add_entity_request_handler(Self::handle_diff_checkpoints);
        client.add_entity_request_handler(Self::handle_load_commit_diff);
        client.add_entity_request_handler(Self::handle_checkout_files);
        client.add_entity_request_handler(Self::handle_open_commit_message_buffer);
        client.add_entity_request_handler(Self::handle_set_index_text);
        client.add_entity_request_handler(Self::handle_askpass);
        client.add_entity_request_handler(Self::handle_check_for_pushed_commits);
        client.add_entity_request_handler(Self::handle_git_diff);
        client.add_entity_request_handler(Self::handle_tree_diff);
        client.add_entity_request_handler(Self::handle_get_blob_content);
        client.add_entity_request_handler(Self::handle_open_unstaged_diff);
        client.add_entity_request_handler(Self::handle_open_uncommitted_diff);
        client.add_entity_message_handler(Self::handle_update_diff_bases);
        client.add_entity_request_handler(Self::handle_get_permalink_to_line);
        client.add_entity_request_handler(Self::handle_blame_buffer);
        client.add_entity_message_handler(Self::handle_update_repository);
        client.add_entity_message_handler(Self::handle_remove_repository);
        client.add_entity_request_handler(Self::handle_git_clone);
        client.add_entity_request_handler(Self::handle_get_worktrees);
        client.add_entity_request_handler(Self::handle_create_worktree);
        client.add_entity_request_handler(Self::handle_remove_worktree);
        client.add_entity_request_handler(Self::handle_rename_worktree);
        client.add_entity_request_handler(Self::handle_worktree_created_at);
        client.add_entity_request_handler(Self::handle_get_head_sha);
        client.add_entity_request_handler(Self::handle_edit_ref);
        client.add_entity_request_handler(Self::handle_repair_worktrees);
        client.add_entity_request_handler(Self::handle_get_commit_data);
        client.add_entity_stream_request_handler(Self::handle_get_initial_graph_data);
        client.add_entity_stream_request_handler(Self::handle_search_commits);
    }

    pub fn is_local(&self) -> bool {
        matches!(self.state, GitStoreState::Local { .. })
    }

    fn set_active_repo_id(&mut self, repo_id: RepositoryId, cx: &mut Context<Self>) {
        if self.active_repo_id != Some(repo_id) {
            self.active_repo_id = Some(repo_id);
            cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(repo_id)));
        }
    }

    pub fn set_active_repo_for_path(&mut self, project_path: &ProjectPath, cx: &mut Context<Self>) {
        if let Some((repo, _)) = self.repository_and_path_for_project_path(project_path, cx) {
            self.set_active_repo_id(repo.read(cx).id, cx);
        }
    }

    pub fn set_active_repo_for_worktree(
        &mut self,
        worktree_id: WorktreeId,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(worktree_id, cx)
        else {
            return;
        };
        let worktree_abs_path = worktree.read(cx).abs_path();
        let Some(repo_id) = self
            .repositories
            .values()
            .filter(|repo| {
                let repo_path = &repo.read(cx).work_directory_abs_path;
                // The folder opened in Mav isn't necessarily the repo root; it may be
                // a subdirectory of it, e.g. opening `~/code/myrepo/backend` when the
                // repo lives at `~/code/myrepo`. So match any repo whose work directory
                // contains the folder. Nested repos can produce multiple matches, e.g.
                // opening `~/code/myrepo/vendor/lib` where `vendor/lib` is a submodule
                // matches both `myrepo` and the submodule; `max_by_key` then picks the
                // innermost match (the submodule), which the folder actually belongs to.
                worktree_abs_path.starts_with(repo_path.as_ref())
            })
            .max_by_key(|repo| repo.read(cx).work_directory_abs_path.as_os_str().len())
            .map(|repo| repo.read(cx).id)
        else {
            return;
        };

        self.set_active_repo_id(repo_id, cx);
    }

    pub fn shared(&mut self, project_id: u64, client: AnyProtoClient, cx: &mut Context<Self>) {
        match &mut self.state {
            GitStoreState::Remote {
                downstream: downstream_client,
                ..
            } => {
                for repo in self.repositories.values() {
                    let update = repo.read(cx).snapshot.initial_update(project_id);
                    for update in split_repository_update(update) {
                        client.send(update).log_err();
                    }
                }
                *downstream_client = Some((client, ProjectId(project_id)));
            }
            GitStoreState::Local {
                downstream: downstream_client,
                ..
            } => {
                let mut snapshots = HashMap::default();
                let (updates_tx, mut updates_rx) = mpsc::unbounded();
                for repo in self.repositories.values() {
                    updates_tx
                        .unbounded_send(DownstreamUpdate::UpdateRepository(
                            repo.read(cx).snapshot.clone(),
                        ))
                        .ok();
                }
                *downstream_client = Some(LocalDownstreamState {
                    client: client.clone(),
                    project_id: ProjectId(project_id),
                    updates_tx,
                    _task: cx.spawn(async move |this, cx| {
                        cx.background_spawn(async move {
                            while let Some(update) = updates_rx.next().await {
                                match update {
                                    DownstreamUpdate::UpdateRepository(snapshot) => {
                                        if let Some(old_snapshot) = snapshots.get_mut(&snapshot.id)
                                        {
                                            let update =
                                                snapshot.build_update(old_snapshot, project_id);
                                            *old_snapshot = snapshot;
                                            for update in split_repository_update(update) {
                                                client.send(update)?;
                                            }
                                        } else {
                                            let update = snapshot.initial_update(project_id);
                                            for update in split_repository_update(update) {
                                                client.send(update)?;
                                            }
                                            snapshots.insert(snapshot.id, snapshot);
                                        }
                                    }
                                    DownstreamUpdate::RemoveRepository(id) => {
                                        client.send(proto::RemoveRepository {
                                            project_id,
                                            id: id.to_proto(),
                                        })?;
                                    }
                                }
                            }
                            anyhow::Ok(())
                        })
                        .await
                        .ok();
                        this.update(cx, |this, _| {
                            if let GitStoreState::Local {
                                downstream: downstream_client,
                                ..
                            } = &mut this.state
                            {
                                downstream_client.take();
                            } else {
                                unreachable!("unshared called on remote store");
                            }
                        })
                    }),
                });
            }
        }
    }

    pub fn unshared(&mut self, _cx: &mut Context<Self>) {
        match &mut self.state {
            GitStoreState::Local {
                downstream: downstream_client,
                ..
            } => {
                downstream_client.take();
            }
            GitStoreState::Remote {
                downstream: downstream_client,
                ..
            } => {
                downstream_client.take();
            }
        }
        self.shared_diffs.clear();
    }

    pub(crate) fn forget_shared_diffs_for(&mut self, peer_id: &proto::PeerId) {
        self.shared_diffs.remove(peer_id);
    }

    pub fn active_repository(&self) -> Option<Entity<Repository>> {
        self.active_repo_id
            .as_ref()
            .map(|id| self.repositories[id].clone())
    }

    pub fn open_conflict_set(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Entity<ConflictSet>> {
        let buffer_id = buffer.read(cx).remote_id();

        if let Some(git_state) = self.diffs.get(&buffer_id)
            && let Some(conflict_set) = git_state
                .read(cx)
                .conflict_set
                .as_ref()
                .and_then(|weak| weak.upgrade())
        {
            let conflict_set = conflict_set;
            let buffer_snapshot = buffer.read(cx).text_snapshot();

            let rx = git_state.update(cx, |state, cx| {
                state.reparse_conflict_markers(buffer_snapshot, cx)
            });

            return cx.spawn(async move |_, _| {
                rx.await.ok();
                conflict_set
            });
        }

        let is_unmerged = self
            .repository_and_path_for_buffer_id(buffer_id, cx)
            .is_some_and(|(repo, path)| repo.read(cx).snapshot.has_conflict(&path));
        let git_store = cx.weak_entity();
        let buffer_git_state = self
            .diffs
            .entry(buffer_id)
            .or_insert_with(|| cx.new(|cx| BufferGitState::new(git_store, cx)));
        let conflict_set = cx.new(|cx| ConflictSet::new(buffer_id, is_unmerged, cx));

        self._subscriptions
            .push(cx.subscribe(&conflict_set, |_, _, _, cx| {
                cx.emit(GitStoreEvent::ConflictsUpdated);
            }));

        let rx = buffer_git_state.update(cx, |state, cx| {
            state.conflict_set = Some(conflict_set.downgrade());
            let buffer_snapshot = buffer.read(cx).text_snapshot();
            state.reparse_conflict_markers(buffer_snapshot, cx)
        });

        cx.spawn(async move |_, _| {
            rx.await.ok();
            conflict_set
        })
    }

    pub fn project_path_git_status(
        &self,
        project_path: &ProjectPath,
        cx: &App,
    ) -> Option<FileStatus> {
        let (repo, repo_path) = self.repository_and_path_for_project_path(project_path, cx)?;
        Some(repo.read(cx).status_for_path(&repo_path)?.status)
    }

    pub fn checkpoint(&self, cx: &mut App) -> Task<Result<GitStoreCheckpoint>> {
        let mut work_directory_abs_paths = Vec::new();
        let mut checkpoints = Vec::new();
        for repository in self.repositories.values() {
            repository.update(cx, |repository, _| {
                work_directory_abs_paths.push(repository.snapshot.work_directory_abs_path.clone());
                checkpoints.push(repository.checkpoint().map(|checkpoint| checkpoint?));
            });
        }

        cx.background_executor().spawn(async move {
            let checkpoints = future::try_join_all(checkpoints).await?;
            Ok(GitStoreCheckpoint {
                checkpoints_by_work_dir_abs_path: work_directory_abs_paths
                    .into_iter()
                    .zip(checkpoints)
                    .collect(),
            })
        })
    }

    pub fn restore_checkpoint(
        &self,
        checkpoint: GitStoreCheckpoint,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let repositories_by_work_dir_abs_path = self
            .repositories
            .values()
            .map(|repo| (repo.read(cx).snapshot.work_directory_abs_path.clone(), repo))
            .collect::<HashMap<_, _>>();

        let mut tasks = Vec::new();
        for (work_dir_abs_path, checkpoint) in checkpoint.checkpoints_by_work_dir_abs_path {
            if let Some(repository) = repositories_by_work_dir_abs_path.get(&work_dir_abs_path) {
                let restore = repository.update(cx, |repository, _| {
                    repository.restore_checkpoint(checkpoint)
                });
                tasks.push(async move { restore.await? });
            }
        }
        cx.background_spawn(async move {
            future::try_join_all(tasks).await?;
            Ok(())
        })
    }

    /// Compares two checkpoints, returning true if they are equal.
    pub fn compare_checkpoints(
        &self,
        left: GitStoreCheckpoint,
        mut right: GitStoreCheckpoint,
        cx: &mut App,
    ) -> Task<Result<bool>> {
        let repositories_by_work_dir_abs_path = self
            .repositories
            .values()
            .map(|repo| (repo.read(cx).snapshot.work_directory_abs_path.clone(), repo))
            .collect::<HashMap<_, _>>();

        let mut tasks = Vec::new();
        for (work_dir_abs_path, left_checkpoint) in left.checkpoints_by_work_dir_abs_path {
            if let Some(right_checkpoint) = right
                .checkpoints_by_work_dir_abs_path
                .remove(&work_dir_abs_path)
            {
                if let Some(repository) = repositories_by_work_dir_abs_path.get(&work_dir_abs_path)
                {
                    let compare = repository.update(cx, |repository, _| {
                        repository.compare_checkpoints(left_checkpoint, right_checkpoint)
                    });

                    tasks.push(async move { compare.await? });
                }
            } else {
                return Task::ready(Ok(false));
            }
        }
        cx.background_spawn(async move {
            Ok(future::try_join_all(tasks)
                .await?
                .into_iter()
                .all(|result| result))
        })
    }

    /// Blames a buffer.
    pub fn blame_buffer(
        &self,
        buffer: &Entity<Buffer>,
        version: Option<clock::Global>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Blame>>> {
        let buffer = buffer.read(cx);
        let Some((repo, repo_path)) =
            self.repository_and_path_for_buffer_id(buffer.remote_id(), cx)
        else {
            return Task::ready(Err(anyhow!("failed to find a git repository for buffer")));
        };
        let content = match &version {
            Some(version) => buffer.rope_for_version(version),
            None => buffer.as_rope().clone(),
        };
        let line_ending = buffer.line_ending();
        let version = version.unwrap_or(buffer.version());
        let buffer_id = buffer.remote_id();

        let repo = repo.downgrade();
        cx.spawn(async move |_, cx| {
            let repository_state = repo
                .update(cx, |repo, _| repo.repository_state.clone())?
                .await
                .map_err(|err| anyhow::anyhow!(err))?;
            match repository_state {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => backend
                    .blame(repo_path.clone(), content, line_ending)
                    .await
                    .with_context(|| format!("Failed to blame {:?}", repo_path.as_ref()))
                    .map(Some),
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::BlameBuffer {
                            project_id: project_id.to_proto(),
                            buffer_id: buffer_id.into(),
                            version: serialize_version(&version),
                        })
                        .await?;
                    Ok(deserialize_blame_buffer_response(response))
                }
            }
        })
    }

    pub fn get_permalink_to_line(
        &self,
        buffer: &Entity<Buffer>,
        selection: Range<u32>,
        cx: &mut App,
    ) -> Task<Result<url::Url>> {
        let Some(file) = File::from_dyn(buffer.read(cx).file()) else {
            return Task::ready(Err(anyhow!("buffer has no file")));
        };

        let Some((repo, repo_path)) = self.repository_and_path_for_project_path(
            &(file.worktree.read(cx).id(), file.path.clone()).into(),
            cx,
        ) else {
            // If we're not in a Git repo, check whether this is a Rust source
            // file in the Cargo registry (presumably opened with go-to-definition
            // from a normal Rust file). If so, we can put together a permalink
            // using crate metadata.
            if buffer
                .read(cx)
                .language()
                .is_none_or(|lang| lang.name() != "Rust")
            {
                return Task::ready(Err(anyhow!("no permalink available")));
            }
            let file_path = file.worktree.read(cx).absolutize(&file.path);
            return cx.spawn(async move |cx| {
                let provider_registry = cx.update(GitHostingProviderRegistry::default_global);
                get_permalink_in_rust_registry_src(provider_registry, file_path, selection)
                    .context("no permalink available")
            });
        };

        let buffer_id = buffer.read(cx).remote_id();
        let branch = repo.read(cx).branch.clone();
        let remote = branch
            .as_ref()
            .and_then(|b| b.upstream.as_ref())
            .and_then(|b| b.remote_name())
            .unwrap_or("origin")
            .to_string();

        let rx = repo.update(cx, |repo, _| {
            repo.send_job("get_permalink_to_line", None, move |state, cx| async move {
                match state {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        let origin_url = backend
                            .remote_url(&remote)
                            .await
                            .with_context(|| format!("remote \"{remote}\" not found"))?;

                        let sha = backend.head_sha().await.context("reading HEAD SHA")?;

                        let provider_registry =
                            cx.update(GitHostingProviderRegistry::default_global);

                        let (provider, remote) =
                            parse_git_remote_url(provider_registry, &origin_url)
                                .context("parsing Git remote URL")?;

                        Ok(provider.build_permalink(
                            remote,
                            BuildPermalinkParams::new(&sha, &repo_path, Some(selection)),
                        ))
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        let response = client
                            .request(proto::GetPermalinkToLine {
                                project_id: project_id.to_proto(),
                                buffer_id: buffer_id.into(),
                                selection: Some(proto::Range {
                                    start: selection.start as u64,
                                    end: selection.end as u64,
                                }),
                            })
                            .await?;

                        url::Url::parse(&response.permalink).context("failed to parse permalink")
                    }
                }
            })
        });
        cx.spawn(|_: &mut AsyncApp| async move { rx.await? })
    }

    fn downstream_client(&self) -> Option<(AnyProtoClient, ProjectId)> {
        match &self.state {
            GitStoreState::Local {
                downstream: downstream_client,
                ..
            } => downstream_client
                .as_ref()
                .map(|state| (state.client.clone(), state.project_id)),
            GitStoreState::Remote {
                downstream: downstream_client,
                ..
            } => downstream_client.clone(),
        }
    }

    fn upstream_client(&self) -> Option<AnyProtoClient> {
        match &self.state {
            GitStoreState::Local { .. } => None,
            GitStoreState::Remote {
                upstream_client, ..
            } => Some(upstream_client.clone()),
        }
    }

    fn on_worktree_store_event(
        &mut self,
        worktree_store: Entity<WorktreeStore>,
        event: &WorktreeStoreEvent,
        cx: &mut Context<Self>,
    ) {
        let GitStoreState::Local {
            project_environment,
            downstream,
            next_repository_id,
            fs,
            ..
        } = &self.state
        else {
            return;
        };

        match event {
            WorktreeStoreEvent::WorktreeUpdatedEntries(worktree_id, updated_entries) => {
                if let Some(worktree) = self
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(*worktree_id, cx)
                {
                    let paths_by_git_repo =
                        self.process_updated_entries(&worktree, updated_entries, cx);
                    let downstream = downstream
                        .as_ref()
                        .map(|downstream| downstream.updates_tx.clone());
                    cx.spawn(async move |_, cx| {
                        let paths_by_git_repo = paths_by_git_repo.await;
                        for (repo, paths) in paths_by_git_repo {
                            repo.update(cx, |repo, cx| {
                                repo.paths_changed(paths, downstream.clone(), cx);
                            });
                        }
                    })
                    .detach();
                }
            }
            WorktreeStoreEvent::WorktreeUpdatedGitRepositories(worktree_id, changed_repos) => {
                let Some(worktree) = worktree_store.read(cx).worktree_for_id(*worktree_id, cx)
                else {
                    return;
                };
                log::debug!("received worktree update for repositories: {changed_repos:?}");
                self.update_repositories_from_worktree(
                    *worktree_id,
                    project_environment.clone(),
                    next_repository_id.clone(),
                    downstream
                        .as_ref()
                        .map(|downstream| downstream.updates_tx.clone()),
                    changed_repos.clone(),
                    fs.clone(),
                    cx,
                );
                self.local_worktree_git_repos_changed(worktree, changed_repos, cx);
            }
            WorktreeStoreEvent::WorktreeRemoved(_entity_id, worktree_id) => {
                let repos_without_worktree: Vec<RepositoryId> = self
                    .worktree_ids
                    .iter_mut()
                    .filter_map(|(repo_id, worktree_ids)| {
                        worktree_ids.remove(worktree_id);
                        if worktree_ids.is_empty() {
                            Some(*repo_id)
                        } else {
                            None
                        }
                    })
                    .collect();
                let is_active_repo_removed = repos_without_worktree
                    .iter()
                    .any(|repo_id| self.active_repo_id == Some(*repo_id));

                for repo_id in repos_without_worktree {
                    self.repositories.remove(&repo_id);
                    self.worktree_ids.remove(&repo_id);
                    if let Some(updates_tx) =
                        downstream.as_ref().map(|downstream| &downstream.updates_tx)
                    {
                        updates_tx
                            .unbounded_send(DownstreamUpdate::RemoveRepository(repo_id))
                            .ok();
                    }
                }

                if is_active_repo_removed {
                    if let Some((&repo_id, _)) = self.repositories.iter().next() {
                        self.active_repo_id = Some(repo_id);
                        cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(repo_id)));
                    } else {
                        self.active_repo_id = None;
                        cx.emit(GitStoreEvent::ActiveRepositoryChanged(None));
                    }
                }
            }
            _ => {}
        }
    }
    fn on_repository_event(
        &mut self,
        repo: Entity<Repository>,
        event: &RepositoryEvent,
        cx: &mut Context<Self>,
    ) {
        let id = repo.read(cx).id;
        let repo_snapshot = repo.read(cx).snapshot.clone();
        for (buffer_id, diff) in self.diffs.iter() {
            if let Some((buffer_repo, repo_path)) =
                self.repository_and_path_for_buffer_id(*buffer_id, cx)
                && buffer_repo == repo
            {
                diff.update(cx, |diff, cx| {
                    if let Some(conflict_set) = &diff.conflict_set {
                        let conflict_status_changed =
                            conflict_set.update(cx, |conflict_set, cx| {
                                let has_conflict = repo_snapshot.has_conflict(&repo_path);
                                conflict_set.set_has_conflict(has_conflict, cx)
                            })?;
                        if conflict_status_changed {
                            let buffer_store = self.buffer_store.read(cx);
                            if let Some(buffer) = buffer_store.get(*buffer_id) {
                                let _ = diff
                                    .reparse_conflict_markers(buffer.read(cx).text_snapshot(), cx);
                            }
                        }
                    }
                    anyhow::Ok(())
                })
                .ok();
            }
        }
        cx.emit(GitStoreEvent::RepositoryUpdated(
            id,
            event.clone(),
            self.active_repo_id == Some(id),
        ))
    }

    fn on_jobs_updated(&mut self, _: Entity<Repository>, _: &JobsUpdated, cx: &mut Context<Self>) {
        cx.emit(GitStoreEvent::JobsUpdated)
    }

    fn repository_is_trusted(&self, repository_id: RepositoryId, cx: &mut Context<Self>) -> bool {
        let Some(worktree_ids) = self.worktree_ids.get(&repository_id) else {
            return false;
        };
        let Some(trusted_worktrees) = TrustedWorktrees::try_get_global(cx) else {
            return false;
        };

        worktree_ids.iter().any(|worktree_id| {
            trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                trusted_worktrees.can_trust(&self.worktree_store, *worktree_id, cx)
            })
        })
    }

    /// Update our list of repositories and schedule git scans in response to a notification from a worktree,
    fn update_repositories_from_worktree(
        &mut self,
        worktree_id: WorktreeId,
        project_environment: Entity<ProjectEnvironment>,
        next_repository_id: Arc<AtomicU64>,
        updates_tx: Option<mpsc::UnboundedSender<DownstreamUpdate>>,
        updated_git_repositories: UpdatedGitRepositoriesSet,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) {
        let mut removed_ids = Vec::new();
        for update in updated_git_repositories.iter() {
            if let Some((id, existing)) = self.repositories.iter().find(|(_, repo)| {
                let existing_work_directory_abs_path =
                    repo.read(cx).work_directory_abs_path.clone();
                Some(&existing_work_directory_abs_path)
                    == update.old_work_directory_abs_path.as_ref()
                    || Some(&existing_work_directory_abs_path)
                        == update.new_work_directory_abs_path.as_ref()
            }) {
                let repo_id = *id;
                if let Some(new_work_directory_abs_path) =
                    update.new_work_directory_abs_path.clone()
                {
                    self.worktree_ids
                        .entry(repo_id)
                        .or_insert_with(HashSet::new)
                        .insert(worktree_id);
                    let path_changed = update.old_work_directory_abs_path.as_ref()
                        != update.new_work_directory_abs_path.as_ref();
                    if path_changed
                        && let Some(dot_git_abs_path) = update.dot_git_abs_path.clone()
                        && let Some(repository_dir_abs_path) =
                            update.repository_dir_abs_path.clone()
                        && let Some(common_dir_abs_path) = update.common_dir_abs_path.clone()
                    {
                        let is_trusted = TrustedWorktrees::try_get_global(cx)
                            .map(|trusted_worktrees| {
                                trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                                    trusted_worktrees.can_trust(
                                        &self.worktree_store,
                                        worktree_id,
                                        cx,
                                    )
                                })
                            })
                            .unwrap_or(false);
                        existing.update(cx, |existing, cx| {
                            existing.reinitialize_local_backend(
                                new_work_directory_abs_path,
                                dot_git_abs_path,
                                repository_dir_abs_path,
                                common_dir_abs_path,
                                project_environment.downgrade(),
                                fs.clone(),
                                is_trusted,
                                cx,
                            );
                            existing.schedule_scan(updates_tx.clone(), cx);
                        });
                    } else {
                        existing.update(cx, |existing, cx| {
                            existing.snapshot.work_directory_abs_path = new_work_directory_abs_path;
                            existing.schedule_scan(updates_tx.clone(), cx);
                        });
                    }
                } else {
                    if let Some(worktree_ids) = self.worktree_ids.get_mut(&repo_id) {
                        worktree_ids.remove(&worktree_id);
                        if worktree_ids.is_empty() {
                            removed_ids.push(repo_id);
                        }
                    }
                }
            } else if let UpdatedGitRepository {
                new_work_directory_abs_path: Some(work_directory_abs_path),
                dot_git_abs_path: Some(dot_git_abs_path),
                repository_dir_abs_path: Some(repository_dir_abs_path),
                common_dir_abs_path: Some(common_dir_abs_path),
                ..
            } = update
            {
                let repository_dir_abs_path = repository_dir_abs_path.clone();
                let common_dir_abs_path = common_dir_abs_path.clone();
                let id = RepositoryId(next_repository_id.fetch_add(1, atomic::Ordering::Release));
                let is_trusted = TrustedWorktrees::try_get_global(cx)
                    .map(|trusted_worktrees| {
                        trusted_worktrees.update(cx, |trusted_worktrees, cx| {
                            trusted_worktrees.can_trust(&self.worktree_store, worktree_id, cx)
                        })
                    })
                    .unwrap_or(false);
                let git_store = cx.weak_entity();
                let repo = cx.new(|cx| {
                    let mut repo = Repository::local(
                        id,
                        work_directory_abs_path.clone(),
                        repository_dir_abs_path.clone(),
                        common_dir_abs_path.clone(),
                        dot_git_abs_path.clone(),
                        project_environment.downgrade(),
                        fs.clone(),
                        is_trusted,
                        git_store,
                        cx,
                    );
                    if let Some(updates_tx) = updates_tx.as_ref() {
                        // trigger an empty `UpdateRepository` to ensure remote active_repo_id is set correctly
                        updates_tx
                            .unbounded_send(DownstreamUpdate::UpdateRepository(repo.snapshot()))
                            .ok();
                    }
                    repo.schedule_scan(updates_tx.clone(), cx);
                    repo
                });
                self._subscriptions
                    .push(cx.subscribe(&repo, Self::on_repository_event));
                self._subscriptions
                    .push(cx.subscribe(&repo, Self::on_jobs_updated));
                self.repositories.insert(id, repo);
                self.worktree_ids.insert(id, HashSet::from([worktree_id]));
                cx.emit(GitStoreEvent::RepositoryAdded);
                self.active_repo_id.get_or_insert_with(|| {
                    cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(id)));
                    id
                });
            }
        }

        for id in removed_ids {
            if self.active_repo_id == Some(id) {
                self.active_repo_id = None;
                cx.emit(GitStoreEvent::ActiveRepositoryChanged(None));
            }
            self.repositories.remove(&id);
            if let Some(updates_tx) = updates_tx.as_ref() {
                updates_tx
                    .unbounded_send(DownstreamUpdate::RemoveRepository(id))
                    .ok();
            }
        }
    }

    fn on_trusted_worktrees_event(
        &mut self,
        _: Entity<TrustedWorktreesStore>,
        event: &TrustedWorktreesEvent,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.state, GitStoreState::Local { .. }) {
            return;
        }

        let (is_trusted, event_paths) = match event {
            TrustedWorktreesEvent::Trusted(_, trusted_paths) => (true, trusted_paths),
            TrustedWorktreesEvent::Restricted(_, restricted_paths) => (false, restricted_paths),
        };

        for (repo_id, worktree_ids) in &self.worktree_ids {
            if worktree_ids
                .iter()
                .any(|worktree_id| event_paths.contains(&PathTrust::Worktree(*worktree_id)))
            {
                if let Some(repo) = self.repositories.get(repo_id) {
                    let repository_state = repo.read(cx).repository_state.clone();
                    cx.background_spawn(async move {
                        if let Ok(RepositoryState::Local(state)) = repository_state.await {
                            state.backend.set_trusted(is_trusted);
                        }
                    })
                    .detach();
                }
            }
        }
    }

    fn on_buffer_store_event(
        &mut self,
        _: Entity<BufferStore>,
        event: &BufferStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            BufferStoreEvent::BufferAdded(buffer) => {
                cx.subscribe(buffer, |this, buffer, event, cx| {
                    if let BufferEvent::LanguageChanged(_) = event {
                        let buffer_id = buffer.read(cx).remote_id();
                        if let Some(diff_state) = this.diffs.get(&buffer_id) {
                            diff_state.update(cx, |diff_state, cx| {
                                diff_state.buffer_language_changed(buffer, cx);
                            });
                        }
                    }
                })
                .detach();
            }
            BufferStoreEvent::SharedBufferClosed(peer_id, buffer_id) => {
                if let Some(diffs) = self.shared_diffs.get_mut(peer_id) {
                    diffs.remove(buffer_id);
                }
            }
            BufferStoreEvent::BufferDropped(buffer_id) => {
                self.diffs.remove(buffer_id);
                for diffs in self.shared_diffs.values_mut() {
                    diffs.remove(buffer_id);
                }
            }
            BufferStoreEvent::BufferChangedFilePath { buffer, .. } => {
                // Whenever a buffer's file path changes, it's possible that the
                // new path is actually a path that is being tracked by a git
                // repository. In that case, we'll want to update the buffer's
                // `BufferDiffState`, in case it already has one.
                let buffer_id = buffer.read(cx).remote_id();
                let diff_state = self.diffs.get(&buffer_id);
                let repo = self.repository_and_path_for_buffer_id(buffer_id, cx);

                if let Some(diff_state) = diff_state
                    && let Some((repo, repo_path)) = repo
                {
                    let buffer = buffer.clone();
                    let diff_state = diff_state.clone();
                    let is_symlink = Self::buffer_is_symlink(&buffer, cx);

                    cx.spawn(async move |_git_store, cx| {
                        async {
                            let diff_bases_change = if is_symlink {
                                DiffBasesChange::SetBoth(None)
                            } else {
                                repo.update(cx, |repo, cx| {
                                    repo.load_committed_text(buffer_id, repo_path, cx)
                                })
                                .await?
                            };

                            diff_state.update(cx, |diff_state, cx| {
                                let buffer_snapshot = buffer.read(cx).text_snapshot();
                                diff_state.diff_bases_changed(
                                    buffer_snapshot,
                                    Some(diff_bases_change),
                                    cx,
                                );
                            });
                            anyhow::Ok(())
                        }
                        .await
                        .log_err();
                    })
                    .detach();
                }
            }
        }
    }

    pub fn recalculate_buffer_diffs(
        &mut self,
        buffers: Vec<Entity<Buffer>>,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = ()> + use<> {
        let mut futures = Vec::new();
        for buffer in buffers {
            if let Some(diff_state) = self.diffs.get_mut(&buffer.read(cx).remote_id()) {
                let buffer = buffer.read(cx).text_snapshot();
                diff_state.update(cx, |diff_state, cx| {
                    diff_state.recalculate_diffs(buffer.clone(), cx);
                    futures.extend(diff_state.wait_for_recalculation().map(FutureExt::boxed));
                });
                futures.push(diff_state.update(cx, |diff_state, cx| {
                    diff_state
                        .reparse_conflict_markers(buffer, cx)
                        .map(|_| {})
                        .boxed()
                }));
            }
        }
        async move {
            futures::future::join_all(futures).await;
        }
    }

    fn on_buffer_diff_event(
        &mut self,
        diff: Entity<buffer_diff::BufferDiff>,
        event: &BufferDiffEvent,
        cx: &mut Context<Self>,
    ) {
        if let BufferDiffEvent::HunksStagedOrUnstaged(new_index_text) = event {
            let buffer_id = diff.read(cx).buffer_id;
            if let Some(diff_state) = self.diffs.get(&buffer_id) {
                let new_index_text = new_index_text.as_ref().map(|rope| rope.to_string());
                if new_index_text.as_deref() == diff_state.read(cx).index_text.as_deref() {
                    return;
                }
                let hunk_staging_operation_count = diff_state.update(cx, |diff_state, _| {
                    diff_state.hunk_staging_operation_count += 1;
                    diff_state.hunk_staging_operation_count
                });
                if let Some((repo, path)) = self.repository_and_path_for_buffer_id(buffer_id, cx) {
                    let recv = repo.update(cx, |repo, cx| {
                        log::debug!("hunks changed for {}", path.as_unix_str());
                        repo.spawn_set_index_text_job(
                            path,
                            new_index_text,
                            Some(hunk_staging_operation_count),
                            cx,
                        )
                    });
                    let diff = diff.downgrade();
                    cx.spawn(async move |this, cx| {
                        if let Ok(Err(error)) = cx.background_spawn(recv).await {
                            diff.update(cx, |diff, cx| {
                                diff.clear_pending_hunks(cx);
                            })
                            .ok();
                            this.update(cx, |_, cx| cx.emit(GitStoreEvent::IndexWriteError(error)))
                                .ok();
                        }
                    })
                    .detach();
                }
            }
        }
    }

    fn local_worktree_git_repos_changed(
        &mut self,
        worktree: Entity<Worktree>,
        changed_repos: &UpdatedGitRepositoriesSet,
        cx: &mut Context<Self>,
    ) {
        log::debug!("local worktree repos changed");
        debug_assert!(worktree.read(cx).is_local());

        for repository in self.repositories.values() {
            repository.update(cx, |repository, cx| {
                let repo_abs_path = &repository.work_directory_abs_path;
                if changed_repos.iter().any(|update| {
                    update.old_work_directory_abs_path.as_ref() == Some(repo_abs_path)
                        || update.new_work_directory_abs_path.as_ref() == Some(repo_abs_path)
                }) {
                    repository.reload_buffer_diff_bases(cx);
                }
            });
        }
    }

    pub fn repositories(&self) -> &HashMap<RepositoryId, Entity<Repository>> {
        &self.repositories
    }

    /// Returns the main repository working directory for the given worktree.
    /// For normal checkouts this equals the worktree's own path. For linked
    /// worktrees it points back to the main worktree, if one exists. Linked
    /// worktrees attached to a bare repository have no main worktree path.
    pub fn original_repo_path_for_worktree(
        &self,
        worktree_id: WorktreeId,
        cx: &App,
    ) -> Option<Arc<Path>> {
        self.active_repo_id
            .iter()
            .chain(self.worktree_ids.keys())
            .find(|repo_id| {
                self.worktree_ids
                    .get(repo_id)
                    .is_some_and(|ids| ids.contains(&worktree_id))
            })
            .and_then(|repo_id| self.repositories.get(repo_id))
            .and_then(|repo| {
                repo.read(cx)
                    .snapshot()
                    .main_worktree_abs_path()
                    .map(Arc::from)
            })
    }

    pub fn status_for_buffer_id(&self, buffer_id: BufferId, cx: &App) -> Option<FileStatus> {
        let (repo, path) = self.repository_and_path_for_buffer_id(buffer_id, cx)?;
        let status = repo.read(cx).snapshot.status_for_path(&path)?;
        Some(status.status)
    }

    pub fn repository_and_path_for_buffer_id(
        &self,
        buffer_id: BufferId,
        cx: &App,
    ) -> Option<(Entity<Repository>, RepoPath)> {
        let buffer = self.buffer_store.read(cx).get(buffer_id)?;
        let project_path = buffer.read(cx).project_path(cx)?;
        self.repository_and_path_for_project_path(&project_path, cx)
    }

    pub fn repository_and_path_for_project_path(
        &self,
        path: &ProjectPath,
        cx: &App,
    ) -> Option<(Entity<Repository>, RepoPath)> {
        let abs_path = self.worktree_store.read(cx).absolutize(path, cx)?;
        self.repositories
            .values()
            .filter_map(|repo| {
                let repo_path = repo.read(cx).abs_path_to_repo_path(&abs_path)?;
                Some((repo.clone(), repo_path))
            })
            .max_by_key(|(repo, _)| repo.read(cx).work_directory_abs_path.clone())
    }

    pub fn git_init(
        &self,
        path: Arc<Path>,
        fallback_branch_name: String,
        cx: &App,
    ) -> Task<Result<()>> {
        match &self.state {
            GitStoreState::Local { fs, .. } => {
                let fs = fs.clone();
                cx.background_executor()
                    .spawn(async move { fs.git_init(&path, fallback_branch_name).await })
            }
            GitStoreState::Remote {
                upstream_client,
                upstream_project_id: project_id,
                ..
            } => {
                let client = upstream_client.clone();
                let project_id = *project_id;
                cx.background_executor().spawn(async move {
                    client
                        .request(proto::GitInit {
                            project_id: project_id,
                            abs_path: path.to_string_lossy().into_owned(),
                            fallback_branch_name,
                        })
                        .await?;
                    Ok(())
                })
            }
        }
    }

    pub fn git_clone(
        &self,
        repo: String,
        path: impl Into<Arc<std::path::Path>>,
        cx: &App,
    ) -> Task<Result<()>> {
        let path = path.into();
        match &self.state {
            GitStoreState::Local { fs, .. } => {
                let fs = fs.clone();
                cx.background_executor()
                    .spawn(async move { fs.git_clone(&path, &repo).await })
            }
            GitStoreState::Remote {
                upstream_client,
                upstream_project_id,
                ..
            } => {
                if upstream_client.is_via_collab() {
                    return Task::ready(Err(anyhow!(
                        "Git Clone isn't supported for project guests"
                    )));
                }
                let request = upstream_client.request(proto::GitClone {
                    project_id: *upstream_project_id,
                    abs_path: path.to_string_lossy().into_owned(),
                    remote_repo: repo,
                });

                cx.background_spawn(async move {
                    let result = request.await?;

                    match result.success {
                        true => Ok(()),
                        false => Err(anyhow!("Git Clone failed")),
                    }
                })
            }
        }
    }

    pub fn git_config(&self, path: Arc<Path>, args: Vec<String>, cx: &App) -> Task<Result<String>> {
        match &self.state {
            GitStoreState::Local { fs, .. } => {
                let fs = fs.clone();
                cx.background_executor()
                    .spawn(async move { fs.git_config(&path, args).await })
            }
            GitStoreState::Remote {
                upstream_client, ..
            } => {
                // Prevent running git config commands for collab.
                if upstream_client.is_via_collab() {
                    return Task::ready(Err(anyhow!(
                        "Git Config isn't support for project guests"
                    )));
                }

                // TODO: Implement this for remote repositories.
                Task::ready(Err(anyhow!(
                    "Git Config isn't yet supported for remote projects"
                )))
            }
        }
    }

    async fn handle_update_repository(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateRepository>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            let path_style = this.worktree_store.read(cx).path_style();
            let mut update = envelope.payload;

            let id = RepositoryId::from_proto(update.id);
            let client = this.upstream_client().context("no upstream client")?;

            let repository_dir_abs_path: Option<Arc<Path>> = update
                .repository_dir_abs_path
                .as_deref()
                .map(|p| Path::new(p).into());
            let common_dir_abs_path: Option<Arc<Path>> = update
                .common_dir_abs_path
                .as_deref()
                .map(|p| Path::new(p).into());

            let mut repo_subscription = None;
            let repo = this.repositories.entry(id).or_insert_with(|| {
                let git_store = cx.weak_entity();
                let repo = cx.new(|cx| {
                    Repository::remote(
                        id,
                        Path::new(&update.abs_path).into(),
                        repository_dir_abs_path.clone(),
                        common_dir_abs_path.clone(),
                        path_style,
                        ProjectId(update.project_id),
                        client,
                        git_store,
                        cx,
                    )
                });
                repo_subscription = Some(cx.subscribe(&repo, Self::on_repository_event));
                cx.emit(GitStoreEvent::RepositoryAdded);
                repo
            });
            this._subscriptions.extend(repo_subscription);

            repo.update(cx, {
                let update = update.clone();
                |repo, cx| repo.apply_remote_update(update, cx)
            })?;

            this.active_repo_id.get_or_insert_with(|| {
                cx.emit(GitStoreEvent::ActiveRepositoryChanged(Some(id)));
                id
            });

            if let Some((client, project_id)) = this.downstream_client() {
                update.project_id = project_id.to_proto();
                client.send(update).log_err();
            }
            Ok(())
        })
    }

    async fn handle_remove_repository(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RemoveRepository>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            let mut update = envelope.payload;
            let id = RepositoryId::from_proto(update.id);
            this.repositories.remove(&id);
            if let Some((client, project_id)) = this.downstream_client() {
                update.project_id = project_id.to_proto();
                client.send(update).log_err();
            }
            if this.active_repo_id == Some(id) {
                this.active_repo_id = None;
                cx.emit(GitStoreEvent::ActiveRepositoryChanged(None));
            }
            cx.emit(GitStoreEvent::RepositoryRemoved(id));
        });
        Ok(())
    }

    async fn handle_git_init(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitInit>,
        cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let path: Arc<Path> = PathBuf::from(envelope.payload.abs_path).into();
        let name = envelope.payload.fallback_branch_name;
        cx.update(|cx| this.read(cx).git_init(path, name, cx))
            .await?;

        Ok(proto::Ack {})
    }

    async fn handle_git_clone(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitClone>,
        cx: AsyncApp,
    ) -> Result<proto::GitCloneResponse> {
        let path: Arc<Path> = PathBuf::from(envelope.payload.abs_path).into();
        let repo_name = envelope.payload.remote_repo;
        let result = cx
            .update(|cx| this.read(cx).git_clone(repo_name, path, cx))
            .await;

        Ok(proto::GitCloneResponse {
            success: result.is_ok(),
        })
    }

    async fn handle_fetch(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Fetch>,
        mut cx: AsyncApp,
    ) -> Result<proto::RemoteMessageResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let fetch_options = FetchOptions::from_proto(envelope.payload.remote);
        let askpass_id = envelope.payload.askpass_id;

        let askpass = make_remote_delegate(
            this,
            envelope.payload.project_id,
            repository_id,
            askpass_id,
            &mut cx,
        );

        let remote_output = repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.fetch(fetch_options, askpass, cx)
            })
            .await??;

        Ok(proto::RemoteMessageResponse {
            stdout: remote_output.stdout,
            stderr: remote_output.stderr,
        })
    }

    async fn handle_push(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Push>,
        mut cx: AsyncApp,
    ) -> Result<proto::RemoteMessageResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let askpass_id = envelope.payload.askpass_id;
        let askpass = make_remote_delegate(
            this,
            envelope.payload.project_id,
            repository_id,
            askpass_id,
            &mut cx,
        );

        let options = envelope
            .payload
            .options
            .as_ref()
            .map(|_| match envelope.payload.options() {
                proto::push::PushOptions::SetUpstream => git::repository::PushOptions::SetUpstream,
                proto::push::PushOptions::Force => git::repository::PushOptions::Force,
            });

        let branch_name = envelope.payload.branch_name.into();
        let remote_branch_name = envelope.payload.remote_branch_name.into();
        let remote_name = envelope.payload.remote_name.into();

        let remote_output = repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.push(
                    branch_name,
                    remote_branch_name,
                    remote_name,
                    options,
                    askpass,
                    cx,
                )
            })
            .await??;
        Ok(proto::RemoteMessageResponse {
            stdout: remote_output.stdout,
            stderr: remote_output.stderr,
        })
    }

    async fn handle_pull(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Pull>,
        mut cx: AsyncApp,
    ) -> Result<proto::RemoteMessageResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let askpass_id = envelope.payload.askpass_id;
        let askpass = make_remote_delegate(
            this,
            envelope.payload.project_id,
            repository_id,
            askpass_id,
            &mut cx,
        );

        let branch_name = envelope.payload.branch_name.map(|name| name.into());
        let remote_name = envelope.payload.remote_name.into();
        let rebase = envelope.payload.rebase;

        let remote_message = repository_handle
            .update(&mut cx, |repository_handle, cx| {
                repository_handle.pull(branch_name, remote_name, rebase, askpass, cx)
            })
            .await??;

        Ok(proto::RemoteMessageResponse {
            stdout: remote_message.stdout,
            stderr: remote_message.stderr,
        })
    }

    async fn handle_get_commit_data(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetCommitData>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetCommitDataResponse> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let shas: Vec<Oid> = envelope
            .payload
            .shas
            .iter()
            .filter_map(|s| Oid::from_str(s).ok())
            .collect();

        let mut commits = Vec::with_capacity(shas.len());
        let mut receivers = Vec::new();

        repository_handle.update(&mut cx, |repository, cx| {
            for &sha in &shas {
                match repository.fetch_commit_data(sha, true, cx) {
                    CommitDataState::Loaded(data) => {
                        commits.push(commit_data_to_proto(data));
                    }
                    CommitDataState::Loading(Some(shared)) => {
                        receivers.push(shared.clone());
                    }
                    CommitDataState::Loading(None) => {
                        // todo(git_graph) this could happen if the request fails, we should encode an error case
                        debug_panic!(
                            "This should never happen since we passed true into fetch commit data"
                        );
                    }
                }
            }
        });

        let results = future::join_all(receivers).await;

        commits.extend(
            results
                .into_iter()
                .filter_map(|result| result.ok())
                .map(|data| commit_data_to_proto(&data)),
        );

        Ok(proto::GetCommitDataResponse { commits })
    }

    async fn handle_get_initial_graph_data(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetInitialGraphData>,
        mut cx: AsyncApp,
    ) -> Result<impl Stream<Item = Result<proto::GetInitialGraphDataResponse>>> {
        const CHUNK_SIZE: usize = git::repository::GRAPH_CHUNK_SIZE;
        let payload = envelope.payload;

        let repository_id = RepositoryId::from_proto(payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        let log_order = log_order_from_proto(payload.log_order());
        let log_source = log_source_from_proto(
            payload
                .log_source
                .context("missing initial graph data log source")?,
        )?;

        let (subscriber_sender, subscriber_receiver) = async_channel::unbounded();
        let (cached_commits, error, is_loading) =
            repository_handle.update(&mut cx, |repository, cx| {
                let response =
                    repository.graph_data(log_source.clone(), log_order, 0..usize::MAX, cx);
                let cached_commits = response.commits.to_vec();
                let error = response.error.clone();
                let is_loading = response.is_loading;

                if is_loading {
                    if let Some(graph_data) = repository
                        .initial_graph_data
                        .get_mut(&(log_source.clone(), log_order))
                    {
                        graph_data.subscribers.push(subscriber_sender);
                    }
                }

                (cached_commits, error, is_loading)
            });

        let (mut response_tx, response_rx) = mpsc::unbounded();
        cx.background_spawn(async move {
            if let Some(error) = error {
                if response_tx
                    .send(Err(anyhow!(error.to_string())))
                    .await
                    .is_err()
                {
                    return;
                }
                return;
            }

            for commits in cached_commits.chunks(CHUNK_SIZE) {
                let response = proto::GetInitialGraphDataResponse {
                    commits: commits
                        .iter()
                        .map(|commit| initial_graph_commit_to_proto(commit))
                        .collect(),
                };
                if response_tx.send(Ok(response)).await.is_err() {
                    return;
                }
            }

            if !is_loading {
                return;
            }

            while let Ok(chunk_result) = subscriber_receiver.recv().await {
                let commits = match chunk_result {
                    Ok(commits) => commits,
                    Err(error) => {
                        response_tx
                            .send(Err(anyhow!(error.to_string())))
                            .await
                            .context("Failed to send error")
                            .log_err();
                        return;
                    }
                };

                for commits in commits.chunks(CHUNK_SIZE) {
                    let response = proto::GetInitialGraphDataResponse {
                        commits: commits
                            .iter()
                            .map(|commit| initial_graph_commit_to_proto(commit))
                            .collect(),
                    };
                    if response_tx.send(Ok(response)).await.is_err() {
                        return;
                    }
                }
            }
        })
        .detach();

        Ok(response_rx)
    }

    async fn handle_search_commits(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::SearchCommits>,
        mut cx: AsyncApp,
    ) -> Result<impl Stream<Item = Result<proto::SearchCommitsResponse>>> {
        const CHUNK_SIZE: usize = 100;

        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let log_source = log_source_from_proto(
            envelope
                .payload
                .log_source
                .context("missing search commit log source")?,
        )?;
        let search_args = SearchCommitArgs {
            query: SharedString::from(envelope.payload.query),
            case_sensitive: envelope.payload.case_sensitive,
        };

        let (request_tx, request_rx) = async_channel::unbounded();
        repository_handle.update(&mut cx, |repository, cx| {
            repository.search_commits(log_source, search_args, request_tx, cx);
        });

        let (mut response_tx, response_rx) = mpsc::unbounded();
        cx.background_spawn(async move {
            let mut shas = Vec::new();

            while let Ok(sha) = request_rx.recv().await {
                shas.push(sha.to_string());

                if shas.len() >= CHUNK_SIZE {
                    if response_tx
                        .send(Ok(proto::SearchCommitsResponse {
                            shas: mem::take(&mut shas),
                        }))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }

            if !shas.is_empty() {
                response_tx
                    .send(Ok(proto::SearchCommitsResponse { shas }))
                    .await
                    .ok();
            }
        })
        .detach();

        Ok(response_rx)
    }

    async fn handle_edit_ref(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitEditRef>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;
        let ref_name = envelope.payload.ref_name;
        let commit = match envelope.payload.action {
            Some(proto::git_edit_ref::Action::UpdateToCommit(sha)) => Some(sha),
            Some(proto::git_edit_ref::Action::Delete(_)) => None,
            None => anyhow::bail!("GitEditRef missing action"),
        };

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.edit_ref(ref_name, commit)
            })
            .await??;

        Ok(proto::Ack {})
    }

    async fn handle_repair_worktrees(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GitRepairWorktrees>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let repository_id = RepositoryId::from_proto(envelope.payload.repository_id);
        let repository_handle = Self::repository_for_request(&this, repository_id, &mut cx)?;

        repository_handle
            .update(&mut cx, |repository_handle, _| {
                repository_handle.repair_worktrees()
            })
            .await??;

        Ok(proto::Ack {})
    }

    pub fn repo_snapshots(&self, cx: &App) -> HashMap<RepositoryId, RepositorySnapshot> {
        self.repositories
            .iter()
            .map(|(id, repo)| (*id, repo.read(cx).snapshot.clone()))
            .collect()
    }

    fn coalesce_repo_paths(mut paths: Vec<RepoPath>) -> Vec<RepoPath> {
        paths.sort();

        let mut coalesced = Vec::with_capacity(paths.len());
        for path in paths {
            if coalesced
                .last()
                .is_some_and(|ancestor: &RepoPath| path.starts_with(ancestor))
            {
                continue;
            }
            coalesced.push(path);
        }

        coalesced
    }

    fn process_updated_entries(
        &self,
        worktree: &Entity<Worktree>,
        updated_entries: &[(Arc<RelPath>, ProjectEntryId, PathChange)],
        cx: &mut App,
    ) -> Task<HashMap<Entity<Repository>, Vec<RepoPath>>> {
        let path_style = worktree.read(cx).path_style();
        let mut repo_paths = self
            .repositories
            .values()
            .map(|repo| (repo.read(cx).work_directory_abs_path.clone(), repo.clone()))
            .collect::<Vec<_>>();
        let mut entries: Vec<_> = updated_entries
            .iter()
            .map(|(path, _, _)| path.clone())
            .collect();
        entries.sort();
        let worktree = worktree.read(cx);

        let entries = entries
            .into_iter()
            .map(|path| worktree.absolutize(&path))
            .collect::<Arc<[_]>>();

        let executor = cx.background_executor().clone();
        cx.background_executor().spawn(async move {
            repo_paths.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
            let mut paths_by_git_repo = HashMap::<_, Vec<_>>::default();
            let mut tasks = FuturesOrdered::new();
            for (repo_path, repo) in repo_paths.into_iter().rev() {
                let entries = entries.clone();
                let task = executor.spawn(async move {
                    // Find all repository paths that belong to this repo
                    let mut ix = entries.partition_point(|path| path < &*repo_path);
                    if ix == entries.len() {
                        return None;
                    };

                    let mut paths = Vec::new();
                    // All paths prefixed by a given repo will constitute a continuous range.
                    while let Some(path) = entries.get(ix)
                        && let Some(repo_path) = RepositorySnapshot::abs_path_to_repo_path_inner(
                            &repo_path, path, path_style,
                        )
                    {
                        paths.push((repo_path, ix));
                        ix += 1;
                    }
                    if paths.is_empty() {
                        None
                    } else {
                        Some((repo, paths))
                    }
                });
                tasks.push_back(task);
            }

            // Now, let's filter out the "duplicate" entries that were processed by multiple distinct repos.
            let mut path_was_used = vec![false; entries.len()];
            let tasks = tasks.collect::<Vec<_>>().await;
            // Process tasks from the back: iterating backwards allows us to see more-specific paths first.
            // We always want to assign a path to it's innermost repository.
            for t in tasks {
                let Some((repo, paths)) = t else {
                    continue;
                };
                let entry = paths_by_git_repo.entry(repo).or_default();
                for (repo_path, ix) in paths {
                    if path_was_used[ix] {
                        continue;
                    }
                    path_was_used[ix] = true;
                    entry.push(repo_path);
                }
            }

            for paths in paths_by_git_repo.values_mut() {
                *paths = Self::coalesce_repo_paths(mem::take(paths));
            }

            paths_by_git_repo
        })
    }
}

impl BufferGitState {
    fn new(_git_store: WeakEntity<GitStore>, _cx: &mut Context<Self>) -> Self {
        Self {
            unstaged_diff: Default::default(),
            staged_diff: Default::default(),
            uncommitted_diff: Default::default(),
            oid_diffs: Default::default(),
            recalculate_diff_task: Default::default(),
            language: Default::default(),
            language_registry: Default::default(),
            recalculating_tx: postage::watch::channel_with(false).0,
            hunk_staging_operation_count: 0,
            hunk_staging_operation_count_as_of_write: 0,
            head_text: Default::default(),
            index_text: Default::default(),
            oid_texts: Default::default(),
            head_text_buffer: WeakEntity::new_invalid(),
            index_text_buffer: WeakEntity::new_invalid(),
            index_text_buffer_language_enabled: Default::default(),
            head_changed: Default::default(),
            index_changed: Default::default(),
            language_changed: Default::default(),
            conflict_updated_futures: Default::default(),
            conflict_set: Default::default(),
            reparse_conflict_markers_task: Default::default(),
        }
    }

    fn get_or_create_head_text_buffer(&mut self, cx: &mut Context<Self>) -> Entity<Buffer> {
        if let Some(buffer) = self.head_text_buffer.upgrade() {
            return buffer;
        }
        let head_text = self.head_text.clone();
        let buffer = cx.new(|cx| {
            let mut buffer = Buffer::local(head_text.as_deref().unwrap_or(""), cx);
            buffer.set_capability(Capability::ReadOnly, cx);
            buffer
        });
        self.head_text_buffer = buffer.downgrade();
        buffer
    }

    fn get_or_create_index_text_buffer(&mut self, cx: &mut Context<Self>) -> Entity<Buffer> {
        if let Some(buffer) = self.index_text_buffer.upgrade() {
            return buffer;
        }
        let index_text = self.index_text.clone();
        let buffer = cx.new(|cx| {
            let mut buffer = Buffer::local(index_text.as_deref().unwrap_or(""), cx);
            buffer.set_capability(Capability::ReadOnly, cx);
            buffer
        });
        self.index_text_buffer = buffer.downgrade();
        buffer
    }

    #[ztracing::instrument(skip_all)]
    fn buffer_language_changed(&mut self, buffer: Entity<Buffer>, cx: &mut Context<Self>) {
        self.language = buffer.read(cx).language().cloned();
        self.language_changed = true;
        let _ = self.recalculate_diffs(buffer.read(cx).text_snapshot(), cx);
    }

    fn reparse_conflict_markers(
        &mut self,
        buffer: text::BufferSnapshot,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();

        let Some(conflict_set) = self
            .conflict_set
            .as_ref()
            .and_then(|conflict_set| conflict_set.upgrade())
        else {
            return rx;
        };

        let has_conflict = conflict_set.read_with(cx, |conflict_set, _| conflict_set.has_conflict);
        if !has_conflict {
            return rx;
        }

        let old_snapshot = conflict_set.read_with(cx, |conflict_set, _| conflict_set.snapshot());
        self.conflict_updated_futures.push(tx);
        self.reparse_conflict_markers_task = Some(cx.spawn(async move |this, cx| {
            let (snapshot, changed_range) = cx
                .background_spawn(async move {
                    let new_snapshot = ConflictSet::parse(&buffer);
                    let changed_range = old_snapshot.compare(&new_snapshot, &buffer);
                    (new_snapshot, changed_range)
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some(conflict_set) = &this.conflict_set {
                    conflict_set
                        .update(cx, |conflict_set, cx| {
                            conflict_set.set_snapshot(snapshot, changed_range, cx);
                        })
                        .ok();
                }
                let futures = std::mem::take(&mut this.conflict_updated_futures);
                for tx in futures {
                    tx.send(()).ok();
                }
            })
        }));

        rx
    }

    fn unstaged_diff(&self) -> Option<Entity<BufferDiff>> {
        self.unstaged_diff.as_ref().and_then(|set| set.upgrade())
    }

    fn staged_diff(&self) -> Option<Entity<BufferDiff>> {
        self.staged_diff.as_ref().and_then(|(set, _)| set.upgrade())
    }

    fn uncommitted_diff(&self) -> Option<Entity<BufferDiff>> {
        self.uncommitted_diff.as_ref().and_then(|set| set.upgrade())
    }

    fn oid_diff(&self, oid: Option<git::Oid>) -> Option<Entity<BufferDiff>> {
        self.oid_diffs.get(&oid).and_then(|weak| weak.upgrade())
    }

    /// Whether the index text is known to match the committed text, without
    /// comparing their contents. Always true when both texts were set by a
    /// single `DiffBasesChange::SetBoth`, which shares one allocation between
    /// them. May be false even when the contents are equal, if the texts were
    /// loaded separately.
    fn index_matches_head(&self) -> bool {
        match (self.index_text.as_ref(), self.head_text.as_ref()) {
            (Some(index), Some(head)) => Arc::ptr_eq(index, head),
            (None, None) => true,
            _ => false,
        }
    }

    fn handle_base_texts_updated(
        &mut self,
        buffer: text::BufferSnapshot,
        message: proto::UpdateDiffBases,
        cx: &mut Context<Self>,
    ) {
        use proto::update_diff_bases::Mode;

        let Some(mode) = Mode::from_i32(message.mode) else {
            return;
        };

        let diff_bases_change = match mode {
            Mode::HeadOnly => DiffBasesChange::SetHead(message.committed_text),
            Mode::IndexOnly => DiffBasesChange::SetIndex(message.staged_text),
            Mode::IndexMatchesHead => DiffBasesChange::SetBoth(message.committed_text),
            Mode::IndexAndHead => DiffBasesChange::SetEach {
                index: message.staged_text,
                head: message.committed_text,
            },
        };

        self.diff_bases_changed(buffer, Some(diff_bases_change), cx);
    }

    pub fn wait_for_recalculation(&mut self) -> Option<impl Future<Output = ()> + use<>> {
        if *self.recalculating_tx.borrow() {
            let mut rx = self.recalculating_tx.subscribe();
            Some(async move {
                loop {
                    let is_recalculating = rx.recv().await;
                    if is_recalculating != Some(true) {
                        break;
                    }
                }
            })
        } else {
            None
        }
    }

    fn diff_bases_changed(
        &mut self,
        buffer: text::BufferSnapshot,
        diff_bases_change: Option<DiffBasesChange>,
        cx: &mut Context<Self>,
    ) {
        match diff_bases_change {
            Some(DiffBasesChange::SetIndex(index)) => {
                self.index_text = index.map(|mut index| {
                    text::LineEnding::normalize(&mut index);
                    Arc::from(index.as_str())
                });
                self.index_changed = true;
            }
            Some(DiffBasesChange::SetHead(head)) => {
                self.head_text = head.map(|mut head| {
                    text::LineEnding::normalize(&mut head);
                    Arc::from(head.as_str())
                });
                self.head_changed = true;
            }
            Some(DiffBasesChange::SetBoth(text)) => {
                let text = text.map(|mut text| {
                    text::LineEnding::normalize(&mut text);
                    Arc::from(text.as_str())
                });
                self.head_text = text.clone();
                self.index_text = text;
                self.head_changed = true;
                self.index_changed = true;
            }
            Some(DiffBasesChange::SetEach { index, head }) => {
                self.index_text = index.map(|mut index| {
                    text::LineEnding::normalize(&mut index);
                    Arc::from(index.as_str())
                });
                self.index_changed = true;
                self.head_text = head.map(|mut head| {
                    text::LineEnding::normalize(&mut head);
                    Arc::from(head.as_str())
                });
                self.head_changed = true;
            }
            None => {}
        }

        self.recalculate_diffs(buffer, cx)
    }
}

fn make_remote_delegate(
    this: Entity<GitStore>,
    project_id: u64,
    repository_id: RepositoryId,
    askpass_id: u64,
    cx: &mut AsyncApp,
) -> AskPassDelegate {
    AskPassDelegate::new(cx, move |prompt, tx, cx| {
        this.update(cx, |this, cx| {
            let Some((client, _)) = this.downstream_client() else {
                return;
            };
            let response = client.request(proto::AskPassRequest {
                project_id,
                repository_id: repository_id.to_proto(),
                askpass_id,
                prompt,
            });
            cx.spawn(async move |_, _| {
                let mut response = response.await?.response;
                tx.send(EncryptedPassword::try_from(response.as_ref())?)
                    .ok();
                response.zeroize();
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        });
    })
}

impl RepositoryId {
    pub fn to_proto(self) -> u64 {
        self.0
    }

    pub fn from_proto(id: u64) -> Self {
        RepositoryId(id)
    }
}

impl RepositorySnapshot {
    fn empty(
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

    fn initial_update(&self, project_id: u64) -> proto::UpdateRepository {
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

    fn build_update(&self, old: &Self, project_id: u64) -> proto::UpdateRepository {
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

    fn repo_path_to_abs_path(&self, repo_path: &RepoPath) -> PathBuf {
        let repo_path = repo_path.display(self.path_style);
        PathBuf::from(
            self.path_style
                .join(&self.work_directory_abs_path, repo_path.as_ref())
                .unwrap(),
        )
    }

    #[inline]
    fn abs_path_to_repo_path_inner(
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

pub fn stash_to_proto(entry: &StashEntry) -> proto::StashEntry {
    proto::StashEntry {
        oid: entry.oid.as_bytes().to_vec(),
        message: entry.message.clone(),
        branch: entry.branch.clone(),
        index: entry.index as u64,
        timestamp: entry.timestamp,
    }
}

pub fn proto_to_stash(entry: &proto::StashEntry) -> Result<StashEntry> {
    Ok(StashEntry {
        oid: Oid::from_bytes(&entry.oid)?,
        message: entry.message.clone(),
        index: entry.index as usize,
        branch: entry.branch.clone(),
        timestamp: entry.timestamp,
    })
}

impl MergeDetails {
    async fn update(
        &mut self,
        backend: &Arc<dyn GitRepository>,
        current_conflicted_paths: Vec<RepoPath>,
    ) -> bool {
        log::debug!("load merge details");
        self.message = backend.merge_message().await.map(SharedString::from);
        let heads = backend
            .revparse_batch(vec![
                "MERGE_HEAD".into(),
                "CHERRY_PICK_HEAD".into(),
                "REBASE_HEAD".into(),
                "REVERT_HEAD".into(),
                "APPLY_HEAD".into(),
            ])
            .await
            .log_err()
            .unwrap_or_default()
            .into_iter()
            .map(|opt| opt.map(SharedString::from))
            .collect::<Vec<_>>();

        let mut conflicts_changed = false;

        // Record the merge state for newly conflicted paths
        for path in &current_conflicted_paths {
            if self.merge_heads_by_conflicted_path.get(&path).is_none() {
                conflicts_changed = true;
                self.merge_heads_by_conflicted_path
                    .insert(path.clone(), heads.clone());
            }
        }

        // Clear state for paths that are no longer conflicted and for which the merge heads have changed
        self.merge_heads_by_conflicted_path
            .retain(|path, old_merge_heads| {
                let keep = current_conflicted_paths.contains(path)
                    || (old_merge_heads == &heads
                        && old_merge_heads.iter().any(|head| head.is_some()));
                if !keep {
                    conflicts_changed = true;
                }
                keep
            });

        conflicts_changed
    }
}

impl Repository {
    fn buffer_store(&self, cx: &App) -> Option<Entity<BufferStore>> {
        Some(self.git_store.upgrade()?.read(cx).buffer_store.clone())
    }

    fn save_buffers<'a>(
        &self,
        entries: impl IntoIterator<Item = &'a RepoPath>,
        cx: &mut Context<Self>,
    ) -> Vec<Task<anyhow::Result<()>>> {
        let mut save_futures = Vec::new();
        if let Some(buffer_store) = self.buffer_store(cx) {
            buffer_store.update(cx, |buffer_store, cx| {
                for path in entries {
                    let Some(project_path) = self.repo_path_to_project_path(path, cx) else {
                        continue;
                    };
                    if let Some(buffer) = buffer_store.get_by_path(&project_path)
                        && buffer
                            .read(cx)
                            .file()
                            .is_some_and(|file| file.disk_state().exists())
                        && buffer.read(cx).has_unsaved_edits()
                    {
                        save_futures.push(buffer_store.save_buffer(buffer, cx));
                    }
                }
            })
        }
        save_futures
    }

    pub fn stage_entries(
        &mut self,
        entries: Vec<RepoPath>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.stage_or_unstage_entries(true, entries, cx)
    }

    pub fn unstage_entries(
        &mut self,
        entries: Vec<RepoPath>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        self.stage_or_unstage_entries(false, entries, cx)
    }

    fn stage_or_unstage_entries(
        &mut self,
        stage: bool,
        entries: Vec<RepoPath>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        if entries.is_empty() {
            return Task::ready(Ok(()));
        }
        let Some(git_store) = self.git_store.upgrade() else {
            return Task::ready(Ok(()));
        };
        let id = self.id;
        let save_tasks = self.save_buffers(&entries, cx);
        let paths = entries
            .iter()
            .map(|p| p.as_unix_str())
            .collect::<Vec<_>>()
            .join(" ");
        let status = if stage {
            format!("git add {paths}")
        } else {
            format!("git reset {paths}")
        };
        let job_key = GitJobKey::WriteIndex(entries.clone());

        self.spawn_job_with_tracking(
            entries.clone(),
            if stage {
                pending_op::GitStatus::Staged
            } else {
                pending_op::GitStatus::Unstaged
            },
            cx,
            async move |this, cx| {
                for save_task in save_tasks {
                    save_task.await?;
                }

                this.update(cx, |this, cx| {
                    let weak_this = cx.weak_entity();
                    this.send_keyed_job(
                        "stage_or_unstage_entries",
                        Some(job_key),
                        Some(status.into()),
                        move |git_repo, mut cx| async move {
                            let hunk_staging_operation_counts = weak_this
                                .update(&mut cx, |this, cx| {
                                    let mut hunk_staging_operation_counts = HashMap::default();
                                    for path in &entries {
                                        let Some(project_path) =
                                            this.repo_path_to_project_path(path, cx)
                                        else {
                                            continue;
                                        };
                                        let Some(buffer) = git_store
                                            .read(cx)
                                            .buffer_store
                                            .read(cx)
                                            .get_by_path(&project_path)
                                        else {
                                            continue;
                                        };
                                        let Some(diff_state) = git_store
                                            .read(cx)
                                            .diffs
                                            .get(&buffer.read(cx).remote_id())
                                            .cloned()
                                        else {
                                            continue;
                                        };
                                        let Some(uncommitted_diff) =
                                            diff_state.read(cx).uncommitted_diff.as_ref().and_then(
                                                |uncommitted_diff| uncommitted_diff.upgrade(),
                                            )
                                        else {
                                            continue;
                                        };
                                        let buffer_snapshot = buffer.read(cx).text_snapshot();
                                        let file_exists = buffer
                                            .read(cx)
                                            .file()
                                            .is_some_and(|file| file.disk_state().exists());
                                        let hunk_staging_operation_count =
                                            diff_state.update(cx, |diff_state, cx| {
                                                uncommitted_diff.update(
                                                    cx,
                                                    |uncommitted_diff, cx| {
                                                        uncommitted_diff
                                                            .stage_or_unstage_all_hunks(
                                                                stage,
                                                                &buffer_snapshot,
                                                                file_exists,
                                                                cx,
                                                            );
                                                    },
                                                );

                                                diff_state.hunk_staging_operation_count += 1;
                                                diff_state.hunk_staging_operation_count
                                            });
                                        hunk_staging_operation_counts.insert(
                                            diff_state.downgrade(),
                                            hunk_staging_operation_count,
                                        );
                                    }
                                    hunk_staging_operation_counts
                                })
                                .unwrap_or_default();

                            let result = match git_repo {
                                RepositoryState::Local(LocalRepositoryState {
                                    backend,
                                    environment,
                                    ..
                                }) => {
                                    if stage {
                                        backend.stage_paths(entries, environment.clone()).await
                                    } else {
                                        backend.unstage_paths(entries, environment.clone()).await
                                    }
                                }
                                RepositoryState::Remote(RemoteRepositoryState {
                                    project_id,
                                    client,
                                }) => {
                                    if stage {
                                        client
                                            .request(proto::Stage {
                                                project_id: project_id.0,
                                                repository_id: id.to_proto(),
                                                paths: entries
                                                    .into_iter()
                                                    .map(|repo_path| repo_path.to_proto())
                                                    .collect(),
                                            })
                                            .await
                                            .context("sending stage request")
                                            .map(|_| ())
                                    } else {
                                        client
                                            .request(proto::Unstage {
                                                project_id: project_id.0,
                                                repository_id: id.to_proto(),
                                                paths: entries
                                                    .into_iter()
                                                    .map(|repo_path| repo_path.to_proto())
                                                    .collect(),
                                            })
                                            .await
                                            .context("sending unstage request")
                                            .map(|_| ())
                                    }
                                }
                            };

                            for (diff_state, hunk_staging_operation_count) in
                                hunk_staging_operation_counts
                            {
                                diff_state
                                    .update(&mut cx, |diff_state, cx| {
                                        if result.is_ok() {
                                            diff_state.hunk_staging_operation_count_as_of_write =
                                                hunk_staging_operation_count;
                                        } else if let Some(uncommitted_diff) =
                                            &diff_state.uncommitted_diff
                                        {
                                            uncommitted_diff
                                                .update(cx, |uncommitted_diff, cx| {
                                                    uncommitted_diff.clear_pending_hunks(cx);
                                                })
                                                .ok();
                                        }
                                    })
                                    .ok();
                            }

                            result
                        },
                    )
                })?
                .await?
            },
        )
    }

    pub fn stage_all(&mut self, cx: &mut Context<Self>) -> Task<anyhow::Result<()>> {
        let snapshot = self.snapshot.clone();
        let pending_ops = self.pending_ops.clone();
        let to_stage = cx.background_spawn(async move {
            snapshot
                .status()
                .filter_map(|entry| {
                    if let Some(ops) = pending_ops
                        .get(&PathKey(entry.repo_path.as_ref().clone()), ())
                        .filter(|ops| !ops.last_op_errored())
                    {
                        if ops.staging() || ops.staged() {
                            None
                        } else {
                            Some(entry.repo_path)
                        }
                    } else if entry.status.staging().is_fully_staged() {
                        None
                    } else {
                        Some(entry.repo_path)
                    }
                })
                .collect()
        });

        cx.spawn(async move |this, cx| {
            let to_stage = to_stage.await;
            this.update(cx, |this, cx| {
                this.stage_or_unstage_entries(true, to_stage, cx)
            })?
            .await
        })
    }

    pub fn unstage_all(&mut self, cx: &mut Context<Self>) -> Task<anyhow::Result<()>> {
        let snapshot = self.snapshot.clone();
        let pending_ops = self.pending_ops.clone();
        let to_unstage = cx.background_spawn(async move {
            snapshot
                .status()
                .filter_map(|entry| {
                    if let Some(ops) = pending_ops
                        .get(&PathKey(entry.repo_path.as_ref().clone()), ())
                        .filter(|ops| !ops.last_op_errored())
                    {
                        if !ops.staging() && !ops.staged() {
                            None
                        } else {
                            Some(entry.repo_path)
                        }
                    } else if entry.status.staging().is_fully_unstaged() {
                        None
                    } else {
                        Some(entry.repo_path)
                    }
                })
                .collect()
        });

        cx.spawn(async move |this, cx| {
            let to_unstage = to_unstage.await;
            this.update(cx, |this, cx| {
                this.stage_or_unstage_entries(false, to_unstage, cx)
            })?
            .await
        })
    }

    fn spawn_set_index_text_job(
        &mut self,
        path: RepoPath,
        content: Option<String>,
        hunk_staging_operation_count: Option<usize>,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<anyhow::Result<()>> {
        let id = self.id;
        let this = cx.weak_entity();
        let git_store = self.git_store.clone();
        let abs_path = self.snapshot.repo_path_to_abs_path(&path);
        self.send_keyed_job(
            "spawn_set_index_text_job",
            Some(GitJobKey::WriteIndex(vec![path.clone()])),
            None,
            move |git_repo, mut cx| async move {
                log::debug!(
                    "start updating index text for buffer {}",
                    path.as_unix_str()
                );

                match git_repo {
                    RepositoryState::Local(LocalRepositoryState {
                        fs,
                        backend,
                        environment,
                        ..
                    }) => {
                        let executable = match fs.metadata(&abs_path).await {
                            Ok(Some(meta)) => meta.is_executable,
                            Ok(None) => false,
                            Err(_err) => false,
                        };
                        backend
                            .set_index_text(path.clone(), content, environment.clone(), executable)
                            .await?;
                    }
                    RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                        client
                            .request(proto::SetIndexText {
                                project_id: project_id.0,
                                repository_id: id.to_proto(),
                                path: path.to_proto(),
                                text: content,
                            })
                            .await?;
                    }
                }
                log::debug!(
                    "finish updating index text for buffer {}",
                    path.as_unix_str()
                );

                if let Some(hunk_staging_operation_count) = hunk_staging_operation_count {
                    let project_path = this
                        .read_with(&cx, |this, cx| this.repo_path_to_project_path(&path, cx))
                        .ok()
                        .flatten();
                    git_store
                        .update(&mut cx, |git_store, cx| {
                            let buffer_id = git_store
                                .buffer_store
                                .read(cx)
                                .get_by_path(&project_path?)?
                                .read(cx)
                                .remote_id();
                            let diff_state = git_store.diffs.get(&buffer_id)?;
                            diff_state.update(cx, |diff_state, _| {
                                diff_state.hunk_staging_operation_count_as_of_write =
                                    hunk_staging_operation_count;
                            });
                            Some(())
                        })
                        .context("Git store dropped")?;
                }
                Ok(())
            },
        )
    }

    pub fn checkpoint(&mut self) -> oneshot::Receiver<Result<GitRepositoryCheckpoint>> {
        let id = self.id;
        self.send_job("checkpoint", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.checkpoint().await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitCreateCheckpoint {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                        })
                        .await?;

                    Ok(GitRepositoryCheckpoint {
                        commit_sha: Oid::from_bytes(&response.commit_sha)?,
                    })
                }
            }
        })
    }

    pub fn restore_checkpoint(
        &mut self,
        checkpoint: GitRepositoryCheckpoint,
    ) -> oneshot::Receiver<Result<()>> {
        let id = self.id;
        self.send_job("restore_checkpoint", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.restore_checkpoint(checkpoint).await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    client
                        .request(proto::GitRestoreCheckpoint {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            commit_sha: checkpoint.commit_sha.as_bytes().to_vec(),
                        })
                        .await?;
                    Ok(())
                }
            }
        })
    }

    pub(crate) fn apply_remote_update(
        &mut self,
        update: proto::UpdateRepository,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if let Some(repository_dir_abs_path) = &update.repository_dir_abs_path {
            self.snapshot.repository_dir_abs_path =
                Path::new(repository_dir_abs_path.as_str()).into();
        }
        if let Some(common_dir_abs_path) = &update.common_dir_abs_path {
            self.snapshot.common_dir_abs_path = Path::new(common_dir_abs_path.as_str()).into();
        }

        let new_branch = update.branch_summary.as_ref().map(proto_to_branch);
        let new_head_commit = update
            .head_commit_details
            .as_ref()
            .map(proto_to_commit_details);
        if self.snapshot.branch != new_branch || self.snapshot.head_commit != new_head_commit {
            cx.emit(RepositoryEvent::HeadChanged)
        }
        self.snapshot.branch = new_branch;
        self.snapshot.head_commit = new_head_commit;

        if update.is_last_update {
            let new_branch_list: Arc<[Branch]> =
                update.branch_list.iter().map(proto_to_branch).collect();
            let new_branch_list_error = update.branch_list_error.map(SharedString::from);
            if *self.snapshot.branch_list != *new_branch_list
                || self.snapshot.branch_list_error != new_branch_list_error
            {
                cx.emit(RepositoryEvent::BranchListChanged);
            }
            self.snapshot.branch_list = new_branch_list;
            self.snapshot.branch_list_error = new_branch_list_error;
        }

        // We don't store any merge head state for downstream projects; the upstream
        // will track it and we will just get the updated conflicts
        let new_merge_heads = TreeMap::from_ordered_entries(
            update
                .current_merge_conflicts
                .into_iter()
                .filter_map(|path| Some((RepoPath::from_proto(&path).ok()?, vec![]))),
        );
        let conflicts_changed =
            self.snapshot.merge.merge_heads_by_conflicted_path != new_merge_heads;
        self.snapshot.merge.merge_heads_by_conflicted_path = new_merge_heads;
        self.snapshot.merge.message = update.merge_message.map(SharedString::from);
        let new_stash_entries = GitStash {
            entries: update
                .stash_entries
                .iter()
                .filter_map(|entry| proto_to_stash(entry).ok())
                .collect(),
        };
        if self.snapshot.stash_entries != new_stash_entries {
            cx.emit(RepositoryEvent::StashEntriesChanged)
        }
        self.snapshot.stash_entries = new_stash_entries;
        let new_linked_worktrees: Arc<[GitWorktree]> = update
            .linked_worktrees
            .iter()
            .map(proto_to_worktree)
            .collect();
        if *self.snapshot.linked_worktrees != *new_linked_worktrees {
            cx.emit(RepositoryEvent::GitWorktreeListChanged);
        }
        self.snapshot.linked_worktrees = new_linked_worktrees;
        self.snapshot.remote_upstream_url = update.remote_upstream_url;
        self.snapshot.remote_origin_url = update.remote_origin_url;

        let edits = update
            .removed_statuses
            .into_iter()
            .filter_map(|path| {
                Some(sum_tree::Edit::Remove(PathKey(
                    RelPath::from_proto(&path).log_err()?,
                )))
            })
            .chain(
                update
                    .updated_statuses
                    .into_iter()
                    .filter_map(|updated_status| {
                        Some(sum_tree::Edit::Insert(updated_status.try_into().log_err()?))
                    }),
            )
            .collect::<Vec<_>>();
        if conflicts_changed || !edits.is_empty() {
            cx.emit(RepositoryEvent::StatusesChanged);
        }
        self.snapshot.statuses_by_path.edit(edits, ());

        if update.is_last_update {
            self.snapshot.scan_id = update.scan_id;
        }
        self.clear_pending_ops(cx);
        Ok(())
    }

    pub fn compare_checkpoints(
        &mut self,
        left: GitRepositoryCheckpoint,
        right: GitRepositoryCheckpoint,
    ) -> oneshot::Receiver<Result<bool>> {
        let id = self.id;
        self.send_job("compare_checkpoints", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.compare_checkpoints(left, right).await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitCompareCheckpoints {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            left_commit_sha: left.commit_sha.as_bytes().to_vec(),
                            right_commit_sha: right.commit_sha.as_bytes().to_vec(),
                        })
                        .await?;
                    Ok(response.equal)
                }
            }
        })
    }

    pub fn diff_checkpoints(
        &mut self,
        base_checkpoint: GitRepositoryCheckpoint,
        target_checkpoint: GitRepositoryCheckpoint,
    ) -> oneshot::Receiver<Result<String>> {
        let id = self.id;
        self.send_job("diff_checkpoints", None, move |repo, _cx| async move {
            match repo {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend
                        .diff_checkpoints(base_checkpoint, target_checkpoint)
                        .await
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::GitDiffCheckpoints {
                            project_id: project_id.0,
                            repository_id: id.to_proto(),
                            base_commit_sha: base_checkpoint.commit_sha.as_bytes().to_vec(),
                            target_commit_sha: target_checkpoint.commit_sha.as_bytes().to_vec(),
                        })
                        .await?;
                    Ok(response.diff)
                }
            }
        })
    }

    fn clear_pending_ops(&mut self, cx: &mut Context<Self>) {
        let updated = SumTree::from_iter(
            self.pending_ops.iter().filter_map(|ops| {
                let inner_ops: Vec<PendingOp> =
                    ops.ops.iter().filter(|op| op.running()).cloned().collect();
                if inner_ops.is_empty() {
                    None
                } else {
                    Some(PendingOps {
                        repo_path: ops.repo_path.clone(),
                        ops: inner_ops,
                    })
                }
            }),
            (),
        );

        if updated != self.pending_ops {
            cx.emit(RepositoryEvent::PendingOpsChanged {
                pending_ops: self.pending_ops.clone(),
            })
        }

        self.pending_ops = updated;
    }

    fn schedule_scan(
        &mut self,
        updates_tx: Option<mpsc::UnboundedSender<DownstreamUpdate>>,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        let _ = self.send_keyed_job(
            "schedule_scan",
            Some(GitJobKey::ReloadGitState),
            None,
            |state, mut cx| async move {
                log::debug!("run scheduled git status scan");

                let Some(this) = this.upgrade() else {
                    return Ok(());
                };
                let RepositoryState::Local(LocalRepositoryState { backend, .. }) = state else {
                    bail!("not a local repository")
                };
                let snapshot = compute_snapshot(this.clone(), backend.clone(), &mut cx).await;
                this.update(&mut cx, |this, cx| {
                    this.clear_pending_ops(cx);
                });
                if let Some(updates_tx) = updates_tx {
                    updates_tx
                        .unbounded_send(DownstreamUpdate::UpdateRepository(snapshot))
                        .ok();
                }
                Ok(())
            },
        );
    }

    fn spawn_local_git_worker(
        state: Shared<Task<Result<LocalRepositoryState, String>>>,
        cx: &mut Context<Self>,
    ) -> (mpsc::UnboundedSender<GitJob>, Task<()>) {
        let (job_tx, mut job_rx) = mpsc::unbounded::<GitJob>();

        let worker_task = cx.spawn(async move |this, cx| {
            let Some(state) = state.await.log_err() else {
                return;
            };
            if let Some(git_hosting_provider_registry) =
                cx.update(|cx| GitHostingProviderRegistry::try_global(cx))
            {
                git_hosting_providers::register_additional_providers(
                    git_hosting_provider_registry,
                    state.backend.clone(),
                )
                .await;
            }
            let state = RepositoryState::Local(state);
            let mut jobs = VecDeque::new();
            loop {
                while let Ok(next_job) = job_rx.try_recv() {
                    jobs.push_back(next_job);
                }

                if let Some(job) = jobs.pop_front() {
                    if let Some(current_key) = &job.key
                        && jobs
                            .iter()
                            .any(|other_job| other_job.key.as_ref() == Some(current_key))
                    {
                        let skipped_job_id = job.id;
                        this.update(cx, |repo, _| {
                            repo.job_debug_queue.mark_complete(
                                skipped_job_id,
                                job_debug_queue::CompletedJobStatus::Skipped,
                            );
                        })
                        .ok();
                        continue;
                    }
                    (job.job)(state.clone(), cx).await;
                } else if let Some(job) = job_rx.next().await {
                    jobs.push_back(job);
                } else {
                    break;
                }
            }
        });

        (job_tx, worker_task)
    }

    fn spawn_remote_git_worker(
        state: RemoteRepositoryState,
        cx: &mut Context<Self>,
    ) -> (mpsc::UnboundedSender<GitJob>, Task<()>) {
        let (job_tx, mut job_rx) = mpsc::unbounded::<GitJob>();

        let worker_task = cx.spawn(async move |this, cx| {
            let result: Result<()> = async {
                let state = RepositoryState::Remote(state);
                let mut jobs = VecDeque::new();
                loop {
                    while let Ok(next_job) = job_rx.try_recv() {
                        jobs.push_back(next_job);
                    }

                    if let Some(job) = jobs.pop_front() {
                        if let Some(current_key) = &job.key
                            && jobs
                                .iter()
                                .any(|other_job| other_job.key.as_ref() == Some(current_key))
                        {
                            let skipped_job_id = job.id;
                            this.update(cx, |repo, _| {
                                repo.job_debug_queue.mark_complete(
                                    skipped_job_id,
                                    job_debug_queue::CompletedJobStatus::Skipped,
                                );
                            })
                            .ok();
                            continue;
                        }
                        (job.job)(state.clone(), cx).await;
                    } else if let Some(job) = job_rx.next().await {
                        jobs.push_back(job);
                    } else {
                        break;
                    }
                }
                anyhow::Ok(())
            }
            .await;
            result.log_err();
        });

        (job_tx, worker_task)
    }

    fn load_staged_text(
        &mut self,
        buffer_id: BufferId,
        repo_path: RepoPath,
        cx: &App,
    ) -> Task<Result<Option<String>>> {
        let rx = self.send_job("load_staged_text", None, move |state, _| async move {
            match state {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    anyhow::Ok(backend.load_index_text(repo_path).await)
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    let response = client
                        .request(proto::OpenUnstagedDiff {
                            project_id: project_id.to_proto(),
                            buffer_id: buffer_id.to_proto(),
                        })
                        .await?;
                    Ok(response.staged_text)
                }
            }
        });
        cx.spawn(|_: &mut AsyncApp| async move { rx.await? })
    }

    fn load_committed_text(
        &mut self,
        buffer_id: BufferId,
        repo_path: RepoPath,
        cx: &App,
    ) -> Task<Result<DiffBasesChange>> {
        let rx = self.send_job("load_committed_text", None, move |state, _| async move {
            match state {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    let committed_text = backend.load_committed_text(repo_path.clone()).await;
                    let staged_text = backend.load_index_text(repo_path).await;
                    let diff_bases_change = if committed_text == staged_text {
                        DiffBasesChange::SetBoth(committed_text)
                    } else {
                        DiffBasesChange::SetEach {
                            index: staged_text,
                            head: committed_text,
                        }
                    };
                    anyhow::Ok(diff_bases_change)
                }
                RepositoryState::Remote(RemoteRepositoryState { project_id, client }) => {
                    use proto::open_uncommitted_diff_response::Mode;

                    let response = client
                        .request(proto::OpenUncommittedDiff {
                            project_id: project_id.to_proto(),
                            buffer_id: buffer_id.to_proto(),
                        })
                        .await?;
                    let mode = Mode::from_i32(response.mode).context("Invalid mode")?;
                    let bases = match mode {
                        Mode::IndexMatchesHead => DiffBasesChange::SetBoth(response.committed_text),
                        Mode::IndexAndHead => DiffBasesChange::SetEach {
                            head: response.committed_text,
                            index: response.staged_text,
                        },
                    };
                    Ok(bases)
                }
            }
        });

        cx.spawn(|_: &mut AsyncApp| async move { rx.await? })
    }

    pub fn load_commit_template_text(
        &mut self,
    ) -> oneshot::Receiver<Result<Option<GitCommitTemplate>>> {
        self.send_job(
            "load_commit_template_text",
            None,
            move |git_repo, _cx| async move {
                match git_repo {
                    RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                        backend.load_commit_template().await
                    }
                    RepositoryState::Remote(_) => Ok(None),
                }
            },
        )
    }

    fn load_blob_content(&mut self, oid: Oid, cx: &App) -> Task<Result<String>> {
        let repository_id = self.snapshot.id;
        let rx = self.send_job("load_blob_content", None, move |state, _| async move {
            match state {
                RepositoryState::Local(LocalRepositoryState { backend, .. }) => {
                    backend.load_blob_content(oid).await
                }
                RepositoryState::Remote(RemoteRepositoryState { client, project_id }) => {
                    let response = client
                        .request(proto::GetBlobContent {
                            project_id: project_id.to_proto(),
                            repository_id: repository_id.0,
                            oid: oid.to_string(),
                        })
                        .await?;
                    Ok(response.content)
                }
            }
        });
        cx.spawn(|_: &mut AsyncApp| async move { rx.await? })
    }

    fn paths_changed(
        &mut self,
        paths: Vec<RepoPath>,
        updates_tx: Option<mpsc::UnboundedSender<DownstreamUpdate>>,
        cx: &mut Context<Self>,
    ) {
        if !paths.is_empty() {
            self.paths_needing_status_update.push(paths);
        }

        let this = cx.weak_entity();
        let _ = self.send_keyed_job(
            "paths_changed",
            Some(GitJobKey::RefreshStatuses),
            None,
            |state, mut cx| async move {
                let (prev_snapshot, changed_paths) = this.update(&mut cx, |this, _| {
                    (
                        this.snapshot.clone(),
                        mem::take(&mut this.paths_needing_status_update),
                    )
                })?;
                let RepositoryState::Local(LocalRepositoryState { backend, .. }) = state else {
                    bail!("not a local repository")
                };

                if changed_paths.is_empty() {
                    return Ok(());
                }

                let has_head = prev_snapshot.head_commit.is_some();

                let changed_path_statuses = cx
                    .background_spawn(async move {
                        let changed_paths = GitStore::coalesce_repo_paths(
                            changed_paths
                                .into_iter()
                                .flatten()
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .collect(),
                        );
                        let changed_paths_vec = changed_paths.iter().cloned().collect::<Vec<_>>();

                        let status_task = backend.status(&changed_paths_vec);
                        let diff_stat_future = if has_head {
                            backend.diff_stat(&changed_paths_vec)
                        } else {
                            future::ready(Ok(status::GitDiffStat {
                                entries: Arc::default(),
                            }))
                            .boxed()
                        };

                        let (statuses, diff_stats) =
                            futures::future::try_join(status_task, diff_stat_future).await?;

                        let diff_stats: HashMap<RepoPath, DiffStat> =
                            HashMap::from_iter(diff_stats.entries.into_iter().cloned());

                        let mut changed_path_statuses = Vec::new();
                        let prev_statuses = prev_snapshot.statuses_by_path.clone();
                        let current_status_paths = statuses
                            .entries
                            .iter()
                            .map(|(repo_path, _)| repo_path.clone())
                            .collect::<BTreeSet<_>>();

                        for path in &changed_paths {
                            let mut cursor = prev_statuses.cursor::<PathProgress>(());
                            cursor.seek_forward(&PathTarget::Path(path), Bias::Left);
                            while let Some(entry) = cursor.item() {
                                if !entry.repo_path.starts_with(path) {
                                    break;
                                }

                                if !current_status_paths.contains(&entry.repo_path) {
                                    changed_path_statuses.push(Edit::Remove(PathKey(
                                        entry.repo_path.as_ref().clone(),
                                    )));
                                }
                                cursor.next();
                            }
                        }

                        let mut cursor = prev_statuses.cursor::<PathProgress>(());

                        for (repo_path, status) in &*statuses.entries {
                            let current_diff_stat = diff_stats.get(repo_path).copied();

                            if cursor.seek_forward(&PathTarget::Path(repo_path), Bias::Left)
                                && cursor.item().is_some_and(|entry| {
                                    entry.status == *status && entry.diff_stat == current_diff_stat
                                })
                            {
                                continue;
                            }

                            changed_path_statuses.push(Edit::Insert(StatusEntry {
                                repo_path: repo_path.clone(),
                                status: *status,
                                diff_stat: current_diff_stat,
                            }));
                        }
                        anyhow::Ok(changed_path_statuses)
                    })
                    .await?;

                this.update(&mut cx, |this, cx| {
                    if !changed_path_statuses.is_empty() {
                        cx.emit(RepositoryEvent::StatusesChanged);
                        this.snapshot
                            .statuses_by_path
                            .edit(changed_path_statuses, ());
                        this.snapshot.scan_id += 1;
                    }

                    if let Some(updates_tx) = updates_tx {
                        updates_tx
                            .unbounded_send(DownstreamUpdate::UpdateRepository(
                                this.snapshot.clone(),
                            ))
                            .ok();
                    }
                })
            },
        );
    }

    /// currently running git command and when it started
    pub fn current_job(&self) -> Option<JobInfo> {
        self.active_jobs.values().next().cloned()
    }

    pub fn job_debug_queue(&self) -> &job_debug_queue::GitJobDebugQueue {
        &self.job_debug_queue
    }

    pub fn barrier(&mut self) -> oneshot::Receiver<()> {
        self.send_job("barrier", None, |_, _| async {})
    }

    fn spawn_job_with_tracking<AsyncFn>(
        &mut self,
        paths: Vec<RepoPath>,
        git_status: pending_op::GitStatus,
        cx: &mut Context<Self>,
        f: AsyncFn,
    ) -> Task<Result<()>>
    where
        AsyncFn: AsyncFnOnce(WeakEntity<Repository>, &mut AsyncApp) -> Result<()> + 'static,
    {
        let ids = self.new_pending_ops_for_paths(paths, git_status);

        cx.spawn(async move |this, cx| {
            let (job_status, result) = match f(this.clone(), cx).await {
                Ok(()) => (pending_op::JobStatus::Finished, Ok(())),
                Err(err) if err.is::<Canceled>() => (pending_op::JobStatus::Skipped, Ok(())),
                Err(err) => (pending_op::JobStatus::Error, Err(err)),
            };

            this.update(cx, |this, _| {
                let mut edits = Vec::with_capacity(ids.len());
                for (id, entry) in ids {
                    if let Some(mut ops) = this
                        .pending_ops
                        .get(&PathKey(entry.as_ref().clone()), ())
                        .cloned()
                    {
                        if let Some(op) = ops.op_by_id_mut(id) {
                            op.job_status = job_status;
                        }
                        edits.push(sum_tree::Edit::Insert(ops));
                    }
                }
                this.pending_ops.edit(edits, ());
            })?;

            result
        })
    }

    fn new_pending_ops_for_paths(
        &mut self,
        paths: Vec<RepoPath>,
        git_status: pending_op::GitStatus,
    ) -> Vec<(PendingOpId, RepoPath)> {
        let mut edits = Vec::with_capacity(paths.len());
        let mut ids = Vec::with_capacity(paths.len());
        for path in paths {
            let mut ops = self
                .pending_ops
                .get(&PathKey(path.as_ref().clone()), ())
                .cloned()
                .unwrap_or_else(|| PendingOps::new(&path));
            let id = ops.max_id() + 1;
            ops.ops.push(PendingOp {
                id,
                git_status,
                job_status: pending_op::JobStatus::Running,
            });
            edits.push(sum_tree::Edit::Insert(ops));
            ids.push((id, path));
        }
        self.pending_ops.edit(edits, ());
        ids
    }

    pub fn access(&mut self, _cx: &App) -> oneshot::Receiver<GitAccess> {
        self.send_job("access", None, move |git_repo, _cx| async move {
            match git_repo {
                // TODO: Correctly handle remote repositories, where the user
                // that's running the Mav remote may not own the `.git/`
                // directory. For now we just return `GitAccess::Yes` so that
                // remoting continues working as expected.
                RepositoryState::Remote(..) => GitAccess::Yes,
                RepositoryState::Local(state) => match state.backend.check_access().await {
                    Ok(_) => GitAccess::Yes,
                    Err(_) => GitAccess::No,
                },
            }
        })
    }

    pub fn default_remote_url(&self) -> Option<String> {
        self.remote_upstream_url
            .clone()
            .or(self.remote_origin_url.clone())
    }
}

fn format_job_key(key: &GitJobKey) -> SharedString {
    match key {
        GitJobKey::WriteIndex(paths) => {
            let paths_str: Vec<_> = paths
                .iter()
                .map(|p| {
                    let rel: &RelPath = p;
                    format!("{}", AsRef::<Path>::as_ref(rel).display())
                })
                .collect();
            format!("WriteIndex({})", paths_str.join(", ")).into()
        }
        GitJobKey::ReloadBufferDiffBases => "ReloadBufferDiffBases".into(),
        GitJobKey::RefreshStatuses => "RefreshStatuses".into(),
        GitJobKey::ReloadGitState => "ReloadGitState".into(),
    }
}

async fn append_pattern_to_ignore_file(
    fs: Arc<dyn Fs>,
    file_path: PathBuf,
    pattern: String,
) -> Result<()> {
    let existing_content = fs.load(&file_path).await.unwrap_or_default();

    if existing_content.lines().any(|line| line.trim() == pattern) {
        return Ok(());
    }

    let new_content = if existing_content.is_empty() {
        format!("{}\n", pattern)
    } else if existing_content.ends_with('\n') {
        format!("{}{}\n", existing_content, pattern)
    } else {
        format!("{}\n{}\n", existing_content, pattern)
    };

    fs.save(
        &file_path,
        &text::Rope::from(new_content.as_str()),
        text::LineEnding::Unix,
    )
    .await
}

#[cfg(any(test, feature = "test-support"))]
impl Repository {
    pub fn loaded_commit_data_for_test(&self) -> HashMap<Oid, CommitData> {
        self.commit_data
            .iter()
            .filter_map(|(sha, state)| match state {
                CommitDataState::Loaded(data) => Some((*sha, data.as_ref().clone())),
                CommitDataState::Loading(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Project;
    use fs::{FakeFs, Fs};
    use git::repository::{RepoPath, repo_path};
    use gpui::TestAppContext;
    use gpui::proptest::prelude::*;
    use rand::{SeedableRng, rngs::StdRng};
    use serde_json::json;
    use settings::SettingsStore;
    use std::path::{Path, PathBuf};

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });
    }

    #[gpui::test]
    async fn test_open_uncommitted_diff_skips_symlinks(cx: &mut TestAppContext) {
        use util::rel_path::rel_path;

        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            Path::new("/project"),
            json!({
                ".git": {},
                "target.txt": "rule one\nrule two\n",
            }),
        )
        .await;
        fs.insert_symlink("/project/agents.md", PathBuf::from("target.txt"))
            .await;

        fs.set_head_and_index_for_repo(
            Path::new("/project/.git"),
            &[
                // git stores the symlink's target path as the blob for `agents.md`
                ("agents.md", "target.txt".into()),
                ("target.txt", "rule one\n".into()),
            ],
        );

        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
        project
            .update(cx, |project, cx| project.git_scans_complete(cx))
            .await;

        let worktree_id = project.read_with(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        });

        // symlink file should not produce a base diff
        let symlink_buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree_id, rel_path("agents.md")), cx)
            })
            .await
            .unwrap();
        let symlink_diff = project
            .update(cx, |project, cx| {
                project.open_uncommitted_diff(symlink_buffer, cx)
            })
            .await
            .unwrap();
        symlink_diff.read_with(cx, |diff, _| {
            assert!(
                !diff.base_text_exists(),
                "symlinked buffer should not have a git diff base"
            );
        });

        // regular file should still produce a base diff
        let regular_buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree_id, rel_path("target.txt")), cx)
            })
            .await
            .unwrap();
        let regular_diff = project
            .update(cx, |project, cx| {
                project.open_uncommitted_diff(regular_buffer, cx)
            })
            .await
            .unwrap();
        regular_diff.read_with(cx, |diff, _| {
            assert!(
                diff.base_text_exists(),
                "regular file should have a git diff base"
            );
        });
    }

    #[gpui::test]
    async fn test_append_pattern_to_ignore_file_creates_and_deduplicates(cx: &mut TestAppContext) {
        let fs: Arc<dyn Fs> = FakeFs::new(cx.executor());
        let path = PathBuf::from("/root/.gitignore");

        // Appending to a non-existent file creates it with a trailing newline.
        super::append_pattern_to_ignore_file(fs.clone(), path.clone(), "build/".to_string())
            .await
            .unwrap();
        assert_eq!(fs.load(&path).await.unwrap(), "build/\n");

        // Appending the same pattern again is a no-op (deduplication).
        super::append_pattern_to_ignore_file(fs.clone(), path.clone(), "build/".to_string())
            .await
            .unwrap();
        assert_eq!(fs.load(&path).await.unwrap(), "build/\n");

        // Appending a distinct pattern adds it with a trailing newline.
        super::append_pattern_to_ignore_file(fs.clone(), path.clone(), "target/".to_string())
            .await
            .unwrap();
        assert_eq!(fs.load(&path).await.unwrap(), "build/\ntarget/\n");
    }

    #[gpui::test]
    async fn test_append_pattern_adds_newline_before_pattern_when_missing(cx: &mut TestAppContext) {
        let fs: Arc<dyn Fs> = FakeFs::new(cx.executor());
        let path = PathBuf::from("/root/.gitignore");

        // Pre-populate the file without a trailing newline.
        fs.save(&path, &text::Rope::from("*.log"), text::LineEnding::Unix)
            .await
            .unwrap();

        // The new pattern must be written on its own line.
        super::append_pattern_to_ignore_file(fs.clone(), path.clone(), "build/".to_string())
            .await
            .unwrap();
        assert_eq!(fs.load(&path).await.unwrap(), "*.log\nbuild/\n");
    }

    #[test]
    fn test_new_worktree_path_uses_posix_style_for_remote_paths() {
        let work_dir = Path::new("/home/user/dev/lsp-tests");
        let directory =
            worktrees_directory_for_repo(work_dir, "../worktrees", PathStyle::Posix).unwrap();
        let directory = PathStyle::Posix
            .join_path(&directory, "nimble-sky")
            .unwrap();
        let path = PathStyle::Posix.join_path(&directory, "lsp-tests").unwrap();

        assert_eq!(
            path,
            PathBuf::from("/home/user/dev/worktrees/lsp-tests/nimble-sky/lsp-tests")
        );
    }

    fn verify_invariants(repository: &Repository) -> anyhow::Result<()> {
        match &repository.commit_data_handler {
            CommitDataHandlerState::Open(handler) => {
                verify_loading_entries_are_pending(repository, handler)?;
                verify_await_result_loading_entries_have_completion_senders(repository, handler)?;
                verify_pending_requests_are_loading(repository, handler)?;
                verify_completion_senders_are_await_result_loading(repository, handler)?;
                verify_completion_senders_are_pending(handler)?;
                verify_non_await_result_loading_entries_have_no_completion_sender(
                    repository, handler,
                )?;
                verify_loaded_entries_are_not_pending(repository, handler)?;
                verify_loaded_entries_have_no_completion_sender(repository, handler)?;
            }
            CommitDataHandlerState::Closed => {
                verify_closed_handler_invariants(repository)?;
            }
        }

        Ok(())
    }

    fn verify_loading_entries_are_pending(
        repository: &Repository,
        handler: &CommitDataHandler,
    ) -> anyhow::Result<()> {
        for (sha, state) in &repository.commit_data {
            if matches!(state, CommitDataState::Loading(_)) {
                anyhow::ensure!(
                    handler.pending_requests.contains(sha),
                    "loading commit data for {sha} must be tracked in pending_requests"
                );
            }
        }

        Ok(())
    }

    fn verify_await_result_loading_entries_have_completion_senders(
        repository: &Repository,
        handler: &CommitDataHandler,
    ) -> anyhow::Result<()> {
        for (sha, state) in &repository.commit_data {
            if matches!(state, CommitDataState::Loading(Some(_))) {
                anyhow::ensure!(
                    handler.completion_senders.contains_key(sha),
                    "await-result loading commit data for {sha} must have a completion sender"
                );
            }
        }

        Ok(())
    }

    fn verify_pending_requests_are_loading(
        repository: &Repository,
        handler: &CommitDataHandler,
    ) -> anyhow::Result<()> {
        for sha in &handler.pending_requests {
            anyhow::ensure!(
                matches!(
                    repository.commit_data.get(sha),
                    Some(CommitDataState::Loading(_))
                ),
                "pending request for {sha} must correspond to loading commit data"
            );
        }

        Ok(())
    }

    fn verify_completion_senders_are_await_result_loading(
        repository: &Repository,
        handler: &CommitDataHandler,
    ) -> anyhow::Result<()> {
        for sha in handler.completion_senders.keys() {
            anyhow::ensure!(
                matches!(
                    repository.commit_data.get(sha),
                    Some(CommitDataState::Loading(Some(_)))
                ),
                "completion sender for {sha} must correspond to await-result loading commit data"
            );
        }

        Ok(())
    }

    fn verify_completion_senders_are_pending(handler: &CommitDataHandler) -> anyhow::Result<()> {
        for sha in handler.completion_senders.keys() {
            anyhow::ensure!(
                handler.pending_requests.contains(sha),
                "completion sender for {sha} must also be tracked as pending"
            );
        }

        Ok(())
    }

    fn verify_non_await_result_loading_entries_have_no_completion_sender(
        repository: &Repository,
        handler: &CommitDataHandler,
    ) -> anyhow::Result<()> {
        for (sha, state) in &repository.commit_data {
            if matches!(state, CommitDataState::Loading(None)) {
                anyhow::ensure!(
                    !handler.completion_senders.contains_key(sha),
                    "non-await-result loading commit data for {sha} must not have a completion sender"
                );
            }
        }

        Ok(())
    }

    fn verify_loaded_entries_are_not_pending(
        repository: &Repository,
        handler: &CommitDataHandler,
    ) -> anyhow::Result<()> {
        for (sha, state) in &repository.commit_data {
            if matches!(state, CommitDataState::Loaded(_)) {
                anyhow::ensure!(
                    !handler.pending_requests.contains(sha),
                    "loaded commit data for {sha} must not still be pending"
                );
            }
        }

        Ok(())
    }

    fn verify_loaded_entries_have_no_completion_sender(
        repository: &Repository,
        handler: &CommitDataHandler,
    ) -> anyhow::Result<()> {
        for (sha, state) in &repository.commit_data {
            if matches!(state, CommitDataState::Loaded(_)) {
                anyhow::ensure!(
                    !handler.completion_senders.contains_key(sha),
                    "loaded commit data for {sha} must not keep a completion sender"
                );
            }
        }

        Ok(())
    }

    fn verify_closed_handler_invariants(repository: &Repository) -> anyhow::Result<()> {
        for (sha, state) in &repository.commit_data {
            anyhow::ensure!(
                !matches!(state, CommitDataState::Loading(_)),
                "closed handler must not keep loading commit data for {sha}"
            );
        }

        Ok(())
    }

    #[gpui::property_test(config = ProptestConfig {
        cases: 20,
        ..Default::default()
    })]
    async fn test_commit_data_random_invariants(
        #[strategy = any::<u64>()] seed: u64,
        #[strategy = gpui::proptest::collection::vec(0usize..2000, 1..200)] commit_indexes: Vec<
            usize,
        >,
        #[strategy = gpui::proptest::collection::vec(any::<bool>(), 1..200)] await_results: Vec<
            bool,
        >,
        #[strategy = gpui::proptest::collection::vec(0usize..2000, 0..200)] failing_commit_indexes: Vec<
            usize,
        >,
        #[strategy = gpui::proptest::collection::vec(0usize..2000, 0..200)] missing_commit_indexes: Vec<
            usize,
        >,
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let mut rng = StdRng::seed_from_u64(seed);

        let commit_shas = (0..2000).map(|_| Oid::random(&mut rng)).collect::<Vec<_>>();
        let failing_shas = failing_commit_indexes
            .into_iter()
            .map(|index| commit_shas[index % commit_shas.len()])
            .collect::<HashSet<_>>();
        let missing_shas = missing_commit_indexes
            .into_iter()
            .map(|index| commit_shas[index % commit_shas.len()])
            .collect::<HashSet<_>>();
        let commit_data = commit_shas
            .iter()
            .filter(|sha| !missing_shas.contains(sha))
            .map(|sha| {
                (
                    CommitData {
                        sha: *sha,
                        parents: SmallVec::new(),
                        author_name: SharedString::from(format!("Author {sha}")),
                        author_email: SharedString::from(format!("{sha}@example.com")),
                        commit_timestamp: rng.random_range(0..10_000),
                        subject: SharedString::from(format!("Subject {sha}")),
                        message: SharedString::from(format!("Subject {sha}\n\nBody for {sha}")),
                    },
                    failing_shas.contains(sha),
                )
            })
            .collect::<Vec<_>>();
        let expected_loaded_shas = commit_indexes
            .iter()
            .map(|index| commit_shas[index % commit_shas.len()])
            .filter(|sha| !failing_shas.contains(sha) && !missing_shas.contains(sha))
            .collect::<HashSet<_>>();

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            Path::new("/project"),
            json!({
                ".git": {},
                "file.txt": "content",
            }),
        )
        .await;
        fs.set_commit_data(Path::new("/project/.git"), commit_data);

        let project = Project::test(fs.clone(), [Path::new("/project")], cx).await;
        project
            .update(cx, |project, cx| project.git_scans_complete(cx))
            .await;

        let repository = project.read_with(cx, |project, cx| {
            project
                .active_repository(cx)
                .expect("should have a repository")
        });

        cx.update(|cx| {
            cx.observe(&repository, |repo, cx| {
                verify_invariants(repo.read(cx))
                    .context("Invariant weren't held after a cx.notify")
                    .unwrap();
            })
        })
        .detach();

        let mut next_step = 0;
        while next_step < commit_indexes.len() {
            let remaining_steps = commit_indexes.len() - next_step;
            let chunk_size = rng.random_range(1..=remaining_steps.min(16));
            let chunk_end = next_step + chunk_size;

            for step in next_step..chunk_end {
                let sha = commit_shas[commit_indexes[step] % commit_shas.len()];
                let await_result = await_results[step % await_results.len()];

                repository.update(cx, |repository, cx| {
                    repository.fetch_commit_data(sha, await_result, cx);
                    verify_invariants(repository)
                        .with_context(|| {
                            format!(
                                "commit data invariant violation after step {} for sha {}",
                                step + 1,
                                sha,
                            )
                        })
                        .unwrap();
                });
            }

            cx.run_until_parked();
            repository.read_with(cx, |repository, _cx| {
                verify_invariants(repository)
                    .with_context(|| {
                        format!(
                            "commit data invariant violation after draining through step {}",
                            chunk_end,
                        )
                    })
                    .unwrap();
            });

            next_step = chunk_end;
        }

        cx.run_until_parked();
        repository.read_with(cx, |repository, _cx| {
            verify_invariants(repository)
                .with_context(|| "commit data invariant violation after final drain".to_string())
                .unwrap();

            let loaded_shas = repository
                .commit_data
                .iter()
                .filter_map(|(sha, state)| match state {
                    CommitDataState::Loaded(_) => Some(*sha),
                    CommitDataState::Loading(_) => None,
                })
                .collect::<HashSet<_>>();
            let missing_loaded_shas = expected_loaded_shas
                .difference(&loaded_shas)
                .copied()
                .collect::<Vec<_>>();
            let unexpected_loaded_shas = loaded_shas
                .difference(&expected_loaded_shas)
                .copied()
                .collect::<Vec<_>>();
            assert!(
                missing_loaded_shas.is_empty() && unexpected_loaded_shas.is_empty(),
                "loaded commit data SHAs after final drain did not match expectation. missing: {:?}, unexpected: {:?}",
                missing_loaded_shas,
                unexpected_loaded_shas,
            );
        });
    }

    fn repo_paths(paths: &[&str]) -> Vec<RepoPath> {
        paths.iter().map(repo_path).collect()
    }

    #[test]
    fn coalesce_repo_paths_keeps_root_only() {
        let coalesced = GitStore::coalesce_repo_paths(repo_paths(&["", "src", "src/lib.rs"]));

        assert_eq!(coalesced, repo_paths(&[""]));
    }

    #[test]
    fn coalesce_repo_paths_keeps_existing_ancestors() {
        let coalesced = GitStore::coalesce_repo_paths(repo_paths(&[
            "src",
            "src/lib.rs",
            "src/nested/file.rs",
            "tests/test.rs",
        ]));

        assert_eq!(coalesced, repo_paths(&["src", "tests/test.rs"]));
    }

    #[test]
    fn coalesce_repo_paths_does_not_invent_missing_parents() {
        let coalesced = GitStore::coalesce_repo_paths(repo_paths(&[
            "submodule/a.txt",
            "submodule/nested/b.txt",
            "top_level.rs",
        ]));

        assert_eq!(
            coalesced,
            repo_paths(&["submodule/a.txt", "submodule/nested/b.txt", "top_level.rs"])
        );
    }
}

/// This snapshot computes the repository state on the foreground thread while
/// running the git commands on the background thread. We update branch, head,
/// remotes, and worktrees first so the UI can react sooner, then compute file
/// state and emit those events immediately after.
async fn compute_snapshot(
    this: Entity<Repository>,
    backend: Arc<dyn GitRepository>,
    cx: &mut AsyncApp,
) -> RepositorySnapshot {
    log::debug!("starting compute snapshot");

    let (id, work_directory_abs_path, prev_snapshot) = this.update(cx, |this, _| {
        this.paths_needing_status_update.clear();
        (
            this.id,
            this.work_directory_abs_path.clone(),
            this.snapshot.clone(),
        )
    });

    let branches_future = {
        let backend = backend.clone();
        async move { backend.branches().await.log_err().unwrap_or_default() }
    };
    let head_commit_future = {
        let backend = backend.clone();
        async move { backend.show("HEAD".to_string()).await.ok() }
    };
    let worktrees_future = {
        let backend = backend.clone();
        async move { backend.worktrees().await.log_err().unwrap_or_default() }
    };
    let (branches, head_commit, all_worktrees) =
        futures::future::join3(branches_future, head_commit_future, worktrees_future).await;
    log::debug!("fetched branches, head commit, worktrees");

    let BranchesScanResult {
        branches,
        error: branch_list_error,
    } = branches;
    let branch = branches.iter().find(|branch| branch.is_head).cloned();
    let branch_list: Arc<[Branch]> = branches.into();

    let linked_worktrees: Arc<[GitWorktree]> = all_worktrees
        .into_iter()
        .filter(|wt| wt.path != *work_directory_abs_path)
        .collect();

    let mut remote_urls = backend.remote_urls().await;
    let remote_origin_url = remote_urls.remove("origin");
    let remote_upstream_url = remote_urls.remove("upstream");

    log::debug!("fetched remotes");

    let snapshot = this.update(cx, |this, cx| {
        let head_changed =
            branch != this.snapshot.branch || head_commit != this.snapshot.head_commit;
        let branch_list_changed = *branch_list != *this.snapshot.branch_list;
        let branch_list_error_changed = branch_list_error != this.snapshot.branch_list_error;
        let worktrees_changed = *linked_worktrees != *this.snapshot.linked_worktrees;

        this.snapshot = RepositorySnapshot {
            id,
            work_directory_abs_path,
            branch,
            branch_list: branch_list.clone(),
            branch_list_error,
            head_commit,
            remote_origin_url,
            remote_upstream_url,
            linked_worktrees,
            scan_id: prev_snapshot.scan_id + 1,
            ..prev_snapshot
        };

        if head_changed {
            cx.emit(RepositoryEvent::HeadChanged);
        }

        if branch_list_changed || branch_list_error_changed {
            cx.emit(RepositoryEvent::BranchListChanged);
        }

        if worktrees_changed {
            cx.emit(RepositoryEvent::GitWorktreeListChanged);
        }

        this.snapshot.clone()
    });

    let statuses_future = {
        let backend = backend.clone();
        async move {
            backend
                .status(&[RepoPath::from_rel_path(
                    &RelPath::new(".".as_ref(), PathStyle::local()).unwrap(),
                )])
                .await
                .log_err()
                .unwrap_or_default()
        }
    };
    let diff_stat_future = {
        let snapshot = snapshot.clone();
        let backend = backend.clone();
        async move {
            if snapshot.head_commit.is_some() {
                backend.diff_stat(&[]).await.log_err().unwrap_or_default()
            } else {
                Default::default()
            }
        }
    };
    let stash_entries_future = {
        let backend = backend.clone();
        async move { backend.stash_entries().await.log_err().unwrap_or_default() }
    };

    let (statuses, diff_stats, stash_entries) =
        futures::future::join3(statuses_future, diff_stat_future, stash_entries_future).await;
    log::debug!("fetched statuses, diff stats, stash entries");

    let diff_stat_map: HashMap<&RepoPath, DiffStat> =
        diff_stats.entries.iter().map(|(p, s)| (p, *s)).collect();
    let mut conflicted_paths = Vec::new();
    let statuses_by_path = SumTree::from_iter(
        statuses.entries.iter().map(|(repo_path, status)| {
            if status.is_conflicted() {
                conflicted_paths.push(repo_path.clone());
            }
            StatusEntry {
                repo_path: repo_path.clone(),
                status: *status,
                diff_stat: diff_stat_map.get(repo_path).copied(),
            }
        }),
        (),
    );

    let (merge_details, conflicts_changed) = cx
        .background_spawn({
            let backend = backend.clone();
            let mut merge_details = snapshot.merge.clone();
            async move {
                let conflicts_changed = merge_details.update(&backend, conflicted_paths).await;
                (merge_details, conflicts_changed)
            }
        })
        .await;
    log::debug!("new merge details: {merge_details:?}");

    this.update(cx, |this, cx| {
        if conflicts_changed || statuses_by_path != this.snapshot.statuses_by_path {
            cx.emit(RepositoryEvent::StatusesChanged);
        }
        if stash_entries != this.snapshot.stash_entries {
            cx.emit(RepositoryEvent::StashEntriesChanged);
        }

        this.snapshot.scan_id += 1;
        this.snapshot.merge = merge_details;
        this.snapshot.statuses_by_path = statuses_by_path;
        this.snapshot.stash_entries = stash_entries;

        this.snapshot.clone()
    })
}
