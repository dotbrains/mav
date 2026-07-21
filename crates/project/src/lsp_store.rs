//! LSP store provides unified access to the language server protocol.
//! The consumers of LSP store can interact with language servers without knowing exactly which language server they're interacting with.
//!
//! # Local/Remote LSP Stores
//! This module is split up into three distinct parts:
//! - [`LocalLspStore`], which is ran on the host machine (either project host or SSH host), that manages the lifecycle of language servers.
//! - [`RemoteLspStore`], which is ran on the remote machine (project guests) which is mostly about passing through the requests via RPC.
//!   The remote stores don't really care about which language server they're running against - they don't usually get to decide which language server is going to responsible for handling their request.
//! - [`LspStore`], which unifies the two under one consistent interface for interacting with language servers.
//!
//! Most of the interesting work happens at the local layer, as bulk of the complexity is with managing the lifecycle of language servers. The actual implementation of the LSP protocol is handled by [`lsp`] crate.
mod buffer_diagnostics;
mod buffer_language_maintenance;
mod buffer_lsp_data;
mod buffer_registration;
mod buffer_registration_entrypoint;
mod buffer_sync_notifications;
mod capability_registration;
mod capability_unregistration;
pub mod clangd_ext;
mod code_action_resolution;
pub mod code_lens;
mod completion_documentation;
mod completion_labels;
mod completion_requests;
mod completion_resolution;
mod completion_rpc_handlers;
mod diagnostic_entries;
mod diagnostic_summary;
mod diagnostic_updates;
mod diagnostics_types;
mod document_colors;
mod document_links;
mod document_symbols;
mod editor_query_requests;
mod file_watchers;
mod folding_ranges;
mod formatting_code_actions;
mod formatting_dispatch;
mod formatting_flow;
mod formatting_requests;
mod formatting_transaction;
mod formatting_types;
mod init_handlers;
mod inlay_hint_requests;
mod inlay_hint_resolution;
mod inlay_hints;
pub mod json_language_server_ext;
mod linked_edits;
mod local_buffer_lookup;
mod local_code_actions;
mod local_formatting;
mod local_lsp_adapter_delegate;
mod local_server_lookup;
mod local_server_runtime;
pub mod log_store;
mod lsp_buffer_snapshot;
mod lsp_command_handlers;
pub mod lsp_ext_command;
mod lsp_query_handlers;
mod lsp_query_serving;
mod lsp_store_events;
mod navigation_requests;
mod on_type_formatting;
mod open_buffer_requests;
mod progress_handlers;
mod progress_token;
mod prompt_and_log;
mod proto_serialization;
mod pull_diagnostics;
mod query_types;
mod registration_options;
mod remote_status_sync;
mod rename_watchers;
mod request_failure;
mod request_routing;
mod rpc_language_server_control;
pub mod rust_analyzer_ext;
mod semantic_tokens;
mod server_binary;
mod server_capabilities_update;
mod server_control_handlers;
mod server_identity;
mod server_insertion;
mod server_lifecycle_controls;
mod server_messages;
mod server_startup;
mod server_state;
mod server_status_subscription;
mod settings_helpers;
mod settings_server_tree;
mod ssh_lsp_adapter;
mod store_accessors;
mod store_events;
mod store_initialization;
mod store_mode;
mod supplementary_language_servers;
mod symbol_rpc_handlers;
mod symbol_types;
mod text_document_sync;
pub mod vue_language_server_ext;
mod workspace_config_refresh;
mod workspace_diagnostics;
mod workspace_diagnostics_handlers;
mod worktree_server_updates;

