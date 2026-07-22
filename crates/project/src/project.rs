pub mod agent_registry_store;
pub mod agent_server_store;
pub mod bookmark_store;
pub mod buffer_store;
pub mod color_extractor;
pub mod connection_manager;
pub mod context_server_store;
pub mod debounced_delay;
pub mod debugger;
pub mod git_store;
pub mod image_store;
pub mod lsp_command;
pub mod lsp_store;
pub mod manifest_tree;
pub mod prettier_store;
pub mod project_search;
pub mod project_settings;
pub mod search;
pub mod task_inventory;
pub mod task_store;
pub mod telemetry_snapshot;
pub mod terminals;
pub mod toolchain_store;
pub mod trusted_worktrees;
pub mod worktree_store;

mod accessors;
#[path = "project/buffer_operations.rs"]
mod buffer_operations;
#[cfg(any(test, feature = "test-support"))]
#[path = "project/test_support.rs"]
#[path = "project/connection_state.rs"]
mod connection_state;
mod constructors;
mod entry_operations;
mod environment;
#[path = "project/event_handlers.rs"]
mod event_handlers;
mod inline_values;
#[path = "project/language_git_agent.rs"]
mod language_git_agent;
#[path = "project/language_toolchains.rs"]
mod language_toolchains;
#[path = "project/lsp_requests.rs"]
mod lsp_requests;
#[path = "project/path_access.rs"]
mod path_access;
mod peer_handlers;
mod project_completion;
mod project_lsp_types;
mod project_support_types;
mod project_types;
mod remote_handlers;
#[path = "project/remote_sync.rs"]
mod remote_sync;
#[path = "project/search_worktrees.rs"]
mod search_worktrees;
mod test_support;
use buffer_diff::BufferDiff;
use context_server_store::ContextServerStore;
pub use environment::ProjectEnvironmentEvent;
use git::repository::get_git_committer;
use git_store::{Repository, RepositoryId};
pub use project_lsp_types::{
    CodeAction, Completion, CompletionDisplayOptions, CompletionGroup, CompletionIntent,
    CompletionResponse, CompletionSource, DocumentHighlight, DocumentSymbol, Hover, HoverBlock,
    HoverBlockKind, InlayHint, InlayHintLabel, InlayHintLabelPart, InlayHintLabelPartTooltip,
    InlayHintTooltip, InlayId, LocationLink, LspAction, MarkupContent, PrepareRenameResponse,
    ResolveState, Symbol,
};
pub(crate) use project_lsp_types::{CoreCompletion, CoreCompletionResponse};
pub use project_support_types::{ColorPresentation, DirectoryItem, DirectoryLister, DocumentColor};
pub use project_types::{
    AgentLocation, AgentLocationChanged, CURRENT_PROJECT_FEATURES, DebugAdapterClientState,
    DisableAiSettings, Event, LocalProjectFlags, LspPullDiagnostics, OpenedBufferEvent,
    ProjectItem, ProjectPath, PulledDiagnostics, ResolvedPath, ToastLink,
};
use project_types::{
    BufferOrderedMessage, DownloadingFile, EntitySubscription, ProjectClientState,
    RemotelyCreatedModelGuard, RemotelyCreatedModels,
};
pub mod search_history;
pub mod yarn;

use itertools::{Either, Itertools};

use crate::{
    bookmark_store::BookmarkStore,
    git_store::GitStore,
    lsp_store::{SymbolLocation, log_store::LogKind},
    project_search::SearchResultsHandle,
    trusted_worktrees::{PathTrust, RemoteHostLocation, TrustedWorktrees},
    worktree_store::WorktreeIdCounter,
};
pub use agent_registry_store::{AgentRegistryStore, RegistryAgent};
pub use agent_server_store::{AgentId, AgentServerStore, AgentServersUpdated, ExternalAgentSource};
pub use git_store::{
    ConflictRegion, ConflictSet, ConflictSetSnapshot, ConflictSetUpdate,
    git_traversal::{ChildEntriesGitIter, GitEntry, GitEntryRef, GitTraversal},
    linked_worktree_short_name, repo_identity_path, worktrees_directory_for_repo,
};
pub use manifest_tree::ManifestTree;
pub use project_search::{Search, SearchResults};
pub use worktree_store::WorktreePaths;

use anyhow::{Context as _, Result, anyhow};
use buffer_store::{BufferStore, BufferStoreEvent};
use client::{
    Client, Collaborator, PendingEntitySubscription, ProjectId, TypedEnvelope, UserStore, proto,
};
use clock::ReplicaId;