use self::buffer_lsp_data::{BufferLspData, LspKey};
use self::code_lens::CodeLensData;
pub use self::completion_documentation::CompletionDocumentation;
pub(crate) use self::completion_labels::{
    collapse_newlines, ensure_uniform_list_compatible_label, populate_labels_for_completions,
    populate_labels_for_symbols, remove_empty_hover_blocks, resolve_word_completion,
};
pub use self::diagnostic_summary::{DiagnosticSummary, LanguageServerProgress};
use self::document_colors::DocumentColorData;
use self::document_links::DocumentLinksData;
use self::document_symbols::DocumentSymbolsData;
use self::init_handlers::register_lsp_handlers;
use self::inlay_hints::BufferInlayHints;
pub use self::local_lsp_adapter_delegate::LocalLspAdapterDelegate;
pub use self::lsp_store_events::{LanguageServerStatus, LspStoreEvent};
pub use self::prompt_and_log::{LanguageServerLogType, LanguageServerPromptRequest};
pub use self::query_types::{LanguageServerToQuery, ResolvedHint};
use self::registration_options::server_capabilities_support_range_formatting;
use self::rename_watchers::{LanguageServerWatchedPaths, LazyGlobSet, RenamePathsWatchedForServer};
use self::request_failure::should_log_lsp_request_failure;
pub use self::server_state::LanguageServerState;
use self::server_state::glob_literal_prefix;
pub use self::settings_helpers::{language_server_settings, language_server_settings_for};
pub use self::ssh_lsp_adapter::SshLspAdapter;
use self::workspace_diagnostics::{
    WORKSPACE_DIAGNOSTICS_TOKEN_START, buffer_diagnostic_identifier,
};
use crate::{
    CodeAction, Completion, CompletionDisplayOptions, CompletionResponse, CompletionSource,
    CoreCompletion, Hover, InlayHint, InlayId, LocationLink, LspAction, LspPullDiagnostics,
    ManifestProvidersStore, Project, ProjectItem, ProjectPath, ProjectTransaction,
    PulledDiagnostics, ResolveState, Symbol,
    buffer_store::{BufferStore, BufferStoreEvent},
    environment::ProjectEnvironment,
    lsp_command::{self, *},
    lsp_store::{
        self,
        folding_ranges::FoldingRangeData,
        log_store::{GlobalLogStore, LanguageServerKind},
        semantic_tokens::{SemanticTokenConfig, SemanticTokensData},
    },
    manifest_tree::{
        LanguageServerTree, LanguageServerTreeNode, LaunchDisposition, ManifestQueryDelegate,
        ManifestTree,
    },
    prettier_store::{self, PrettierStore, PrettierStoreEvent},
    project_settings::{LspSettings, ProjectSettings},
    toolchain_store::{LocalToolchainStore, ToolchainStoreEvent},
    trusted_worktrees::{PathTrust, TrustedWorktrees, TrustedWorktreesEvent},
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
    yarn::YarnPathStore,
};
use anyhow::{Context as _, Result, anyhow};
use client::{TypedEnvelope, proto};
use clock::Global;
use collections::{BTreeMap, BTreeSet, HashMap, HashSet, btree_map};
use futures::{
    AsyncWriteExt, Future, FutureExt, StreamExt,
    future::{Shared, join_all},
    select, select_biased,
    stream::FuturesUnordered,
};
use globset::Glob;
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, EventEmitter, PromptLevel, SharedString,
    Subscription, Task, TaskExt, WeakEntity,
};
use http_client::HttpClient;
use itertools::Itertools as _;
use language::{
    Bias, BinaryStatus, Buffer, BufferRow, BufferSnapshot, CachedLspAdapter, Capability, CodeLabel,
    CodeLabelExt, Diagnostic, DiagnosticEntry, DiagnosticSet, DiagnosticSourceKind, Diff,
    File as _, Language, LanguageName, LanguageRegistry, LocalFile, LspAdapter, LspAdapterDelegate,
    ManifestDelegate, ManifestName, ModelineSettings, OffsetUtf16, Patch, PointUtf16,
    TextBufferSnapshot, ToOffset, ToOffsetUtf16, ToPointUtf16, Toolchain, Transaction, Unclipped,
    language_settings::{
        AllLanguageSettings, FormatOnSave, Formatter, LanguageSettings, LineEndingSetting,
        all_language_settings,
    },
    modeline, point_to_lsp,
    proto::{
        deserialize_anchor, deserialize_anchor_range, serialize_anchor, serialize_anchor_range,
        serialize_version,
    },
    range_from_lsp, range_to_lsp,
    row_chunk::RowChunk,
};
use lsp::{
    AdapterServerCapabilities, CodeActionKind, CompletionContext, DiagnosticSeverity,
    DiagnosticTag, DidChangeWatchedFilesRegistrationOptions, Edit, FileRename, FileSystemWatcher,
    LanguageServer, LanguageServerBinary, LanguageServerBinaryOptions, LanguageServerId,
    LanguageServerName, LanguageServerSelector, LspRequestFuture, MessageActionItem, OneOf,
    RenameFilesParams, SymbolKind, TextEdit, Uri, WillRenameFiles, WorkDoneProgressCancelParams,
    WorkspaceFolder, notification::DidRenameFiles,
};
use parking_lot::Mutex;
use postage::{sink::Sink, stream::Stream, watch};
use rand::prelude::*;
use rpc::{
    AnyProtoClient, ErrorCode, ErrorExt as _,
    proto::{LspRequestId, LspRequestMessage as _},
};
use settings::{Settings, SettingsLocation, SettingsStore};
use sha2::{Digest, Sha256};
use snippet::Snippet;
use std::{
    any::TypeId,
    borrow::Cow,
    cell::RefCell,
    cmp::{Ordering, Reverse},
    collections::{VecDeque, hash_map},
    ops::{ControlFlow, Range},
    path::{self, Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{self, AtomicUsize},
    },
    time::{Duration, Instant},
    vec,
};
use sum_tree::Dimensions;
use text::{Anchor, BufferId, LineEnding, OffsetRangeExt, ToPoint as _};

use util::{
    ResultExt as _, debug_panic, defer, maybe, merge_json_value_into,
    paths::{PathStyle, SanitizedPath, UrlExt},
    post_inc,
    redact::redact_command,
    rel_path::RelPath,
};

pub use diagnostics_types::{DocumentDiagnostics, DocumentDiagnosticsUpdate};
pub use document_colors::DocumentColors;
pub use document_links::{
    BufferDocumentLinks, DocumentLinkId, DocumentLinkResolveTask, LspDocumentLink,
    ResolvedDocumentLink,
};
pub use folding_ranges::LspFoldingRange;
use formatting_transaction::extend_formatting_transaction;
use formatting_types::OpenLspBuffer;
pub use formatting_types::{FormatTrigger, LspFormatTarget, OpenLspBufferHandle};
pub use fs::*;
pub use language::Location;
use lsp_buffer_snapshot::{LspBufferSnapshot, include_text};
pub use lsp_store::inlay_hints::{CacheInlayHints, InvalidationStrategy};
#[cfg(any(test, feature = "test-support"))]
pub use prettier::FORMAT_SUFFIX as TEST_PRETTIER_FORMAT_SUFFIX;
#[cfg(any(test, feature = "test-support"))]
pub use prettier::RANGE_FORMAT_SUFFIX as TEST_PRETTIER_RANGE_FORMAT_SUFFIX;
pub use progress_token::ProgressToken;
pub use semantic_tokens::{
    BufferSemanticToken, BufferSemanticTokens, RefreshForServer, SemanticTokenStylizer, TokenType,
};
use server_identity::{
    DynamicRegistrations, LanguageServerSeed, LanguageServerSeedSettings, UnifiedLanguageServer,
};
use server_status_subscription::subscribe_to_binary_statuses;
use store_mode::LspStoreMode;
pub use store_mode::{FormattableBuffer, RemoteLspStore};
use symbol_types::CoreSymbol;
pub use symbol_types::SymbolLocation;

pub use worktree::{
    Entry, EntryKind, FS_WATCH_LATENCY, File, LocalWorktree, PathChange, ProjectEntryId,
    UpdatedEntriesSet, UpdatedGitRepositoriesSet, Worktree, WorktreeId, WorktreeSettings,
};

const SERVER_LAUNCHING_BEFORE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
pub const SERVER_PROGRESS_THROTTLE_TIMEOUT: Duration = Duration::from_millis(100);
const SERVER_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10);
static NEXT_PROMPT_REQUEST_ID: AtomicUsize = AtomicUsize::new(0);