use dap::client::DebugAdapterClient;

use collections::{BTreeSet, HashMap, HashSet, IndexSet};
use debounced_delay::DebouncedDelay;
pub use debugger::breakpoint_store::BreakpointWithPosition;
use debugger::{
    breakpoint_store::{ActiveStackFrame, BreakpointStore},
    dap_store::{DapStore, DapStoreEvent},
    session::Session,
};

pub use environment::ProjectEnvironment;

use futures::{
    StreamExt,
    channel::mpsc::{self, UnboundedReceiver},
    future::try_join_all,
};
pub use image_store::{ImageItem, ImageStore};
use image_store::{ImageItemEvent, ImageStoreEvent};

use ::git::{blame::Blame, status::FileStatus};
use gpui::{
    App, AppContext, AsyncApp, BorrowAppContext, Context, Entity, EventEmitter, Hsla, SharedString,
    Task, TaskExt, WeakEntity, Window,
};
use language::{
    Buffer, BufferEditSource, BufferEvent, Capability, CodeLabel, CursorShape, DiskState, Language,
    LanguageName, LanguageRegistry, PointUtf16, ToOffset, ToPointUtf16, Toolchain,
    ToolchainMetadata, ToolchainScope, Transaction, Unclipped, language_settings::InlayHintKind,
    proto::split_operations,
};
use lsp::{
    CodeActionKind, CompletionContext, DocumentHighlightKind, InsertTextMode, LanguageServerBinary,
    LanguageServerId, LanguageServerName, LanguageServerSelector, MessageActionItem,
};
use lsp_command::*;
use lsp_store::{CompletionDocumentation, LspFormatTarget, OpenLspBufferHandle};
pub use manifest_tree::ManifestProvidersStore;
use node_runtime::NodeRuntime;
use parking_lot::Mutex;
pub use prettier_store::PrettierStore;
use project_settings::{ProjectSettings, SettingsObserver, SettingsObserverEvent};
#[cfg(target_os = "windows")]
use remote::wsl_path_to_windows_path;
use remote::{RemoteClient, RemoteConnectionOptions, same_remote_connection_identity};
use rpc::{
    AnyProtoClient, ErrorCode,
    proto::{LanguageServerPromptResponse, REMOTE_SERVER_PROJECT_ID},
};
use search::{SearchInputKind, SearchQuery, SearchResult};
use search_history::SearchHistory;
use settings::{InvalidSettingsError, RegisterSetting, Settings, SettingsLocation, SettingsStore};
use snippet::Snippet;
pub use snippet_provider;
use snippet_provider::SnippetProvider;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    ffi::OsString,
    future::Future,
    ops::{Not as _, Range},
    path::{Path, PathBuf},
    pin::pin,
    str::{self, FromStr},
    sync::Arc,
    time::Duration,
};

use task_store::TaskStore;
use terminals::Terminals;
use text::{Anchor, BufferId, Rope};
use toolchain_store::EmptyToolchainStore;
use util::{
    ResultExt as _, maybe,
    path_list::PathList,
    paths::{PathStyle, SanitizedPath, is_absolute},
    rel_path::RelPath,
};
use worktree::{CreatedEntry, Snapshot, Traversal};
pub use worktree::{
    Entry, EntryKind, FS_WATCH_LATENCY, File, LocalWorktree, PathChange, ProjectEntryId,
    UpdatedEntriesSet, UpdatedGitRepositoriesSet, Worktree, WorktreeId, WorktreeSettings,
    discover_root_repo_common_dir,
};
use worktree_store::{WorktreeStore, WorktreeStoreEvent};

use inline_values::provide_inline_values;

pub use fs::*;
pub use language::Location;
#[cfg(any(test, feature = "test-support"))]
pub use prettier::FORMAT_SUFFIX as TEST_PRETTIER_FORMAT_SUFFIX;
#[cfg(any(test, feature = "test-support"))]
pub use prettier::RANGE_FORMAT_SUFFIX as TEST_PRETTIER_RANGE_FORMAT_SUFFIX;
pub use task_inventory::{
    BasicContextProvider, ContextProviderWithTasks, DebugScenarioContext, GIT_COMMAND_TASK_TAG,
    Inventory, TaskContexts, TaskSourceKind,
};

pub use buffer_store::ProjectTransaction;
pub use lsp_store::{
    DiagnosticSummary, InvalidationStrategy, LanguageServerLogType, LanguageServerProgress,
    LanguageServerPromptRequest, LanguageServerStatus, LanguageServerToQuery, LspStore,
    LspStoreEvent, ProgressToken, SERVER_PROGRESS_THROTTLE_TIMEOUT,
};
pub use toolchain_store::{ToolchainStore, Toolchains};