pub struct LocalLspStore {
    weak: WeakEntity<LspStore>,
    pub worktree_store: Entity<WorktreeStore>,
    toolchain_store: Entity<LocalToolchainStore>,
    http_client: Arc<dyn HttpClient>,
    environment: Entity<ProjectEnvironment>,
    fs: Arc<dyn Fs>,
    languages: Arc<LanguageRegistry>,
    language_server_ids: HashMap<LanguageServerSeed, UnifiedLanguageServer>,
    yarn: Entity<YarnPathStore>,
    pub language_servers: HashMap<LanguageServerId, LanguageServerState>,
    buffers_being_formatted: HashSet<BufferId>,
    last_workspace_edits_by_language_server: HashMap<LanguageServerId, ProjectTransaction>,
    language_server_watched_paths: HashMap<LanguageServerId, LanguageServerWatchedPaths>,
    watched_manifest_filenames: HashSet<ManifestName>,
    language_server_paths_watched_for_rename:
        HashMap<LanguageServerId, RenamePathsWatchedForServer>,
    language_server_dynamic_registrations: HashMap<LanguageServerId, DynamicRegistrations>,
    supplementary_language_servers:
        HashMap<LanguageServerId, (LanguageServerName, Arc<LanguageServer>)>,
    prettier_store: Entity<PrettierStore>,
    next_diagnostic_group_id: usize,
    diagnostics: HashMap<
        WorktreeId,
        HashMap<
            Arc<RelPath>,
            Vec<(
                LanguageServerId,
                Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
            )>,
        >,
    >,
    buffer_snapshots: HashMap<BufferId, HashMap<LanguageServerId, Vec<LspBufferSnapshot>>>, // buffer_id -> server_id -> vec of snapshots
    _subscription: gpui::Subscription,
    lsp_tree: LanguageServerTree,
    registered_buffers: HashMap<BufferId, usize>,
    buffers_opened_in_servers: HashMap<BufferId, HashSet<LanguageServerId>>,
    buffer_pull_diagnostics_result_ids: HashMap<
        LanguageServerId,
        HashMap<Option<SharedString>, HashMap<PathBuf, Option<SharedString>>>,
    >,
    workspace_pull_diagnostics_result_ids: HashMap<
        LanguageServerId,
        HashMap<Option<SharedString>, HashMap<PathBuf, Option<SharedString>>>,
    >,
    restricted_worktrees_tasks: HashMap<WorktreeId, (Subscription, watch::Receiver<bool>)>,
    all_language_servers_stopped: bool,
    stopped_language_servers: HashSet<LanguageServerName>,

    buffers_to_refresh_hash_set: HashSet<BufferId>,
    buffers_to_refresh_queue: VecDeque<BufferId>,
    _background_diagnostics_worker: Shared<Task<()>>,
}

pub struct LspStore {
    mode: LspStoreMode,
    last_formatting_failure: Option<String>,
    downstream_client: Option<(AnyProtoClient, u64)>,
    nonce: u128,
    buffer_store: Entity<BufferStore>,
    worktree_store: Entity<WorktreeStore>,
    pub languages: Arc<LanguageRegistry>,
    pub language_server_statuses: BTreeMap<LanguageServerId, LanguageServerStatus>,
    active_entry: Option<ProjectEntryId>,
    _maintain_workspace_config: (Task<Result<()>>, watch::Sender<()>),
    _maintain_buffer_languages: Task<()>,
    diagnostic_summaries:
        HashMap<WorktreeId, HashMap<Arc<RelPath>, HashMap<LanguageServerId, DiagnosticSummary>>>,
    pub lsp_server_capabilities: HashMap<LanguageServerId, lsp::ServerCapabilities>,
    semantic_token_config: SemanticTokenConfig,
    lsp_data: HashMap<BufferId, BufferLspData>,
    buffer_reload_tasks: HashMap<BufferId, Task<anyhow::Result<()>>>,
    next_hint_id: Arc<AtomicUsize>,
}

impl LspStore {}

impl EventEmitter<LspStoreEvent> for LspStore {}