/// Semantics-aware entity that is relevant to one or more [`Worktree`] with the files.
/// `Project` is responsible for tasks, LSP and collab queries, synchronizing worktree states accordingly.
/// Maps [`Worktree`] entries with its own logic using [`ProjectEntryId`] and [`ProjectPath`] structs.
///
/// Can be either local (for the project opened on the same host) or remote.(for collab projects, browsed by multiple remote users).
pub struct Project {
    active_entry: Option<ProjectEntryId>,
    buffer_ordered_messages_tx: mpsc::UnboundedSender<BufferOrderedMessage>,
    languages: Arc<LanguageRegistry>,
    dap_store: Entity<DapStore>,
    agent_server_store: Entity<AgentServerStore>,

    bookmark_store: Entity<BookmarkStore>,
    breakpoint_store: Entity<BreakpointStore>,
    collab_client: Arc<client::Client>,
    join_project_response_message_id: u32,
    task_store: Entity<TaskStore>,
    user_store: Entity<UserStore>,
    fs: Arc<dyn Fs>,
    remote_client: Option<Entity<RemoteClient>>,
    // todo lw explain the client_state x remote_client matrix, its super confusing
    client_state: ProjectClientState,
    git_store: Entity<GitStore>,
    collaborators: HashMap<proto::PeerId, Collaborator>,
    client_subscriptions: Vec<client::Subscription>,
    worktree_store: Entity<WorktreeStore>,
    buffer_store: Entity<BufferStore>,
    context_server_store: Entity<ContextServerStore>,
    image_store: Entity<ImageStore>,
    lsp_store: Entity<LspStore>,
    _subscriptions: Vec<gpui::Subscription>,
    buffers_needing_diff: HashSet<WeakEntity<Buffer>>,
    git_diff_debouncer: DebouncedDelay<Self>,
    remotely_created_models: Arc<Mutex<RemotelyCreatedModels>>,
    terminals: Terminals,
    node: Option<NodeRuntime>,
    search_history: SearchHistory,
    search_included_history: SearchHistory,
    search_excluded_history: SearchHistory,
    snippets: Entity<SnippetProvider>,
    environment: Entity<ProjectEnvironment>,
    settings_observer: Entity<SettingsObserver>,
    toolchain_store: Option<Entity<ToolchainStore>>,
    agent_location: Option<AgentLocation>,
    downloading_files: Arc<Mutex<HashMap<(WorktreeId, String), DownloadingFile>>>,
    last_worktree_paths: WorktreePaths,
}

pub use project_path_matching::{
    Candidates, PathMatchCandidateSet, PathMatchCandidateSetIter, PathMatchCandidateSetNucleoIter,
    ProjectGroupKey, path_suffix,
};

impl EventEmitter<Event> for Project {}

impl<'a> From<&'a ProjectPath> for SettingsLocation<'a> {
    fn from(val: &'a ProjectPath) -> Self {
        SettingsLocation {
            worktree_id: val.worktree_id,
            path: val.path.as_ref(),
        }
    }
}

impl<P: Into<Arc<RelPath>>> From<(WorktreeId, P)> for ProjectPath {
    fn from((worktree_id, path): (WorktreeId, P)) -> Self {
        Self {
            worktree_id,
            path: path.into(),
        }
    }
}

impl ProjectItem for Buffer {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<Result<Entity<Self>>>> {
        Some(project.update(cx, |project, cx| project.open_buffer(path.clone(), cx)))
    }

    fn entry_id(&self, _cx: &App) -> Option<ProjectEntryId> {
        File::from_dyn(self.file()).and_then(|file| file.project_entry_id())
    }

    fn project_path(&self, cx: &App) -> Option<ProjectPath> {
        let file = self.file()?;

        (!matches!(file.disk_state(), DiskState::Historic { .. })).then(|| ProjectPath {
            worktree_id: file.worktree_id(cx),
            path: file.path().clone(),
        })
    }

    fn is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

fn proto_to_prompt(level: proto::language_server_prompt_request::Level) -> gpui::PromptLevel {
    match level {
        proto::language_server_prompt_request::Level::Info(_) => gpui::PromptLevel::Info,
        proto::language_server_prompt_request::Level::Warning(_) => gpui::PromptLevel::Warning,
        proto::language_server_prompt_request::Level::Critical(_) => gpui::PromptLevel::Critical,
    }
}
