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
mod capability_registration;
mod capability_unregistration;
pub mod clangd_ext;
mod code_action_resolution;
pub mod code_lens;
mod completion_documentation;
mod completion_labels;
mod completion_requests;
mod completion_resolution;
mod diagnostic_summary;
mod diagnostic_updates;
mod diagnostics_types;
mod document_colors;
mod document_links;
mod document_symbols;
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
mod inlay_hints;
pub mod json_language_server_ext;
mod local_code_actions;
mod local_formatting;
mod local_lsp_adapter_delegate;
mod local_server_lookup;
mod local_server_runtime;
pub mod log_store;
mod lsp_buffer_snapshot;
pub mod lsp_ext_command;
mod lsp_query_serving;
mod lsp_store_events;
mod navigation_requests;
mod progress_handlers;
mod progress_token;
mod prompt_and_log;
mod proto_serialization;
mod pull_diagnostics;
mod query_types;
mod registration_options;
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
mod store_events;
mod store_mode;
mod symbol_types;
pub mod vue_language_server_ext;
mod workspace_diagnostics;

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

impl LocalLspStore {
    /// Returns the running language server for the given ID. Note if the language server is starting, it will not be returned.
    pub fn running_language_server_for_id(
        &self,
        id: LanguageServerId,
    ) -> Option<&Arc<LanguageServer>> {
        let language_server_state = self.language_servers.get(&id)?;

        match language_server_state {
            LanguageServerState::Running { server, .. } => Some(server),
            LanguageServerState::Starting { .. } => None,
        }
    }

    fn get_or_insert_language_server(
        &mut self,
        worktree_handle: &Entity<Worktree>,
        delegate: Arc<LocalLspAdapterDelegate>,
        disposition: &Arc<LaunchDisposition>,
        language_name: &LanguageName,
        cx: &mut App,
    ) -> LanguageServerId {
        let key = LanguageServerSeed {
            worktree_id: worktree_handle.read(cx).id(),
            name: disposition.server_name.clone(),
            settings: LanguageServerSeedSettings {
                binary: disposition.settings.binary.clone(),
                initialization_options: disposition.settings.initialization_options.clone(),
            },
            toolchain: disposition.toolchain.clone(),
        };
        if let Some(state) = self.language_server_ids.get_mut(&key) {
            state.project_roots.insert(disposition.path.path.clone());
            state.id
        } else {
            let adapter = self
                .languages
                .lsp_adapters(language_name)
                .into_iter()
                .find(|adapter| adapter.name() == disposition.server_name)
                .expect("To find LSP adapter");
            let new_language_server_id = self.start_language_server(
                worktree_handle,
                delegate,
                adapter,
                disposition.settings.clone(),
                key.clone(),
                language_name.clone(),
                cx,
            );
            if let Some(state) = self.language_server_ids.get_mut(&key) {
                state.project_roots.insert(disposition.path.path.clone());
            } else {
                debug_assert!(
                    false,
                    "Expected `start_language_server` to ensure that `key` exists in a map"
                );
            }
            new_language_server_id
        }
    }
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

impl LspStore {
    pub fn init(client: &AnyProtoClient) {
        register_lsp_handlers(client);
    }

    pub fn as_remote(&self) -> Option<&RemoteLspStore> {
        match &self.mode {
            LspStoreMode::Remote(remote_lsp_store) => Some(remote_lsp_store),
            _ => None,
        }
    }

    pub fn as_local(&self) -> Option<&LocalLspStore> {
        match &self.mode {
            LspStoreMode::Local(local_lsp_store) => Some(local_lsp_store),
            _ => None,
        }
    }

    pub fn as_local_mut(&mut self) -> Option<&mut LocalLspStore> {
        match &mut self.mode {
            LspStoreMode::Local(local_lsp_store) => Some(local_lsp_store),
            _ => None,
        }
    }

    pub fn upstream_client(&self) -> Option<(AnyProtoClient, u64)> {
        match &self.mode {
            LspStoreMode::Remote(RemoteLspStore {
                upstream_client: Some(upstream_client),
                upstream_project_id,
                ..
            }) => Some((upstream_client.clone(), *upstream_project_id)),

            LspStoreMode::Remote(RemoteLspStore {
                upstream_client: None,
                ..
            }) => None,
            LspStoreMode::Local(_) => None,
        }
    }

    pub fn new_local(
        buffer_store: Entity<BufferStore>,
        worktree_store: Entity<WorktreeStore>,
        prettier_store: Entity<PrettierStore>,
        toolchain_store: Entity<LocalToolchainStore>,
        environment: Entity<ProjectEnvironment>,
        manifest_tree: Entity<ManifestTree>,
        languages: Arc<LanguageRegistry>,
        http_client: Arc<dyn HttpClient>,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) -> Self {
        let yarn = YarnPathStore::new(fs.clone(), cx);
        cx.subscribe(&buffer_store, Self::on_buffer_store_event)
            .detach();
        cx.subscribe(&worktree_store, Self::on_worktree_store_event)
            .detach();
        cx.subscribe(&prettier_store, Self::on_prettier_store_event)
            .detach();
        cx.subscribe(&toolchain_store, Self::on_toolchain_store_event)
            .detach();
        cx.observe_global::<SettingsStore>(Self::on_settings_changed)
            .detach();
        subscribe_to_binary_statuses(&languages, cx).detach();

        let _maintain_workspace_config = {
            let (sender, receiver) = watch::channel();
            (Self::maintain_workspace_config(receiver, cx), sender)
        };

        Self {
            mode: LspStoreMode::Local(LocalLspStore {
                weak: cx.weak_entity(),
                worktree_store: worktree_store.clone(),

                supplementary_language_servers: Default::default(),
                languages: languages.clone(),
                language_server_ids: Default::default(),
                language_servers: Default::default(),
                last_workspace_edits_by_language_server: Default::default(),
                language_server_watched_paths: Default::default(),
                language_server_paths_watched_for_rename: Default::default(),
                language_server_dynamic_registrations: Default::default(),
                buffers_being_formatted: Default::default(),
                buffers_to_refresh_hash_set: HashSet::default(),
                buffers_to_refresh_queue: VecDeque::new(),
                _background_diagnostics_worker: Task::ready(()).shared(),
                buffer_snapshots: Default::default(),
                prettier_store,
                environment,
                http_client,
                fs,
                yarn,
                next_diagnostic_group_id: Default::default(),
                diagnostics: Default::default(),
                _subscription: cx.on_app_quit(|this, _| {
                    this.as_local_mut()
                        .unwrap()
                        .shutdown_language_servers_on_quit()
                }),
                lsp_tree: LanguageServerTree::new(
                    manifest_tree,
                    languages.clone(),
                    toolchain_store.clone(),
                ),
                toolchain_store,
                registered_buffers: HashMap::default(),
                buffers_opened_in_servers: HashMap::default(),
                buffer_pull_diagnostics_result_ids: HashMap::default(),
                workspace_pull_diagnostics_result_ids: HashMap::default(),
                restricted_worktrees_tasks: HashMap::default(),
                all_language_servers_stopped: false,
                stopped_language_servers: HashSet::default(),
                watched_manifest_filenames: ManifestProvidersStore::global(cx)
                    .manifest_file_names(),
            }),
            last_formatting_failure: None,
            downstream_client: None,
            buffer_store,
            worktree_store,
            languages: languages.clone(),
            language_server_statuses: Default::default(),
            nonce: StdRng::from_os_rng().random(),
            diagnostic_summaries: HashMap::default(),
            lsp_server_capabilities: HashMap::default(),
            semantic_token_config: SemanticTokenConfig::new(cx),
            lsp_data: HashMap::default(),
            buffer_reload_tasks: HashMap::default(),
            next_hint_id: Arc::default(),
            active_entry: None,
            _maintain_workspace_config,
            _maintain_buffer_languages: Self::maintain_buffer_languages(languages, cx),
        }
    }

    fn send_lsp_proto_request<R: LspCommand>(
        &self,
        buffer: Entity<Buffer>,
        client: AnyProtoClient,
        upstream_project_id: u64,
        request: R,
        cx: &mut Context<LspStore>,
    ) -> Task<anyhow::Result<<R as LspCommand>::Response>> {
        if !self.is_capable_for_proto_request(&buffer, &request, cx) {
            return Task::ready(Ok(R::Response::default()));
        }
        let message = request.to_proto(upstream_project_id, buffer.read(cx));
        cx.spawn(async move |this, cx| {
            let response = client.request(message).await?;
            let this = this.upgrade().context("project dropped")?;
            request
                .response_from_proto(response, this, buffer, cx.clone())
                .await
        })
    }

    pub(super) fn new_remote(
        buffer_store: Entity<BufferStore>,
        worktree_store: Entity<WorktreeStore>,
        languages: Arc<LanguageRegistry>,
        upstream_client: AnyProtoClient,
        project_id: u64,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&buffer_store, Self::on_buffer_store_event)
            .detach();
        cx.subscribe(&worktree_store, Self::on_worktree_store_event)
            .detach();
        subscribe_to_binary_statuses(&languages, cx).detach();
        let _maintain_workspace_config = {
            let (sender, receiver) = watch::channel();
            (Self::maintain_workspace_config(receiver, cx), sender)
        };
        Self {
            mode: LspStoreMode::Remote(RemoteLspStore {
                upstream_client: Some(upstream_client),
                upstream_project_id: project_id,
            }),
            downstream_client: None,
            last_formatting_failure: None,
            buffer_store,
            worktree_store,
            languages: languages.clone(),
            language_server_statuses: Default::default(),
            nonce: StdRng::from_os_rng().random(),
            diagnostic_summaries: HashMap::default(),
            lsp_server_capabilities: HashMap::default(),
            semantic_token_config: SemanticTokenConfig::new(cx),
            next_hint_id: Arc::default(),
            lsp_data: HashMap::default(),
            buffer_reload_tasks: HashMap::default(),
            active_entry: None,

            _maintain_workspace_config,
            _maintain_buffer_languages: Self::maintain_buffer_languages(languages.clone(), cx),
        }
    }

    pub(crate) fn register_buffer_with_language_servers(
        &mut self,
        buffer: &Entity<Buffer>,
        only_register_servers: HashSet<LanguageServerSelector>,
        ignore_refcounts: bool,
        cx: &mut Context<Self>,
    ) -> OpenLspBufferHandle {
        let buffer_id = buffer.read(cx).remote_id();
        let handle = OpenLspBufferHandle(cx.new(|_| OpenLspBuffer(buffer.clone())));
        if let Some(local) = self.as_local_mut() {
            let refcount = local.registered_buffers.entry(buffer_id).or_insert(0);
            if !ignore_refcounts {
                *refcount += 1;
            }

            // We run early exits on non-existing buffers AFTER we mark the buffer as registered in order to handle buffer saving.
            // When a new unnamed buffer is created and saved, we will start loading it's language. Once the language is loaded, we go over all "language-less" buffers and try to fit that new language
            // with them. However, we do that only for the buffers that we think are open in at least one editor; thus, we need to keep tab of unnamed buffers as well, even though they're not actually registered with any language
            // servers in practice (we don't support non-file URI schemes in our LSP impl).
            let Some(file) = File::from_dyn(buffer.read(cx).file()) else {
                return handle;
            };
            if !file.is_local() {
                return handle;
            }

            if ignore_refcounts || *refcount == 1 {
                local.register_buffer_with_language_servers(buffer, only_register_servers, cx);
            }
            if !ignore_refcounts {
                cx.observe_release(&handle.0, move |lsp_store, buffer, cx| {
                    let refcount = {
                        let local = lsp_store.as_local_mut().unwrap();
                        let Some(refcount) = local.registered_buffers.get_mut(&buffer_id) else {
                            debug_panic!("bad refcounting");
                            return;
                        };

                        *refcount -= 1;
                        *refcount
                    };
                    if refcount == 0 {
                        lsp_store.lsp_data.remove(&buffer_id);
                        lsp_store.buffer_reload_tasks.remove(&buffer_id);
                        let local = lsp_store.as_local_mut().unwrap();
                        local.registered_buffers.remove(&buffer_id);

                        local.buffers_opened_in_servers.remove(&buffer_id);
                        if let Some(file) = File::from_dyn(buffer.0.read(cx).file()).cloned() {
                            local.unregister_old_buffer_from_language_servers(&buffer.0, &file, cx);

                            let buffer_abs_path = file.abs_path(cx);
                            for (_, buffer_pull_diagnostics_result_ids) in
                                &mut local.buffer_pull_diagnostics_result_ids
                            {
                                buffer_pull_diagnostics_result_ids.retain(
                                    |_, buffer_result_ids| {
                                        buffer_result_ids.remove(&buffer_abs_path);
                                        !buffer_result_ids.is_empty()
                                    },
                                );
                            }

                            let diagnostic_updates = local
                                .language_servers
                                .keys()
                                .cloned()
                                .map(|server_id| DocumentDiagnosticsUpdate {
                                    diagnostics: DocumentDiagnostics {
                                        document_abs_path: buffer_abs_path.clone(),
                                        version: None,
                                        diagnostics: Vec::new(),
                                    },
                                    result_id: None,
                                    registration_id: None,
                                    server_id,
                                    disk_based_sources: Cow::Borrowed(&[]),
                                })
                                .collect::<Vec<_>>();

                            lsp_store
                                .merge_diagnostic_entries(
                                    diagnostic_updates,
                                    |_, diagnostic, _| {
                                        diagnostic.source_kind != DiagnosticSourceKind::Pulled
                                    },
                                    cx,
                                )
                                .context("Clearing diagnostics for the closed buffer")
                                .log_err();
                        }
                    }
                })
                .detach();
            }
        } else if let Some((upstream_client, upstream_project_id)) = self.upstream_client() {
            let buffer_id = buffer.read(cx).remote_id().to_proto();
            cx.background_spawn(async move {
                upstream_client
                    .request(proto::RegisterBufferWithLanguageServers {
                        project_id: upstream_project_id,
                        buffer_id,
                        only_servers: only_register_servers
                            .into_iter()
                            .map(|selector| {
                                let selector = match selector {
                                    LanguageServerSelector::Id(language_server_id) => {
                                        proto::language_server_selector::Selector::ServerId(
                                            language_server_id.to_proto(),
                                        )
                                    }
                                    LanguageServerSelector::Name(language_server_name) => {
                                        proto::language_server_selector::Selector::Name(
                                            language_server_name.to_string(),
                                        )
                                    }
                                };
                                proto::LanguageServerSelector {
                                    selector: Some(selector),
                                }
                            })
                            .collect(),
                    })
                    .await
            })
            .detach();
        } else {
            // Our remote connection got closed
        }
        handle
    }

    pub fn buffer_store(&self) -> Entity<BufferStore> {
        self.buffer_store.clone()
    }

    pub fn set_active_entry(&mut self, active_entry: Option<ProjectEntryId>) {
        self.active_entry = active_entry;
    }

    pub(crate) fn send_diagnostic_summaries(&self, worktree: &mut Worktree) {
        if let Some((client, downstream_project_id)) = self.downstream_client.clone()
            && let Some(diangostic_summaries) = self.diagnostic_summaries.get(&worktree.id())
        {
            let mut summaries = diangostic_summaries.iter().flat_map(|(path, summaries)| {
                summaries
                    .iter()
                    .map(|(server_id, summary)| summary.to_proto(*server_id, path.as_ref()))
            });
            if let Some(summary) = summaries.next() {
                client
                    .send(proto::UpdateDiagnosticSummary {
                        project_id: downstream_project_id,
                        worktree_id: worktree.id().to_proto(),
                        summary: Some(summary),
                        more_summaries: summaries.collect(),
                    })
                    .log_err();
            }
        }
    }

    pub fn resolved_hint(
        &mut self,
        buffer_id: BufferId,
        id: InlayId,
        cx: &mut Context<Self>,
    ) -> Option<ResolvedHint> {
        let buffer = self.buffer_store.read(cx).get(buffer_id)?;

        let lsp_data = self.lsp_data.get_mut(&buffer_id)?;
        let buffer_lsp_hints = &mut lsp_data.inlay_hints;
        let hint = buffer_lsp_hints.hint_for_id(id)?.clone();
        let (server_id, resolve_data) = match &hint.resolve_state {
            ResolveState::Resolved => return Some(ResolvedHint::Resolved(hint)),
            ResolveState::Resolving => {
                return Some(ResolvedHint::Resolving(
                    buffer_lsp_hints.hint_resolves.get(&id)?.clone(),
                ));
            }
            ResolveState::CanResolve(server_id, resolve_data) => (*server_id, resolve_data.clone()),
        };

        let resolve_task = self.resolve_inlay_hint(hint, buffer, server_id, cx);
        let buffer_lsp_hints = &mut self.lsp_data.get_mut(&buffer_id)?.inlay_hints;
        let previous_task = buffer_lsp_hints.hint_resolves.insert(
            id,
            cx.spawn(async move |lsp_store, cx| {
                let resolved_hint = resolve_task.await;
                lsp_store
                    .update(cx, |lsp_store, _| {
                        if let Some(old_inlay_hint) = lsp_store
                            .lsp_data
                            .get_mut(&buffer_id)
                            .and_then(|buffer_lsp_data| buffer_lsp_data.inlay_hints.hint_for_id(id))
                        {
                            match resolved_hint {
                                Ok(resolved_hint) => {
                                    *old_inlay_hint = resolved_hint;
                                }
                                Err(e) => {
                                    old_inlay_hint.resolve_state =
                                        ResolveState::CanResolve(server_id, resolve_data);
                                    log::error!("Inlay hint resolve failed: {e:#}");
                                }
                            }
                        }
                    })
                    .ok();
            })
            .shared(),
        );
        debug_assert!(
            previous_task.is_none(),
            "Did not change hint's resolve state after spawning its resolve"
        );
        buffer_lsp_hints.hint_for_id(id)?.resolve_state = ResolveState::Resolving;
        None
    }

    pub(crate) fn linked_edits(
        &mut self,
        buffer: &Entity<Buffer>,
        position: Anchor,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<Range<Anchor>>>> {
        let snapshot = buffer.read(cx).snapshot();
        let scope = snapshot.language_scope_at(position);
        let Some(server_id) = self
            .as_local()
            .and_then(|local| {
                buffer.update(cx, |buffer, cx| {
                    local
                        .language_servers_for_buffer(buffer, cx)
                        .filter(|(_, server)| {
                            LinkedEditingRange::check_server_capabilities(server.capabilities())
                        })
                        .filter(|(adapter, _)| {
                            scope
                                .as_ref()
                                .map(|scope| scope.language_allowed(&adapter.name))
                                .unwrap_or(true)
                        })
                        .map(|(_, server)| LanguageServerToQuery::Other(server.server_id()))
                        .next()
                })
            })
            .or_else(|| {
                self.upstream_client()
                    .is_some()
                    .then_some(LanguageServerToQuery::FirstCapable)
            })
            .filter(|_| {
                maybe!({
                    buffer.read(cx).language_at(position)?;
                    Some(
                        LanguageSettings::for_buffer_at(&buffer.read(cx), position, cx)
                            .linked_edits,
                    )
                }) == Some(true)
            })
        else {
            return Task::ready(Ok(Vec::new()));
        };

        self.request_lsp(
            buffer.clone(),
            server_id,
            LinkedEditingRange { position },
            cx,
        )
    }

    fn apply_on_type_formatting(
        &mut self,
        buffer: Entity<Buffer>,
        position: Anchor,
        trigger: String,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        if let Some((client, project_id)) = self.upstream_client() {
            if !self.check_if_capable_for_proto_request(
                &buffer,
                |capabilities| {
                    OnTypeFormatting::supports_on_type_formatting(&trigger, capabilities)
                },
                cx,
            ) {
                return Task::ready(Ok(None));
            }
            let request = proto::OnTypeFormatting {
                project_id,
                buffer_id: buffer.read(cx).remote_id().into(),
                position: Some(serialize_anchor(&position)),
                trigger,
                version: serialize_version(&buffer.read(cx).version()),
            };
            cx.background_spawn(async move {
                client
                    .request(request)
                    .await?
                    .transaction
                    .map(language::proto::deserialize_transaction)
                    .transpose()
            })
        } else if let Some(local) = self.as_local_mut() {
            let buffer_id = buffer.read(cx).remote_id();
            local.buffers_being_formatted.insert(buffer_id);
            cx.spawn(async move |this, cx| {
                let _cleanup = defer({
                    let this = this.clone();
                    let mut cx = cx.clone();
                    move || {
                        this.update(&mut cx, |this, _| {
                            if let Some(local) = this.as_local_mut() {
                                local.buffers_being_formatted.remove(&buffer_id);
                            }
                        })
                        .ok();
                    }
                });

                buffer
                    .update(cx, |buffer, _| {
                        buffer.wait_for_edits(Some(position.timestamp()))
                    })
                    .await?;
                this.update(cx, |this, cx| {
                    let position = position.to_point_utf16(buffer.read(cx));
                    this.on_type_format(buffer, position, trigger, false, cx)
                })?
                .await
            })
        } else {
            Task::ready(Err(anyhow!("No upstream client or local language server")))
        }
    }

    pub fn on_type_format<T: ToPointUtf16>(
        &mut self,
        buffer: Entity<Buffer>,
        position: T,
        trigger: String,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        let position = position.to_point_utf16(buffer.read(cx));
        self.on_type_format_impl(buffer, position, trigger, push_to_history, cx)
    }

    fn on_type_format_impl(
        &mut self,
        buffer: Entity<Buffer>,
        position: PointUtf16,
        trigger: String,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        let options = buffer.update(cx, |buffer, cx| {
            lsp_command::lsp_formatting_options(
                LanguageSettings::for_buffer_at(buffer, position, cx).as_ref(),
            )
        });

        cx.spawn(async move |this, cx| {
            if let Some(waiter) =
                buffer.update(cx, |buffer, _| buffer.wait_for_autoindent_applied())
            {
                waiter.await?;
            }
            cx.update(|cx| {
                this.update(cx, |this, cx| {
                    this.request_lsp(
                        buffer.clone(),
                        LanguageServerToQuery::FirstCapable,
                        OnTypeFormatting {
                            position,
                            trigger,
                            options,
                            push_to_history,
                        },
                        cx,
                    )
                })
            })?
            .await
        })
    }

    pub fn signature_help<T: ToPointUtf16>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: T,
        cx: &mut Context<Self>,
    ) -> Task<Option<Vec<SignatureHelp>>> {
        let position = position.to_point_utf16(buffer.read(cx));

        if let Some((client, upstream_project_id)) = self.upstream_client() {
            let request = GetSignatureHelp { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(None);
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = client.request_lsp(
                upstream_project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(upstream_project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let lsp_store = weak_lsp_store.upgrade()?;
                let signatures = join_all(
                    request_task
                        .await
                        .log_err()
                        .flatten()
                        .map(|response| response.payload)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|response| {
                            let response = GetSignatureHelp { position }.response_from_proto(
                                response.response,
                                lsp_store.clone(),
                                buffer.clone(),
                                cx.clone(),
                            );
                            async move { response.await.log_err().flatten() }
                        }),
                )
                .await
                .into_iter()
                .flatten()
                .collect();
                Some(signatures)
            })
        } else {
            let all_actions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetSignatureHelp { position },
                cx,
            );
            cx.background_spawn(async move {
                Some(
                    all_actions_task
                        .await
                        .into_iter()
                        .flat_map(|(_, actions)| actions)
                        .collect::<Vec<_>>(),
                )
            })
        }
    }

    pub fn hover(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Option<Vec<Hover>>> {
        if let Some((client, upstream_project_id)) = self.upstream_client() {
            let request = GetHover { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(None);
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = client.request_lsp(
                upstream_project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(upstream_project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let lsp_store = weak_lsp_store.upgrade()?;
                let hovers = join_all(
                    request_task
                        .await
                        .log_err()
                        .flatten()
                        .map(|response| response.payload)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|response| {
                            let response = GetHover { position }.response_from_proto(
                                response.response,
                                lsp_store.clone(),
                                buffer.clone(),
                                cx.clone(),
                            );
                            async move {
                                response
                                    .await
                                    .log_err()
                                    .flatten()
                                    .and_then(remove_empty_hover_blocks)
                            }
                        }),
                )
                .await
                .into_iter()
                .flatten()
                .collect();
                Some(hovers)
            })
        } else {
            let all_actions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetHover { position },
                cx,
            );
            cx.background_spawn(async move {
                Some(
                    all_actions_task
                        .await
                        .into_iter()
                        .filter_map(|(_, hover)| remove_empty_hover_blocks(hover?))
                        .collect::<Vec<Hover>>(),
                )
            })
        }
    }

    pub fn symbols(&self, query: &str, cx: &mut Context<Self>) -> Task<Result<Vec<Symbol>>> {
        let language_registry = self.languages.clone();

        if let Some((upstream_client, project_id)) = self.upstream_client().as_ref() {
            let request = upstream_client.request(proto::GetProjectSymbols {
                project_id: *project_id,
                query: query.to_string(),
            });
            cx.foreground_executor().spawn(async move {
                let response = request.await?;
                let mut symbols = Vec::new();
                let core_symbols = response
                    .symbols
                    .into_iter()
                    .filter_map(|symbol| Self::deserialize_symbol(symbol).log_err())
                    .collect::<Vec<_>>();
                populate_labels_for_symbols(core_symbols, &language_registry, None, &mut symbols)
                    .await;
                Ok(symbols)
            })
        } else if let Some(local) = self.as_local() {
            struct WorkspaceSymbolsResult {
                server_id: LanguageServerId,
                lsp_adapter: Arc<CachedLspAdapter>,
                worktree: WeakEntity<Worktree>,
                lsp_symbols: Vec<(String, SymbolKind, lsp::Location, Option<String>)>,
            }

            let mut requests = Vec::new();
            let mut requested_servers = BTreeSet::new();
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();

            for (seed, state) in local.language_server_ids.iter() {
                let Some(worktree_handle) = self
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(seed.worktree_id, cx)
                else {
                    continue;
                };

                let worktree = worktree_handle.read(cx);
                if !worktree.is_visible() {
                    continue;
                }

                if !requested_servers.insert(state.id) {
                    continue;
                }

                let (lsp_adapter, server) = match local.language_servers.get(&state.id) {
                    Some(LanguageServerState::Running {
                        adapter, server, ..
                    }) => (adapter.clone(), server),

                    _ => continue,
                };

                let supports_workspace_symbol_request =
                    match server.capabilities().workspace_symbol_provider {
                        Some(OneOf::Left(supported)) => supported,
                        Some(OneOf::Right(_)) => true,
                        None => false,
                    };

                if !supports_workspace_symbol_request {
                    continue;
                }

                let worktree_handle = worktree_handle.clone();
                let server_id = server.server_id();
                requests.push(
                    server
                        .request::<lsp::request::WorkspaceSymbolRequest>(
                            lsp::WorkspaceSymbolParams {
                                query: query.to_string(),
                                ..Default::default()
                            },
                            request_timeout,
                        )
                        .map(move |response| {
                            let lsp_symbols = response
                                .into_response()
                                .context("workspace symbols request")
                                .log_err()
                                .flatten()
                                .map(|symbol_response| match symbol_response {
                                    lsp::WorkspaceSymbolResponse::Flat(flat_responses) => {
                                        flat_responses
                                            .into_iter()
                                            .map(|lsp_symbol| {
                                                (
                                                    lsp_symbol.name,
                                                    lsp_symbol.kind,
                                                    lsp_symbol.location,
                                                    lsp_symbol.container_name,
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    }
                                    lsp::WorkspaceSymbolResponse::Nested(nested_responses) => {
                                        nested_responses
                                            .into_iter()
                                            .filter_map(|lsp_symbol| {
                                                let location = match lsp_symbol.location {
                                                    OneOf::Left(location) => location,
                                                    OneOf::Right(_) => {
                                                        log::error!(
                                                            "Unexpected: client capabilities \
                                                            forbid symbol resolutions in \
                                                            workspace.symbol.resolveSupport"
                                                        );
                                                        return None;
                                                    }
                                                };
                                                Some((
                                                    lsp_symbol.name,
                                                    lsp_symbol.kind,
                                                    location,
                                                    lsp_symbol.container_name,
                                                ))
                                            })
                                            .collect::<Vec<_>>()
                                    }
                                })
                                .unwrap_or_default();

                            WorkspaceSymbolsResult {
                                server_id,
                                lsp_adapter,
                                worktree: worktree_handle.downgrade(),
                                lsp_symbols,
                            }
                        }),
                );
            }

            cx.spawn(async move |this, cx| {
                let responses = futures::future::join_all(requests).await;
                let this = match this.upgrade() {
                    Some(this) => this,
                    None => return Ok(Vec::new()),
                };

                let mut symbols = Vec::new();
                for result in responses {
                    let core_symbols = this.update(cx, |this, cx| {
                        result
                            .lsp_symbols
                            .into_iter()
                            .filter_map(
                                |(symbol_name, symbol_kind, symbol_location, container_name)| {
                                    let abs_path = symbol_location.uri.to_file_path().ok()?;
                                    let source_worktree = result.worktree.upgrade()?;
                                    let source_worktree_id = source_worktree.read(cx).id();

                                    let path = if let Some((tree, rel_path)) =
                                        this.worktree_store.read(cx).find_worktree(&abs_path, cx)
                                    {
                                        let worktree_id = tree.read(cx).id();
                                        SymbolLocation::InProject(ProjectPath {
                                            worktree_id,
                                            path: rel_path,
                                        })
                                    } else {
                                        SymbolLocation::OutsideProject {
                                            signature: this.symbol_signature(&abs_path),
                                            abs_path: abs_path.into(),
                                        }
                                    };

                                    Some(CoreSymbol {
                                        source_language_server_id: result.server_id,
                                        language_server_name: result.lsp_adapter.name.clone(),
                                        source_worktree_id,
                                        path,
                                        kind: symbol_kind,
                                        name: collapse_newlines(&symbol_name, "↵ "),
                                        range: range_from_lsp(symbol_location.range),
                                        container_name: container_name
                                            .map(|c| collapse_newlines(&c, "↵ ")),
                                    })
                                },
                            )
                            .collect::<Vec<_>>()
                    });

                    populate_labels_for_symbols(
                        core_symbols,
                        &language_registry,
                        Some(result.lsp_adapter),
                        &mut symbols,
                    )
                    .await;
                }

                Ok(symbols)
            })
        } else {
            Task::ready(Err(anyhow!("No upstream client or local language server")))
        }
    }

    pub fn diagnostic_summary(&self, include_ignored: bool, cx: &App) -> DiagnosticSummary {
        let mut summary = DiagnosticSummary::default();
        for (_, _, path_summary) in self.diagnostic_summaries(include_ignored, cx) {
            summary.error_count += path_summary.error_count;
            summary.warning_count += path_summary.warning_count;
        }
        summary
    }

    /// Returns the diagnostic summary for a specific project path.
    pub fn diagnostic_summary_for_path(
        &self,
        project_path: &ProjectPath,
        _: &App,
    ) -> DiagnosticSummary {
        if let Some(summaries) = self
            .diagnostic_summaries
            .get(&project_path.worktree_id)
            .and_then(|map| map.get(&project_path.path))
        {
            let (error_count, warning_count) = summaries.iter().fold(
                (0, 0),
                |(error_count, warning_count), (_language_server_id, summary)| {
                    (
                        error_count + summary.error_count,
                        warning_count + summary.warning_count,
                    )
                },
            );

            DiagnosticSummary {
                error_count,
                warning_count,
            }
        } else {
            DiagnosticSummary::default()
        }
    }

    pub fn diagnostic_summaries<'a>(
        &'a self,
        include_ignored: bool,
        cx: &'a App,
    ) -> impl Iterator<Item = (ProjectPath, LanguageServerId, DiagnosticSummary)> + 'a {
        self.worktree_store
            .read(cx)
            .visible_worktrees(cx)
            .filter_map(|worktree| {
                let worktree = worktree.read(cx);
                Some((worktree, self.diagnostic_summaries.get(&worktree.id())?))
            })
            .flat_map(move |(worktree, summaries)| {
                let worktree_id = worktree.id();
                summaries
                    .iter()
                    .filter(move |(path, _)| {
                        include_ignored
                            || worktree
                                .entry_for_path(path.as_ref())
                                .is_some_and(|entry| !entry.is_ignored)
                    })
                    .flat_map(move |(path, summaries)| {
                        summaries.iter().map(move |(server_id, summary)| {
                            (
                                ProjectPath {
                                    worktree_id,
                                    path: path.clone(),
                                },
                                *server_id,
                                *summary,
                            )
                        })
                    })
            })
    }

    pub fn on_buffer_edited(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let language_servers: Vec<_> = buffer.update(cx, |buffer, cx| {
            Some(
                self.as_local()?
                    .language_servers_for_buffer(buffer, cx)
                    .map(|i| i.1.clone())
                    .collect(),
            )
        })?;

        let buffer = buffer.read(cx);
        let file = File::from_dyn(buffer.file())?;
        let abs_path = file.as_local()?.abs_path(cx);
        let uri = lsp::Uri::from_file_path(&abs_path)
            .ok()
            .with_context(|| format!("Failed to convert path to URI: {}", abs_path.display()))
            .log_err()?;
        let next_snapshot = buffer.text_snapshot();
        for language_server in language_servers {
            let language_server = language_server.clone();

            let buffer_snapshots = self
                .as_local_mut()?
                .buffer_snapshots
                .get_mut(&buffer.remote_id())
                .and_then(|m| m.get_mut(&language_server.server_id()))?;
            let previous_snapshot = buffer_snapshots.last()?;

            let build_incremental_change = || {
                buffer
                    .edits_since::<Dimensions<PointUtf16, usize>>(
                        previous_snapshot.snapshot.version(),
                    )
                    .map(|edit| {
                        let edit_start = edit.new.start.0;
                        let edit_end = edit_start + (edit.old.end.0 - edit.old.start.0);
                        let new_text = next_snapshot
                            .text_for_range(edit.new.start.1..edit.new.end.1)
                            .collect();
                        lsp::TextDocumentContentChangeEvent {
                            range: Some(lsp::Range::new(
                                point_to_lsp(edit_start),
                                point_to_lsp(edit_end),
                            )),
                            range_length: None,
                            text: new_text,
                        }
                    })
                    .collect()
            };

            let document_sync_kind = language_server
                .capabilities()
                .text_document_sync
                .as_ref()
                .and_then(|sync| match sync {
                    lsp::TextDocumentSyncCapability::Kind(kind) => Some(*kind),
                    lsp::TextDocumentSyncCapability::Options(options) => options.change,
                });

            let content_changes: Vec<_> = match document_sync_kind {
                Some(lsp::TextDocumentSyncKind::FULL) => {
                    vec![lsp::TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: next_snapshot.text(),
                    }]
                }
                Some(lsp::TextDocumentSyncKind::INCREMENTAL) => build_incremental_change(),
                _ => {
                    #[cfg(any(test, feature = "test-support"))]
                    {
                        build_incremental_change()
                    }

                    #[cfg(not(any(test, feature = "test-support")))]
                    {
                        continue;
                    }
                }
            };

            let next_version = previous_snapshot.version + 1;
            buffer_snapshots.push(LspBufferSnapshot {
                version: next_version,
                snapshot: next_snapshot.clone(),
            });

            language_server
                .notify::<lsp::notification::DidChangeTextDocument>(
                    lsp::DidChangeTextDocumentParams {
                        text_document: lsp::VersionedTextDocumentIdentifier::new(
                            uri.clone(),
                            next_version,
                        ),
                        content_changes,
                    },
                )
                .ok();
            self.pull_workspace_diagnostics(language_server.server_id());
        }

        None
    }

    pub fn on_buffer_saved(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let file = File::from_dyn(buffer.read(cx).file())?;
        let worktree_id = file.worktree_id(cx);
        let abs_path = file.as_local()?.abs_path(cx);
        let text_document = lsp::TextDocumentIdentifier {
            uri: file_path_to_lsp_url(&abs_path).log_err()?,
        };
        let local = self.as_local()?;

        for server in local.language_servers_for_worktree(worktree_id) {
            if let Some(include_text) = include_text(server.as_ref()) {
                let text = if include_text {
                    Some(buffer.read(cx).text())
                } else {
                    None
                };
                server
                    .notify::<lsp::notification::DidSaveTextDocument>(
                        lsp::DidSaveTextDocumentParams {
                            text_document: text_document.clone(),
                            text,
                        },
                    )
                    .ok();
            }
        }

        let language_servers = buffer.update(cx, |buffer, cx| {
            local.language_server_ids_for_buffer(buffer, cx)
        });
        for language_server_id in language_servers {
            self.simulate_disk_based_diagnostics_events_if_needed(language_server_id, cx);
        }

        None
    }

    async fn refresh_workspace_configurations(lsp_store: &WeakEntity<Self>, cx: &mut AsyncApp) {
        maybe!(async move {
            let mut refreshed_servers = HashSet::default();
            let servers = lsp_store
                .update(cx, |lsp_store, cx| {
                    let local = lsp_store.as_local()?;

                    let servers = local
                        .language_server_ids
                        .iter()
                        .filter_map(|(seed, state)| {
                            let worktree = lsp_store
                                .worktree_store
                                .read(cx)
                                .worktree_for_id(seed.worktree_id, cx);
                            let delegate: Arc<dyn LspAdapterDelegate> =
                                worktree.map(|worktree| {
                                    LocalLspAdapterDelegate::new(
                                        local.languages.clone(),
                                        &local.environment,
                                        cx.weak_entity(),
                                        &worktree,
                                        local.http_client.clone(),
                                        local.fs.clone(),
                                        cx,
                                    )
                                })?;
                            let server_id = state.id;

                            let states = local.language_servers.get(&server_id)?;

                            match states {
                                LanguageServerState::Starting { .. } => None,
                                LanguageServerState::Running {
                                    adapter, server, ..
                                } => {
                                    let adapter = adapter.clone();
                                    let server = server.clone();
                                    refreshed_servers.insert(server.name());
                                    let toolchain = seed.toolchain.clone();
                                    Some(cx.spawn(async move |_, cx| {
                                        let settings =
                                            LocalLspStore::workspace_configuration_for_adapter(
                                                adapter.adapter.clone(),
                                                &delegate,
                                                toolchain,
                                                None,
                                                cx,
                                            )
                                            .await
                                            .ok()?;
                                        server
                                            .notify::<lsp::notification::DidChangeConfiguration>(
                                                lsp::DidChangeConfigurationParams { settings },
                                            )
                                            .ok()?;
                                        Some(())
                                    }))
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    Some(servers)
                })
                .ok()
                .flatten()?;

            log::debug!("Refreshing workspace configurations for servers {refreshed_servers:?}");
            // TODO this asynchronous job runs concurrently with extension (de)registration and may take enough time for a certain extension
            // to stop and unregister its language server wrapper.
            // This is racy : an extension might have already removed all `local.language_servers` state, but here we `.clone()` and hold onto it anyway.
            // This now causes errors in the logs, we should find a way to remove such servers from the processing everywhere.
            let _: Vec<Option<()>> = join_all(servers).await;

            Some(())
        })
        .await;
    }

    fn maintain_workspace_config(
        mut external_refresh_requests: watch::Receiver<()>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        // Multiple things can happen when a workspace environment (selected toolchain + settings) change:
        // - We might shut down a language server if it's no longer enabled for a given language (and there are no buffers using it otherwise).
        // - We might also shut it down when the workspace configuration of all of the users of a given language server converges onto that of the other.
        // - In the same vein, we might also decide to start a new language server if the workspace configuration *diverges* from the other.
        // - In the easiest case (where we're not wrangling the lifetime of a language server anyhow), if none of the roots of a single language server diverge in their configuration,
        // but it is still different to what we had before, we're gonna send out a workspace configuration update.
        //
        // Settings-store changes reach this loop via `on_settings_changed` -> `request_workspace_config_refresh`,
        // which writes to `external_refresh_requests`. Observing `SettingsStore` here as well would cause every
        // settings change to drive the loop twice and emit duplicate `workspace/didChangeConfiguration` notifications.
        cx.spawn(async move |this, cx| {
            while let Some(()) = external_refresh_requests.next().await {
                this.update(cx, |this, cx| {
                    this.refresh_server_tree(cx);
                })
                .ok();

                Self::refresh_workspace_configurations(&this, cx).await;
            }

            anyhow::Ok(())
        })
    }

    pub fn running_language_servers_for_local_buffer<'a>(
        &'a self,
        buffer: &Buffer,
        cx: &mut App,
    ) -> impl Iterator<Item = (&'a Arc<CachedLspAdapter>, &'a Arc<LanguageServer>)> {
        let local = self.as_local();
        let language_server_ids = local
            .map(|local| local.language_server_ids_for_buffer(buffer, cx))
            .unwrap_or_default();

        language_server_ids
            .into_iter()
            .filter_map(
                move |server_id| match local?.language_servers.get(&server_id)? {
                    LanguageServerState::Running {
                        adapter, server, ..
                    } => Some((adapter, server)),
                    _ => None,
                },
            )
    }

    pub fn language_servers_for_local_buffer(
        &self,
        buffer: &Buffer,
        cx: &mut App,
    ) -> Vec<LanguageServerId> {
        let local = self.as_local();
        local
            .map(|local| local.language_server_ids_for_buffer(buffer, cx))
            .unwrap_or_default()
    }

    pub fn language_server_for_local_buffer<'a>(
        &'a self,
        buffer: &'a Buffer,
        server_id: LanguageServerId,
        cx: &'a mut App,
    ) -> Option<(&'a Arc<CachedLspAdapter>, &'a Arc<LanguageServer>)> {
        self.as_local()?
            .language_servers_for_buffer(buffer, cx)
            .find(|(_, s)| s.server_id() == server_id)
    }

    fn remove_worktree(&mut self, id_to_remove: WorktreeId, cx: &mut Context<Self>) {
        self.diagnostic_summaries.remove(&id_to_remove);
        if let Some(local) = self.as_local_mut() {
            let to_remove = local.remove_worktree(id_to_remove, cx);
            for server in to_remove {
                self.language_server_statuses.remove(&server);
            }
        }
    }

    fn invalidate_diagnostic_summaries_for_removed_entries(
        &mut self,
        worktree_id: WorktreeId,
        changes: &UpdatedEntriesSet,
        cx: &mut Context<Self>,
    ) {
        let Some(summaries_for_tree) = self.diagnostic_summaries.get_mut(&worktree_id) else {
            return;
        };

        let mut cleared_paths: Vec<ProjectPath> = Vec::new();
        let mut cleared_server_ids: HashSet<LanguageServerId> = HashSet::default();
        let downstream = self.downstream_client.clone();

        for (path, _, _) in changes
            .iter()
            .filter(|(_, _, change)| *change == PathChange::Removed)
        {
            if let Some(summaries_by_server_id) = summaries_for_tree.remove(path) {
                for (server_id, _) in &summaries_by_server_id {
                    cleared_server_ids.insert(*server_id);
                    if let Some((client, project_id)) = &downstream {
                        client
                            .send(proto::UpdateDiagnosticSummary {
                                project_id: *project_id,
                                worktree_id: worktree_id.to_proto(),
                                summary: Some(proto::DiagnosticSummary {
                                    path: path.as_ref().to_proto(),
                                    language_server_id: server_id.0 as u64,
                                    error_count: 0,
                                    warning_count: 0,
                                }),
                                more_summaries: Vec::new(),
                            })
                            .ok();
                    }
                }
                cleared_paths.push(ProjectPath {
                    worktree_id,
                    path: path.clone(),
                });
            }
        }

        if !cleared_paths.is_empty() {
            for server_id in cleared_server_ids {
                cx.emit(LspStoreEvent::DiagnosticsUpdated {
                    server_id,
                    paths: cleared_paths.clone(),
                });
            }
        }
    }

    pub fn shared(
        &mut self,
        project_id: u64,
        downstream_client: AnyProtoClient,
        _: &mut Context<Self>,
    ) {
        self.downstream_client = Some((downstream_client.clone(), project_id));

        for (server_id, status) in &self.language_server_statuses {
            if let Some(server) = self.language_server_for_id(*server_id) {
                downstream_client
                    .send(proto::StartLanguageServer {
                        project_id,
                        server: Some(proto::LanguageServer {
                            id: server_id.to_proto(),
                            name: status.name.to_string(),
                            worktree_id: status.worktree.map(|id| id.to_proto()),
                            language_name: status
                                .language_name
                                .as_ref()
                                .map(|name| name.to_proto()),
                        }),
                        capabilities: serde_json::to_string(&server.capabilities())
                            .expect("serializing server LSP capabilities"),
                    })
                    .log_err();
            }
        }
    }

    pub fn disconnected_from_host(&mut self) {
        self.downstream_client.take();
    }

    pub fn disconnected_from_ssh_remote(&mut self) {
        if let LspStoreMode::Remote(RemoteLspStore {
            upstream_client, ..
        }) = &mut self.mode
        {
            upstream_client.take();
        }
    }

    pub(crate) fn set_language_server_statuses_from_proto(
        &mut self,
        project: WeakEntity<Project>,
        language_servers: Vec<proto::LanguageServer>,
        server_capabilities: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let lsp_logs = cx
            .try_global::<GlobalLogStore>()
            .map(|lsp_store| lsp_store.0.clone());

        self.language_server_statuses = language_servers
            .into_iter()
            .zip(server_capabilities)
            .map(|(server, server_capabilities)| {
                let server_id = LanguageServerId(server.id as usize);
                if let Ok(server_capabilities) = serde_json::from_str(&server_capabilities) {
                    self.lsp_server_capabilities
                        .insert(server_id, server_capabilities);
                }

                let name = LanguageServerName::from_proto(server.name);
                let worktree = server.worktree_id.map(WorktreeId::from_proto);
                let language_name = server.language_name.map(LanguageName::from_proto);

                if let Some(lsp_logs) = &lsp_logs {
                    lsp_logs.update(cx, |lsp_logs, cx| {
                        lsp_logs.add_language_server(
                            // Only remote clients get their language servers set from proto
                            LanguageServerKind::Remote {
                                project: project.clone(),
                            },
                            server_id,
                            Some(name.clone()),
                            worktree,
                            None,
                            cx,
                        );
                    });
                }

                if let Some(ref lang_name) = language_name {
                    self.try_register_remote_adapter_locally(&name, lang_name);
                }

                (
                    server_id,
                    LanguageServerStatus {
                        name,
                        language_name: language_name,
                        server_version: None,
                        server_readable_version: None,
                        pending_work: Default::default(),
                        has_pending_diagnostic_updates: false,
                        progress_tokens: Default::default(),
                        worktree,
                        binary: None,
                        configuration: None,
                        workspace_folders: BTreeSet::new(),
                        process_id: None,
                    },
                )
            })
            .collect();
    }

    fn try_register_remote_adapter_locally(
        &self,
        server_name: &LanguageServerName,
        language_name: &LanguageName,
    ) {
        let already_registered = self
            .languages
            .lsp_adapters(language_name)
            .iter()
            .any(|adapter| adapter.name() == *server_name);

        if already_registered {
            return;
        }

        if let Some(adapter) = self.languages.load_available_lsp_adapter(server_name) {
            log::info!(
                "Registering LSP adapter '{}' for language '{}' on local client",
                server_name.0,
                language_name.0
            );
            self.languages
                .register_lsp_adapter(language_name.clone(), adapter.adapter.clone());
        } else {
            log::warn!(
                "LSP adapter '{}' for language '{}' not available locally",
                server_name.0,
                language_name.0
            );
        }
    }

    #[cfg(feature = "test-support")]
    pub fn update_diagnostic_entries(
        &mut self,
        server_id: LanguageServerId,
        abs_path: PathBuf,
        result_id: Option<SharedString>,
        version: Option<i32>,
        diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.merge_diagnostic_entries(
            vec![DocumentDiagnosticsUpdate {
                diagnostics: DocumentDiagnostics {
                    diagnostics,
                    document_abs_path: abs_path,
                    version,
                },
                result_id,
                server_id,
                disk_based_sources: Cow::Borrowed(&[]),
                registration_id: None,
            }],
            |_, _, _| false,
            cx,
        )?;
        Ok(())
    }

    pub fn merge_diagnostic_entries<'a>(
        &mut self,
        diagnostic_updates: Vec<DocumentDiagnosticsUpdate<'a, DocumentDiagnostics>>,
        merge: impl Fn(&lsp::Uri, &Diagnostic, &App) -> bool + Clone,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let mut diagnostics_summary = None::<proto::UpdateDiagnosticSummary>;
        let mut updated_diagnostics_paths = HashMap::default();
        for mut update in diagnostic_updates {
            let abs_path = &update.diagnostics.document_abs_path;
            let server_id = update.server_id;
            let Some((worktree, relative_path)) =
                self.worktree_store.read(cx).find_worktree(abs_path, cx)
            else {
                log::warn!("skipping diagnostics update, no worktree found for path {abs_path:?}");
                return Ok(());
            };

            let worktree_id = worktree.read(cx).id();
            let project_path = ProjectPath {
                worktree_id,
                path: relative_path,
            };

            let document_uri = lsp::Uri::from_file_path(abs_path)
                .map_err(|()| anyhow!("Failed to convert buffer path {abs_path:?} to lsp Uri"))?;
            if let Some(buffer_handle) = self.buffer_store.read(cx).get_by_path(&project_path) {
                let snapshot = buffer_handle.read(cx).snapshot();
                let buffer = buffer_handle.read(cx);
                let reused_diagnostics = buffer
                    .buffer_diagnostics(Some(server_id))
                    .iter()
                    .filter(|v| merge(&document_uri, &v.diagnostic, cx))
                    .map(|v| {
                        let start = Unclipped(v.range.start.to_point_utf16(&snapshot));
                        let end = Unclipped(v.range.end.to_point_utf16(&snapshot));
                        DiagnosticEntry {
                            range: start..end,
                            diagnostic: v.diagnostic.clone(),
                        }
                    })
                    .collect::<Vec<_>>();

                self.as_local_mut()
                    .context("cannot merge diagnostics on a remote LspStore")?
                    .update_buffer_diagnostics(
                        &buffer_handle,
                        server_id,
                        Some(update.registration_id),
                        update.result_id,
                        update.diagnostics.version,
                        update.diagnostics.diagnostics.clone(),
                        reused_diagnostics.clone(),
                        cx,
                    )?;

                update.diagnostics.diagnostics.extend(reused_diagnostics);
            } else if let Some(local) = self.as_local() {
                let reused_diagnostics = local
                    .diagnostics
                    .get(&worktree_id)
                    .and_then(|diagnostics_for_tree| diagnostics_for_tree.get(&project_path.path))
                    .and_then(|diagnostics_by_server_id| {
                        diagnostics_by_server_id
                            .binary_search_by_key(&server_id, |e| e.0)
                            .ok()
                            .map(|ix| &diagnostics_by_server_id[ix].1)
                    })
                    .into_iter()
                    .flatten()
                    .filter(|v| merge(&document_uri, &v.diagnostic, cx));

                update
                    .diagnostics
                    .diagnostics
                    .extend(reused_diagnostics.cloned());
            }

            let updated = worktree.update(cx, |worktree, cx| {
                self.update_worktree_diagnostics(
                    worktree.id(),
                    server_id,
                    project_path.path.clone(),
                    update.diagnostics.diagnostics,
                    cx,
                )
            })?;
            match updated {
                ControlFlow::Continue(new_summary) => {
                    if let Some((project_id, new_summary)) = new_summary {
                        match &mut diagnostics_summary {
                            Some(diagnostics_summary) => {
                                diagnostics_summary
                                    .more_summaries
                                    .push(proto::DiagnosticSummary {
                                        path: project_path.path.as_ref().to_proto(),
                                        language_server_id: server_id.0 as u64,
                                        error_count: new_summary.error_count,
                                        warning_count: new_summary.warning_count,
                                    })
                            }
                            None => {
                                diagnostics_summary = Some(proto::UpdateDiagnosticSummary {
                                    project_id,
                                    worktree_id: worktree_id.to_proto(),
                                    summary: Some(proto::DiagnosticSummary {
                                        path: project_path.path.as_ref().to_proto(),
                                        language_server_id: server_id.0 as u64,
                                        error_count: new_summary.error_count,
                                        warning_count: new_summary.warning_count,
                                    }),
                                    more_summaries: Vec::new(),
                                })
                            }
                        }
                    }
                    updated_diagnostics_paths
                        .entry(server_id)
                        .or_insert_with(Vec::new)
                        .push(project_path);
                }
                ControlFlow::Break(()) => {}
            }
        }

        if let Some((diagnostics_summary, (downstream_client, _))) =
            diagnostics_summary.zip(self.downstream_client.as_ref())
        {
            downstream_client.send(diagnostics_summary).log_err();
        }
        for (server_id, paths) in updated_diagnostics_paths {
            cx.emit(LspStoreEvent::DiagnosticsUpdated { server_id, paths });
        }
        Ok(())
    }

    fn update_worktree_diagnostics(
        &mut self,
        worktree_id: WorktreeId,
        server_id: LanguageServerId,
        path_in_worktree: Arc<RelPath>,
        diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
        _: &mut Context<Worktree>,
    ) -> Result<ControlFlow<(), Option<(u64, proto::DiagnosticSummary)>>> {
        let local = match &mut self.mode {
            LspStoreMode::Local(local_lsp_store) => local_lsp_store,
            _ => anyhow::bail!("update_worktree_diagnostics called on remote"),
        };

        let summaries_for_tree = self.diagnostic_summaries.entry(worktree_id).or_default();
        let diagnostics_for_tree = local.diagnostics.entry(worktree_id).or_default();
        let summaries_by_server_id = summaries_for_tree
            .entry(path_in_worktree.clone())
            .or_default();

        let old_summary = summaries_by_server_id
            .remove(&server_id)
            .unwrap_or_default();

        let new_summary = DiagnosticSummary::new(&diagnostics);
        if diagnostics.is_empty() {
            if let Some(diagnostics_by_server_id) = diagnostics_for_tree.get_mut(&path_in_worktree)
            {
                if let Ok(ix) = diagnostics_by_server_id.binary_search_by_key(&server_id, |e| e.0) {
                    diagnostics_by_server_id.remove(ix);
                }
                if diagnostics_by_server_id.is_empty() {
                    diagnostics_for_tree.remove(&path_in_worktree);
                }
            }
        } else {
            summaries_by_server_id.insert(server_id, new_summary);
            let diagnostics_by_server_id = diagnostics_for_tree
                .entry(path_in_worktree.clone())
                .or_default();
            match diagnostics_by_server_id.binary_search_by_key(&server_id, |e| e.0) {
                Ok(ix) => {
                    diagnostics_by_server_id[ix] = (server_id, diagnostics);
                }
                Err(ix) => {
                    diagnostics_by_server_id.insert(ix, (server_id, diagnostics));
                }
            }
        }

        if !old_summary.is_empty() || !new_summary.is_empty() {
            if let Some((_, project_id)) = &self.downstream_client {
                Ok(ControlFlow::Continue(Some((
                    *project_id,
                    proto::DiagnosticSummary {
                        path: path_in_worktree.to_proto(),
                        language_server_id: server_id.0 as u64,
                        error_count: new_summary.error_count as u32,
                        warning_count: new_summary.warning_count as u32,
                    },
                ))))
            } else {
                Ok(ControlFlow::Continue(None))
            }
        } else {
            Ok(ControlFlow::Break(()))
        }
    }

    pub fn open_buffer_for_symbol(
        &mut self,
        symbol: &Symbol,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        if let Some((client, project_id)) = self.upstream_client() {
            let request = client.request(proto::OpenBufferForSymbol {
                project_id,
                symbol: Some(Self::serialize_symbol(symbol)),
            });
            cx.spawn(async move |this, cx| {
                let response = request.await?;
                let buffer_id = BufferId::new(response.buffer_id)?;
                this.update(cx, |this, cx| this.wait_for_remote_buffer(buffer_id, cx))?
                    .await
            })
        } else if let Some(local) = self.as_local() {
            let is_valid = local.language_server_ids.iter().any(|(seed, state)| {
                seed.worktree_id == symbol.source_worktree_id
                    && state.id == symbol.source_language_server_id
                    && symbol.language_server_name == seed.name
            });
            if !is_valid {
                return Task::ready(Err(anyhow!(
                    "language server for worktree and language not found"
                )));
            };

            let symbol_abs_path = match &symbol.path {
                SymbolLocation::InProject(project_path) => self
                    .worktree_store
                    .read(cx)
                    .absolutize(&project_path, cx)
                    .context("no such worktree"),
                SymbolLocation::OutsideProject {
                    abs_path,
                    signature: _,
                } => Ok(abs_path.to_path_buf()),
            };
            let symbol_abs_path = match symbol_abs_path {
                Ok(abs_path) => abs_path,
                Err(err) => return Task::ready(Err(err)),
            };
            let symbol_uri = if let Ok(uri) = lsp::Uri::from_file_path(symbol_abs_path) {
                uri
            } else {
                return Task::ready(Err(anyhow!("invalid symbol path")));
            };

            self.open_local_buffer_via_lsp(symbol_uri, symbol.source_language_server_id, cx)
        } else {
            Task::ready(Err(anyhow!("no upstream client or local store")))
        }
    }

    pub(crate) fn open_local_buffer_via_lsp(
        &mut self,
        abs_path: lsp::Uri,
        language_server_id: LanguageServerId,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        let path_style = self.worktree_store.read(cx).path_style();
        cx.spawn(async move |lsp_store, cx| {
            // Escape percent-encoded string.
            let current_scheme = abs_path.scheme().to_owned();
            // Uri is immutable, so we can't modify the scheme

            let abs_path = abs_path
                .to_file_path_ext(path_style)
                .map_err(|()| anyhow!("can't convert URI to path"))?;
            let p = abs_path.clone();
            let yarn_worktree = lsp_store
                .update(cx, move |lsp_store, cx| match lsp_store.as_local() {
                    Some(local_lsp_store) => local_lsp_store.yarn.update(cx, |_, cx| {
                        cx.spawn(async move |this, cx| {
                            let t = this
                                .update(cx, |this, cx| this.process_path(&p, &current_scheme, cx))
                                .ok()?;
                            t.await
                        })
                    }),
                    None => Task::ready(None),
                })?
                .await;
            let (worktree_root_target, known_relative_path) =
                if let Some((zip_root, relative_path)) = yarn_worktree {
                    (zip_root, Some(relative_path))
                } else {
                    (Arc::<Path>::from(abs_path.as_path()), None)
                };
            let worktree = lsp_store.update(cx, |lsp_store, cx| {
                lsp_store.worktree_store.update(cx, |worktree_store, cx| {
                    worktree_store.find_worktree(&worktree_root_target, cx)
                })
            })?;
            let (worktree, relative_path, source_ws) = if let Some(result) = worktree {
                let relative_path = known_relative_path.unwrap_or_else(|| result.1.clone());
                (result.0, relative_path, None)
            } else {
                let worktree = lsp_store
                    .update(cx, |lsp_store, cx| {
                        lsp_store.worktree_store.update(cx, |worktree_store, cx| {
                            worktree_store.create_worktree(&worktree_root_target, false, cx)
                        })
                    })?
                    .await?;
                let worktree_root = worktree.read_with(cx, |worktree, _| worktree.abs_path());
                let source_ws = if worktree.read_with(cx, |worktree, _| worktree.is_local()) {
                    lsp_store
                        .update(cx, |lsp_store, cx| {
                            if let Some(local) = lsp_store.as_local_mut() {
                                local.register_language_server_for_invisible_worktree(
                                    &worktree,
                                    language_server_id,
                                    cx,
                                )
                            }
                            match lsp_store.language_server_statuses.get(&language_server_id) {
                                Some(status) => status.worktree,
                                None => None,
                            }
                        })
                        .ok()
                        .flatten()
                        .zip(Some(worktree_root.clone()))
                } else {
                    None
                };
                let relative_path = if let Some(known_path) = known_relative_path {
                    known_path
                } else {
                    RelPath::new(abs_path.strip_prefix(worktree_root)?, PathStyle::local())?
                        .into_arc()
                };
                (worktree, relative_path, source_ws)
            };
            let project_path = ProjectPath {
                worktree_id: worktree.read_with(cx, |worktree, _| worktree.id()),
                path: relative_path,
            };
            let buffer = lsp_store
                .update(cx, |lsp_store, cx| {
                    lsp_store.buffer_store().update(cx, |buffer_store, cx| {
                        buffer_store.open_buffer(project_path, cx)
                    })
                })?
                .await?;
            // we want to adhere to the read-only settings of the worktree we came from in case we opened an invisible one
            if let Some((source_ws, worktree_root)) = source_ws {
                buffer.update(cx, |buffer, cx| {
                    let settings = WorktreeSettings::get(
                        Some(
                            (&ProjectPath {
                                worktree_id: source_ws,
                                path: Arc::from(RelPath::empty()),
                            })
                                .into(),
                        ),
                        cx,
                    );
                    let is_read_only = settings.is_std_path_read_only(&worktree_root);
                    if is_read_only {
                        buffer.set_capability(Capability::ReadOnly, cx);
                    }
                });
            }
            Ok(buffer)
        })
    }

    fn local_lsp_servers_for_buffer(
        &self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Vec<LanguageServerId> {
        let Some(local) = self.as_local() else {
            return Vec::new();
        };

        let snapshot = buffer.read(cx).snapshot();

        buffer.update(cx, |buffer, cx| {
            local
                .language_servers_for_buffer(buffer, cx)
                .map(|(_, server)| server.server_id())
                .filter(|server_id| {
                    self.as_local().is_none_or(|local| {
                        local
                            .buffers_opened_in_servers
                            .get(&snapshot.remote_id())
                            .is_some_and(|servers| servers.contains(server_id))
                    })
                })
                .collect()
        })
    }

    fn request_multiple_lsp_locally<P, R>(
        &mut self,
        buffer: &Entity<Buffer>,
        position: Option<P>,
        request: R,
        cx: &mut Context<Self>,
    ) -> Task<Vec<(LanguageServerId, R::Response)>>
    where
        P: ToOffset,
        R: LspCommand + Clone,
        <R::LspRequest as lsp::request::Request>::Result: Send,
        <R::LspRequest as lsp::request::Request>::Params: Send,
    {
        let Some(local) = self.as_local() else {
            return Task::ready(Vec::new());
        };

        let snapshot = buffer.read(cx).snapshot();
        let scope = position.and_then(|position| snapshot.language_scope_at(position));

        let server_ids = buffer.update(cx, |buffer, cx| {
            local
                .language_servers_for_buffer(buffer, cx)
                .filter(|(adapter, _)| {
                    scope
                        .as_ref()
                        .map(|scope| scope.language_allowed(&adapter.name))
                        .unwrap_or(true)
                })
                .map(|(_, server)| server.server_id())
                .filter(|server_id| {
                    self.as_local().is_none_or(|local| {
                        local
                            .buffers_opened_in_servers
                            .get(&snapshot.remote_id())
                            .is_some_and(|servers| servers.contains(server_id))
                    })
                })
                .collect::<Vec<_>>()
        });

        let mut response_results = server_ids
            .into_iter()
            .map(|server_id| {
                let task = self.request_lsp(
                    buffer.clone(),
                    LanguageServerToQuery::Other(server_id),
                    request.clone(),
                    cx,
                );
                async move { (server_id, task.await) }
            })
            .collect::<FuturesUnordered<_>>();

        cx.background_spawn(async move {
            let mut responses = Vec::with_capacity(response_results.len());
            while let Some((server_id, response_result)) = response_results.next().await {
                match response_result {
                    Ok(response) => responses.push((server_id, response)),
                    // rust-analyzer likes to error with this when its still loading up
                    Err(e) if format!("{e:#}").ends_with("content modified") => (),
                    Err(e) => log::error!("Error handling response for request {request:?}: {e:#}"),
                }
            }
            responses
        })
    }

    async fn handle_lsp_get_completions(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetCompletions>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetCompletionsResponse> {
        let sender_id = envelope.original_sender_id().unwrap_or_default();

        let buffer_id = GetCompletions::buffer_id_from_proto(&envelope.payload)?;
        let buffer_handle = this.update(&mut cx, |this, cx| {
            this.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        let request = GetCompletions::from_proto(
            envelope.payload,
            this.clone(),
            buffer_handle.clone(),
            cx.clone(),
        )
        .await?;

        let server_to_query = match request.server_id {
            Some(server_id) => LanguageServerToQuery::Other(server_id),
            None => LanguageServerToQuery::FirstCapable,
        };

        let response = this
            .update(&mut cx, |this, cx| {
                this.request_lsp(buffer_handle.clone(), server_to_query, request, cx)
            })
            .await?;
        this.update(&mut cx, |this, cx| {
            Ok(GetCompletions::response_to_proto(
                response,
                this,
                sender_id,
                &buffer_handle.read(cx).version(),
                cx,
            ))
        })
    }

    async fn handle_lsp_command<T: LspCommand>(
        this: Entity<Self>,
        envelope: TypedEnvelope<T::ProtoRequest>,
        mut cx: AsyncApp,
    ) -> Result<<T::ProtoRequest as proto::RequestMessage>::Response>
    where
        <T::LspRequest as lsp::request::Request>::Params: Send,
        <T::LspRequest as lsp::request::Request>::Result: Send,
    {
        let sender_id = envelope.original_sender_id().unwrap_or_default();
        let buffer_id = T::buffer_id_from_proto(&envelope.payload)?;
        let buffer_handle = this.update(&mut cx, |this, cx| {
            this.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        let request = T::from_proto(
            envelope.payload,
            this.clone(),
            buffer_handle.clone(),
            cx.clone(),
        )
        .await?;
        let response = this
            .update(&mut cx, |this, cx| {
                this.request_lsp(
                    buffer_handle.clone(),
                    LanguageServerToQuery::FirstCapable,
                    request,
                    cx,
                )
            })
            .await?;
        this.update(&mut cx, |this, cx| {
            Ok(T::response_to_proto(
                response,
                this,
                sender_id,
                &buffer_handle.read(cx).version(),
                cx,
            ))
        })
    }

    async fn handle_lsp_query(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::LspQuery>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        use proto::lsp_query::Request;
        let sender_id = envelope.original_sender_id().unwrap_or_default();
        let lsp_query = envelope.payload;
        let lsp_request_id = LspRequestId(lsp_query.lsp_request_id);
        let server_id = lsp_query.server_id.map(LanguageServerId::from_proto);
        match lsp_query.request.context("invalid LSP query request")? {
            Request::GetReferences(get_references) => {
                let position = get_references.position.clone().and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetReferences>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_references,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDocumentColor(get_document_color) => {
                Self::query_lsp_locally::<GetDocumentColor>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_document_color,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetFoldingRanges(get_folding_ranges) => {
                Self::query_lsp_locally::<GetFoldingRanges>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_folding_ranges,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDocumentSymbols(get_document_symbols) => {
                Self::query_lsp_locally::<GetDocumentSymbols>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_document_symbols,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDocumentLinks(get_document_links) => {
                let (buffer_version, buffer) = Self::wait_for_buffer_version::<GetDocumentLinks>(
                    &lsp_store,
                    &get_document_links,
                    &mut cx,
                )
                .await?;
                lsp_store.update(&mut cx, |lsp_store, cx| {
                    let document_links_task = lsp_store.fetch_document_links(&buffer, cx);
                    let fetch_task = cx.background_spawn(async move {
                        document_links_task
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(server_id, links)| {
                                (server_id, links.into_values().collect::<Vec<_>>())
                            })
                            .collect()
                    });
                    lsp_store.serve_lsp_query::<GetDocumentLinks>(
                        server_id,
                        sender_id,
                        lsp_request_id,
                        &buffer,
                        buffer_version,
                        fetch_task,
                        cx,
                    );
                });
            }
            Request::GetHover(get_hover) => {
                let position = get_hover.position.clone().and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetHover>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_hover,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetCodeActions(get_code_actions) => {
                Self::query_lsp_locally::<GetCodeActions>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_code_actions,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetSignatureHelp(get_signature_help) => {
                let position = get_signature_help
                    .position
                    .clone()
                    .and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetSignatureHelp>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_signature_help,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetCodeLens(get_code_lens) => {
                Self::query_lsp_locally::<GetCodeLens>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_code_lens,
                    None,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDefinition(get_definition) => {
                let position = get_definition.position.clone().and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetDefinitions>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_definition,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetDeclaration(get_declaration) => {
                let position = get_declaration
                    .position
                    .clone()
                    .and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetDeclarations>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_declaration,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetTypeDefinition(get_type_definition) => {
                let position = get_type_definition
                    .position
                    .clone()
                    .and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetTypeDefinitions>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_type_definition,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::GetImplementation(get_implementation) => {
                let position = get_implementation
                    .position
                    .clone()
                    .and_then(deserialize_anchor);
                Self::query_lsp_locally::<GetImplementations>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    get_implementation,
                    position,
                    &mut cx,
                )
                .await?;
            }
            Request::InlayHints(inlay_hints) => {
                let query_start = inlay_hints
                    .start
                    .clone()
                    .and_then(deserialize_anchor)
                    .context("invalid inlay hints range start")?;
                let query_end = inlay_hints
                    .end
                    .clone()
                    .and_then(deserialize_anchor)
                    .context("invalid inlay hints range end")?;
                Self::deduplicate_range_based_lsp_requests::<InlayHints>(
                    &lsp_store,
                    server_id,
                    lsp_request_id,
                    &inlay_hints,
                    query_start..query_end,
                    &mut cx,
                )
                .await
                .context("preparing inlay hints request")?;
                Self::query_lsp_locally::<InlayHints>(
                    lsp_store,
                    server_id,
                    sender_id,
                    lsp_request_id,
                    inlay_hints,
                    None,
                    &mut cx,
                )
                .await
                .context("querying for inlay hints")?
            }
            //////////////////////////////
            // Below are LSP queries that need to fetch more data,
            // hence cannot just proxy the request to language server with `query_lsp_locally`.
            Request::GetDocumentDiagnostics(get_document_diagnostics) => {
                let (_, buffer) = Self::wait_for_buffer_version::<GetDocumentDiagnostics>(
                    &lsp_store,
                    &get_document_diagnostics,
                    &mut cx,
                )
                .await?;
                lsp_store.update(&mut cx, |lsp_store, cx| {
                    let lsp_data = lsp_store.latest_lsp_data(&buffer, cx);
                    let key = LspKey {
                        request_type: TypeId::of::<GetDocumentDiagnostics>(),
                        server_queried: server_id,
                    };
                    if <GetDocumentDiagnostics as LspCommand>::ProtoRequest::stop_previous_requests(
                    ) {
                        if let Some(lsp_requests) = lsp_data.lsp_requests.get_mut(&key) {
                            lsp_requests.clear();
                        };
                    }

                    lsp_data.lsp_requests.entry(key).or_default().insert(
                        lsp_request_id,
                        cx.spawn(async move |lsp_store, cx| {
                            let diagnostics_pull = lsp_store
                                .update(cx, |lsp_store, cx| {
                                    lsp_store.pull_diagnostics_for_buffer(buffer, cx)
                                })
                                .ok();
                            if let Some(diagnostics_pull) = diagnostics_pull {
                                match diagnostics_pull.await {
                                    Ok(()) => {}
                                    Err(e) => log::error!("Failed to pull diagnostics: {e:#}"),
                                };
                            }
                        }),
                    );
                });
            }
            Request::SemanticTokens(semantic_tokens) => {
                let (buffer_version, buffer) = Self::wait_for_buffer_version::<SemanticTokensFull>(
                    &lsp_store,
                    &semantic_tokens,
                    &mut cx,
                )
                .await?;
                let for_server = semantic_tokens.for_server.map(LanguageServerId::from_proto);
                lsp_store.update(&mut cx, |lsp_store, cx| {
                    let semantic_tokens_task =
                        lsp_store.fetch_semantic_tokens_for_buffer(&buffer, for_server, cx);
                    lsp_store.serve_lsp_query::<SemanticTokensFull>(
                        server_id,
                        sender_id,
                        lsp_request_id,
                        &buffer,
                        buffer_version,
                        cx.background_spawn(async move {
                            semantic_tokens_task.await.unwrap_or_default()
                        }),
                        cx,
                    );
                });
            }
        }
        Ok(proto::Ack {})
    }

    async fn handle_lsp_query_response(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::LspQueryResponse>,
        cx: AsyncApp,
    ) -> Result<()> {
        lsp_store.read_with(&cx, |lsp_store, _| {
            if let Some((upstream_client, _)) = lsp_store.upstream_client() {
                upstream_client.handle_lsp_response(envelope.clone());
            }
        });
        Ok(())
    }

    async fn handle_apply_code_action(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ApplyCodeAction>,
        mut cx: AsyncApp,
    ) -> Result<proto::ApplyCodeActionResponse> {
        let sender_id = envelope.original_sender_id().unwrap_or_default();
        let action =
            Self::deserialize_code_action(envelope.payload.action.context("invalid action")?)?;
        let apply_code_action = this.update(&mut cx, |this, cx| {
            let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
            let buffer = this.buffer_store.read(cx).get_existing(buffer_id)?;
            anyhow::Ok(this.apply_code_action(buffer, action, false, cx))
        })?;

        let project_transaction = apply_code_action.await?;
        let project_transaction = this.update(&mut cx, |this, cx| {
            this.buffer_store.update(cx, |buffer_store, cx| {
                buffer_store.serialize_project_transaction_for_peer(
                    project_transaction,
                    sender_id,
                    cx,
                )
            })
        });
        Ok(proto::ApplyCodeActionResponse {
            transaction: Some(project_transaction),
        })
    }

    async fn handle_register_buffer_with_language_servers(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RegisterBufferWithLanguageServers>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
        let peer_id = envelope.original_sender_id.unwrap_or(envelope.sender_id);
        this.update(&mut cx, |this, cx| {
            if let Some((upstream_client, upstream_project_id)) = this.upstream_client() {
                return upstream_client.send(proto::RegisterBufferWithLanguageServers {
                    project_id: upstream_project_id,
                    buffer_id: buffer_id.to_proto(),
                    only_servers: envelope.payload.only_servers,
                });
            }

            let Some(buffer) = this.buffer_store().read(cx).get(buffer_id) else {
                anyhow::bail!("buffer is not open");
            };

            let handle = this.register_buffer_with_language_servers(
                &buffer,
                envelope
                    .payload
                    .only_servers
                    .into_iter()
                    .filter_map(|selector| {
                        Some(match selector.selector? {
                            proto::language_server_selector::Selector::ServerId(server_id) => {
                                LanguageServerSelector::Id(LanguageServerId::from_proto(server_id))
                            }
                            proto::language_server_selector::Selector::Name(name) => {
                                LanguageServerSelector::Name(LanguageServerName(
                                    SharedString::from(name),
                                ))
                            }
                        })
                    })
                    .collect(),
                false,
                cx,
            );
            // Pull diagnostics for the buffer even if it was already registered.
            // This is needed to make test_streamed_lsp_pull_diagnostics pass,
            // but it's unclear if we need it.
            this.pull_diagnostics_for_buffer(buffer.clone(), cx)
                .detach();
            this.buffer_store().update(cx, |buffer_store, _| {
                buffer_store.register_shared_lsp_handle(peer_id, buffer_id, handle);
            });

            Ok(())
        })?;
        Ok(proto::Ack {})
    }

    async fn handle_rename_project_entry(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RenameProjectEntry>,
        mut cx: AsyncApp,
    ) -> Result<proto::ProjectEntryResponse> {
        let entry_id = ProjectEntryId::from_proto(envelope.payload.entry_id);
        let new_worktree_id = WorktreeId::from_proto(envelope.payload.new_worktree_id);
        let new_path =
            RelPath::from_proto(&envelope.payload.new_path).context("invalid relative path")?;

        let (worktree_store, old_worktree, new_worktree, old_entry) = this
            .update(&mut cx, |this, cx| {
                let (worktree, entry) = this
                    .worktree_store
                    .read(cx)
                    .worktree_and_entry_for_id(entry_id, cx)?;
                let new_worktree = this
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(new_worktree_id, cx)?;
                Some((
                    this.worktree_store.clone(),
                    worktree,
                    new_worktree,
                    entry.clone(),
                ))
            })
            .context("worktree not found")?;
        let (old_abs_path, old_worktree_id) = old_worktree.read_with(&cx, |worktree, _| {
            (worktree.absolutize(&old_entry.path), worktree.id())
        });
        let new_abs_path =
            new_worktree.read_with(&cx, |worktree, _| worktree.absolutize(&new_path));

        let _transaction = Self::will_rename_entry(
            this.downgrade(),
            old_worktree_id,
            &old_abs_path,
            &new_abs_path,
            old_entry.is_dir(),
            cx.clone(),
        )
        .await;
        let response = WorktreeStore::handle_rename_project_entry(
            worktree_store,
            envelope.payload,
            cx.clone(),
        )
        .await;
        this.read_with(&cx, |this, _| {
            this.did_rename_entry(
                old_worktree_id,
                &old_abs_path,
                &new_abs_path,
                old_entry.is_dir(),
            );
        });
        response
    }

    async fn handle_update_diagnostic_summary(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateDiagnosticSummary>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |lsp_store, cx| {
            let worktree_id = WorktreeId::from_proto(envelope.payload.worktree_id);
            let mut updated_diagnostics_paths = HashMap::default();
            let mut diagnostics_summary = None::<proto::UpdateDiagnosticSummary>;
            for message_summary in envelope
                .payload
                .summary
                .into_iter()
                .chain(envelope.payload.more_summaries)
            {
                let project_path = ProjectPath {
                    worktree_id,
                    path: RelPath::from_proto(&message_summary.path).context("invalid path")?,
                };
                let path = project_path.path.clone();
                let server_id = LanguageServerId(message_summary.language_server_id as usize);
                let summary = DiagnosticSummary {
                    error_count: message_summary.error_count as usize,
                    warning_count: message_summary.warning_count as usize,
                };

                if summary.is_empty() {
                    if let Some(worktree_summaries) =
                        lsp_store.diagnostic_summaries.get_mut(&worktree_id)
                        && let Some(summaries) = worktree_summaries.get_mut(&path)
                    {
                        summaries.remove(&server_id);
                        if summaries.is_empty() {
                            worktree_summaries.remove(&path);
                        }
                    }
                } else {
                    lsp_store
                        .diagnostic_summaries
                        .entry(worktree_id)
                        .or_default()
                        .entry(path)
                        .or_default()
                        .insert(server_id, summary);
                }

                if let Some((_, project_id)) = &lsp_store.downstream_client {
                    match &mut diagnostics_summary {
                        Some(diagnostics_summary) => {
                            diagnostics_summary
                                .more_summaries
                                .push(proto::DiagnosticSummary {
                                    path: project_path.path.as_ref().to_proto(),
                                    language_server_id: server_id.0 as u64,
                                    error_count: summary.error_count as u32,
                                    warning_count: summary.warning_count as u32,
                                })
                        }
                        None => {
                            diagnostics_summary = Some(proto::UpdateDiagnosticSummary {
                                project_id: *project_id,
                                worktree_id: worktree_id.to_proto(),
                                summary: Some(proto::DiagnosticSummary {
                                    path: project_path.path.as_ref().to_proto(),
                                    language_server_id: server_id.0 as u64,
                                    error_count: summary.error_count as u32,
                                    warning_count: summary.warning_count as u32,
                                }),
                                more_summaries: Vec::new(),
                            })
                        }
                    }
                }
                updated_diagnostics_paths
                    .entry(server_id)
                    .or_insert_with(Vec::new)
                    .push(project_path);
            }

            if let Some((diagnostics_summary, (downstream_client, _))) =
                diagnostics_summary.zip(lsp_store.downstream_client.as_ref())
            {
                downstream_client.send(diagnostics_summary).log_err();
            }
            for (server_id, paths) in updated_diagnostics_paths {
                cx.emit(LspStoreEvent::DiagnosticsUpdated { server_id, paths });
            }
            Ok(())
        })
    }

    pub fn disk_based_diagnostics_started(
        &mut self,
        language_server_id: LanguageServerId,
        cx: &mut Context<Self>,
    ) {
        if let Some(language_server_status) =
            self.language_server_statuses.get_mut(&language_server_id)
        {
            language_server_status.has_pending_diagnostic_updates = true;
        }

        cx.emit(LspStoreEvent::DiskBasedDiagnosticsStarted { language_server_id });
        cx.emit(LspStoreEvent::LanguageServerUpdate {
            language_server_id,
            name: self
                .language_server_adapter_for_id(language_server_id)
                .map(|adapter| adapter.name()),
            message: proto::update_language_server::Variant::DiskBasedDiagnosticsUpdating(
                Default::default(),
            ),
        })
    }

    pub fn disk_based_diagnostics_finished(
        &mut self,
        language_server_id: LanguageServerId,
        cx: &mut Context<Self>,
    ) {
        if let Some(language_server_status) =
            self.language_server_statuses.get_mut(&language_server_id)
        {
            language_server_status.has_pending_diagnostic_updates = false;
        }

        cx.emit(LspStoreEvent::DiskBasedDiagnosticsFinished { language_server_id });
        cx.emit(LspStoreEvent::LanguageServerUpdate {
            language_server_id,
            name: self
                .language_server_adapter_for_id(language_server_id)
                .map(|adapter| adapter.name()),
            message: proto::update_language_server::Variant::DiskBasedDiagnosticsUpdated(
                Default::default(),
            ),
        })
    }

    // After saving a buffer using a language server that doesn't provide a disk-based progress token,
    // kick off a timer that will reset every time the buffer is saved. If the timer eventually fires,
    // simulate disk-based diagnostics being finished so that other pieces of UI (e.g., project
    // diagnostics view, diagnostic status bar) can update. We don't emit an event right away because
    // the language server might take some time to publish diagnostics.
    fn simulate_disk_based_diagnostics_events_if_needed(
        &mut self,
        language_server_id: LanguageServerId,
        cx: &mut Context<Self>,
    ) {
        const DISK_BASED_DIAGNOSTICS_DEBOUNCE: Duration = Duration::from_secs(1);

        let Some(LanguageServerState::Running {
            simulate_disk_based_diagnostics_completion,
            adapter,
            ..
        }) = self
            .as_local_mut()
            .and_then(|local_store| local_store.language_servers.get_mut(&language_server_id))
        else {
            return;
        };

        if adapter.disk_based_diagnostics_progress_token.is_some() {
            return;
        }

        let prev_task =
            simulate_disk_based_diagnostics_completion.replace(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(DISK_BASED_DIAGNOSTICS_DEBOUNCE)
                    .await;

                this.update(cx, |this, cx| {
                    this.disk_based_diagnostics_finished(language_server_id, cx);

                    if let Some(LanguageServerState::Running {
                        simulate_disk_based_diagnostics_completion,
                        ..
                    }) = this.as_local_mut().and_then(|local_store| {
                        local_store.language_servers.get_mut(&language_server_id)
                    }) {
                        *simulate_disk_based_diagnostics_completion = None;
                    }
                })
                .ok();
            }));

        if prev_task.is_none() {
            self.disk_based_diagnostics_started(language_server_id, cx);
        }
    }

    pub fn language_server_statuses(
        &self,
    ) -> impl DoubleEndedIterator<Item = (LanguageServerId, &LanguageServerStatus)> {
        self.language_server_statuses
            .iter()
            .map(|(key, value)| (*key, value))
    }

    #[cfg(feature = "test-support")]
    pub fn has_language_server_seed_for_worktree(&self, worktree_id: WorktreeId) -> bool {
        self.as_local().is_some_and(|local| {
            local
                .language_server_ids
                .keys()
                .any(|seed| seed.worktree_id == worktree_id)
        })
    }

    pub(super) fn did_rename_entry(
        &self,
        worktree_id: WorktreeId,
        old_path: &Path,
        new_path: &Path,
        is_dir: bool,
    ) {
        maybe!({
            let local_store = self.as_local()?;

            let old_uri = lsp::Uri::from_file_path(old_path)
                .ok()
                .map(|uri| uri.to_string())?;
            let new_uri = lsp::Uri::from_file_path(new_path)
                .ok()
                .map(|uri| uri.to_string())?;

            for language_server in local_store.language_servers_for_worktree(worktree_id) {
                let Some(filter) = local_store
                    .language_server_paths_watched_for_rename
                    .get(&language_server.server_id())
                else {
                    continue;
                };

                if filter.should_send_did_rename(&old_uri, is_dir) {
                    language_server
                        .notify::<DidRenameFiles>(RenameFilesParams {
                            files: vec![FileRename {
                                old_uri: old_uri.clone(),
                                new_uri: new_uri.clone(),
                            }],
                        })
                        .ok();
                }
            }
            Some(())
        });
    }

    pub(super) fn will_rename_entry(
        this: WeakEntity<Self>,
        worktree_id: WorktreeId,
        old_path: &Path,
        new_path: &Path,
        is_dir: bool,
        cx: AsyncApp,
    ) -> Task<ProjectTransaction> {
        let old_uri = lsp::Uri::from_file_path(old_path)
            .ok()
            .map(|uri| uri.to_string());
        let new_uri = lsp::Uri::from_file_path(new_path)
            .ok()
            .map(|uri| uri.to_string());
        cx.spawn(async move |cx| {
            let mut tasks = vec![];
            this.update(cx, |this, cx| {
                let local_store = this.as_local()?;
                let old_uri = old_uri?;
                let new_uri = new_uri?;
                for language_server in local_store.language_servers_for_worktree(worktree_id) {
                    let Some(filter) = local_store
                        .language_server_paths_watched_for_rename
                        .get(&language_server.server_id())
                    else {
                        continue;
                    };

                    if !filter.should_send_will_rename(&old_uri, is_dir) {
                        continue;
                    }
                    let request_timeout = ProjectSettings::get_global(cx)
                        .global_lsp_settings
                        .get_request_timeout();

                    let apply_edit = cx.spawn({
                        let old_uri = old_uri.clone();
                        let new_uri = new_uri.clone();
                        let language_server = language_server.clone();
                        async move |this, cx| {
                            let edit = language_server
                                .request::<WillRenameFiles>(
                                    RenameFilesParams {
                                        files: vec![FileRename { old_uri, new_uri }],
                                    },
                                    request_timeout,
                                )
                                .await
                                .into_response()
                                .context("will rename files")
                                .log_err()
                                .flatten()?;

                            LocalLspStore::deserialize_workspace_edit(
                                this.upgrade()?,
                                edit,
                                false,
                                language_server.clone(),
                                cx,
                            )
                            .await
                            .ok()
                        }
                    });
                    tasks.push(apply_edit);
                }
                Some(())
            })
            .ok()
            .flatten();
            let mut merged_transaction = ProjectTransaction::default();
            for task in tasks {
                // Await on tasks sequentially so that the order of application of edits is deterministic
                // (at least with regards to the order of registration of language servers)
                if let Some(transaction) = task.await {
                    for (buffer, buffer_transaction) in transaction.0 {
                        merged_transaction.0.insert(buffer, buffer_transaction);
                    }
                }
            }
            merged_transaction
        })
    }

    fn lsp_notify_abs_paths_changed(
        &mut self,
        server_id: LanguageServerId,
        changes: Vec<PathEvent>,
    ) {
        maybe!({
            let server = self.language_server_for_id(server_id)?;
            let changes = changes
                .into_iter()
                .filter_map(|event| {
                    let typ = match event.kind? {
                        PathEventKind::Created => lsp::FileChangeType::CREATED,
                        PathEventKind::Removed => lsp::FileChangeType::DELETED,
                        PathEventKind::Changed | PathEventKind::Rescan => {
                            lsp::FileChangeType::CHANGED
                        }
                    };
                    Some(lsp::FileEvent {
                        uri: file_path_to_lsp_url(&event.path).log_err()?,
                        typ,
                    })
                })
                .collect::<Vec<_>>();
            if !changes.is_empty() {
                server
                    .notify::<lsp::notification::DidChangeWatchedFiles>(
                        lsp::DidChangeWatchedFilesParams { changes },
                    )
                    .ok();
            }
            Some(())
        });
    }

    pub fn language_server_for_id(&self, id: LanguageServerId) -> Option<Arc<LanguageServer>> {
        self.as_local()?.language_server_for_id(id)
    }

    async fn handle_resolve_completion_documentation(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ResolveCompletionDocumentation>,
        mut cx: AsyncApp,
    ) -> Result<proto::ResolveCompletionDocumentationResponse> {
        let lsp_completion = serde_json::from_slice(&envelope.payload.lsp_completion)?;

        let completion = this
            .read_with(&cx, |this, cx| {
                let id = LanguageServerId(envelope.payload.language_server_id as usize);
                let server = this
                    .language_server_for_id(id)
                    .with_context(|| format!("No language server {id}"))?;

                let request_timeout = ProjectSettings::get_global(cx)
                    .global_lsp_settings
                    .get_request_timeout();

                anyhow::Ok(cx.background_spawn(async move {
                    let can_resolve = server
                        .capabilities()
                        .completion_provider
                        .as_ref()
                        .and_then(|options| options.resolve_provider)
                        .unwrap_or(false);
                    if can_resolve {
                        server
                            .request::<lsp::request::ResolveCompletionItem>(
                                lsp_completion,
                                request_timeout,
                            )
                            .await
                            .into_response()
                            .context("resolve completion item")
                    } else {
                        anyhow::Ok(lsp_completion)
                    }
                }))
            })?
            .await?;

        let mut documentation_is_markdown = false;
        let lsp_completion = serde_json::to_string(&completion)?.into_bytes();
        let documentation = match completion.documentation {
            Some(lsp::Documentation::String(text)) => text,

            Some(lsp::Documentation::MarkupContent(lsp::MarkupContent { kind, value })) => {
                documentation_is_markdown = kind == lsp::MarkupKind::Markdown;
                value
            }

            _ => String::new(),
        };

        // If we have a new buffer_id, that means we're talking to a new client
        // and want to check for new text_edits in the completion too.
        let mut old_replace_start = None;
        let mut old_replace_end = None;
        let mut old_insert_start = None;
        let mut old_insert_end = None;
        let mut new_text = String::default();
        if let Ok(buffer_id) = BufferId::new(envelope.payload.buffer_id) {
            let buffer_snapshot = this.update(&mut cx, |this, cx| {
                let buffer = this.buffer_store.read(cx).get_existing(buffer_id)?;
                anyhow::Ok(buffer.read(cx).snapshot())
            })?;

            if let Some(text_edit) = completion.text_edit.as_ref() {
                let edit = parse_completion_text_edit(text_edit, &buffer_snapshot);

                if let Some(mut edit) = edit {
                    LineEnding::normalize(&mut edit.new_text);

                    new_text = edit.new_text;
                    old_replace_start = Some(serialize_anchor(&edit.replace_range.start));
                    old_replace_end = Some(serialize_anchor(&edit.replace_range.end));
                    if let Some(insert_range) = edit.insert_range {
                        old_insert_start = Some(serialize_anchor(&insert_range.start));
                        old_insert_end = Some(serialize_anchor(&insert_range.end));
                    }
                }
            }
        }

        Ok(proto::ResolveCompletionDocumentationResponse {
            documentation,
            documentation_is_markdown,
            old_replace_start,
            old_replace_end,
            new_text,
            lsp_completion,
            old_insert_start,
            old_insert_end,
        })
    }

    async fn handle_on_type_formatting(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::OnTypeFormatting>,
        mut cx: AsyncApp,
    ) -> Result<proto::OnTypeFormattingResponse> {
        let on_type_formatting = this.update(&mut cx, |this, cx| {
            let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
            let buffer = this.buffer_store.read(cx).get_existing(buffer_id)?;
            let position = envelope
                .payload
                .position
                .and_then(deserialize_anchor)
                .context("invalid position")?;
            anyhow::Ok(this.apply_on_type_formatting(
                buffer,
                position,
                envelope.payload.trigger.clone(),
                cx,
            ))
        })?;

        let transaction = on_type_formatting
            .await?
            .as_ref()
            .map(language::proto::serialize_transaction);
        Ok(proto::OnTypeFormattingResponse { transaction })
    }

    async fn handle_pull_workspace_diagnostics(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::PullWorkspaceDiagnostics>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let server_id = LanguageServerId::from_proto(envelope.payload.server_id);
        lsp_store.update(&mut cx, |lsp_store, _| {
            lsp_store.pull_workspace_diagnostics(server_id);
        });
        Ok(proto::Ack {})
    }

    async fn handle_open_buffer_for_symbol(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::OpenBufferForSymbol>,
        mut cx: AsyncApp,
    ) -> Result<proto::OpenBufferForSymbolResponse> {
        let peer_id = envelope.original_sender_id().unwrap_or_default();
        let symbol = envelope.payload.symbol.context("invalid symbol")?;
        let symbol = Self::deserialize_symbol(symbol)?;
        this.read_with(&cx, |this, _| {
            if let SymbolLocation::OutsideProject {
                abs_path,
                signature,
            } = &symbol.path
            {
                let new_signature = this.symbol_signature(&abs_path);
                anyhow::ensure!(&new_signature == signature, "invalid symbol signature");
            }
            Ok(())
        })?;
        let buffer = this
            .update(&mut cx, |this, cx| {
                this.open_buffer_for_symbol(
                    &Symbol {
                        language_server_name: symbol.language_server_name,
                        source_worktree_id: symbol.source_worktree_id,
                        source_language_server_id: symbol.source_language_server_id,
                        path: symbol.path,
                        name: symbol.name,
                        kind: symbol.kind,
                        range: symbol.range,
                        label: CodeLabel::default(),
                        container_name: symbol.container_name,
                    },
                    cx,
                )
            })
            .await?;

        this.update(&mut cx, |this, cx| {
            let is_private = buffer
                .read(cx)
                .file()
                .map(|f| f.is_private())
                .unwrap_or_default();
            if is_private {
                Err(anyhow!(rpc::ErrorCode::UnsharedItem))
            } else {
                this.buffer_store
                    .update(cx, |buffer_store, cx| {
                        buffer_store.create_buffer_for_peer(&buffer, peer_id, cx)
                    })
                    .detach_and_log_err(cx);
                let buffer_id = buffer.read(cx).remote_id().to_proto();
                Ok(proto::OpenBufferForSymbolResponse { buffer_id })
            }
        })
    }

    fn symbol_signature(&self, abs_path: &Path) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(abs_path.to_string_lossy().as_bytes());
        hasher.update(self.nonce.to_be_bytes());
        hasher.finalize().as_slice().try_into().unwrap()
    }

    pub async fn handle_get_project_symbols(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::GetProjectSymbols>,
        mut cx: AsyncApp,
    ) -> Result<proto::GetProjectSymbolsResponse> {
        let symbols = this
            .update(&mut cx, |this, cx| {
                this.symbols(&envelope.payload.query, cx)
            })
            .await?;

        Ok(proto::GetProjectSymbolsResponse {
            symbols: symbols.iter().map(Self::serialize_symbol).collect(),
        })
    }

    async fn handle_apply_additional_edits_for_completion(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ApplyCompletionAdditionalEdits>,
        mut cx: AsyncApp,
    ) -> Result<proto::ApplyCompletionAdditionalEditsResponse> {
        let (buffer, completion, all_commit_ranges) = this.update(&mut cx, |this, cx| {
            let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
            let buffer = this.buffer_store.read(cx).get_existing(buffer_id)?;
            let completion = Self::deserialize_completion(
                envelope.payload.completion.context("invalid completion")?,
            )?;
            let all_commit_ranges = envelope
                .payload
                .all_commit_ranges
                .into_iter()
                .map(language::proto::deserialize_anchor_range)
                .collect::<Result<Vec<_>, _>>()?;
            anyhow::Ok((buffer, completion, all_commit_ranges))
        })?;

        let apply_additional_edits = this.update(&mut cx, |this, cx| {
            this.apply_additional_edits_for_completion(
                buffer,
                Rc::new(RefCell::new(Box::new([Completion {
                    replace_range: completion.replace_range,
                    new_text: completion.new_text,
                    source: completion.source,
                    documentation: None,
                    label: CodeLabel::default(),
                    match_start: None,
                    snippet_deduplication_key: None,
                    insert_text_mode: None,
                    icon_path: None,
                    icon_color: None,
                    confirm: None,
                    group: None,
                }]))),
                0,
                false,
                all_commit_ranges,
                cx,
            )
        });

        Ok(proto::ApplyCompletionAdditionalEditsResponse {
            transaction: apply_additional_edits
                .await?
                .as_ref()
                .map(language::proto::serialize_transaction),
        })
    }

    fn register_supplementary_language_server(
        &mut self,
        id: LanguageServerId,
        name: LanguageServerName,
        server: Arc<LanguageServer>,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.as_local_mut() {
            local
                .supplementary_language_servers
                .insert(id, (name.clone(), server));
            cx.emit(LspStoreEvent::LanguageServerAdded(id, name, None));
        }
    }

    fn unregister_supplementary_language_server(
        &mut self,
        id: LanguageServerId,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.as_local_mut() {
            local.supplementary_language_servers.remove(&id);
            cx.emit(LspStoreEvent::LanguageServerRemoved(id));
        }
    }

    pub(crate) fn supplementary_language_servers(
        &self,
    ) -> impl '_ + Iterator<Item = (LanguageServerId, LanguageServerName)> {
        self.as_local().into_iter().flat_map(|local| {
            local
                .supplementary_language_servers
                .iter()
                .map(|(id, (name, _))| (*id, name.clone()))
        })
    }

    pub fn language_server_adapter_for_id(
        &self,
        id: LanguageServerId,
    ) -> Option<Arc<CachedLspAdapter>> {
        if let Some(local) = self.as_local()
            && let Some(LanguageServerState::Running { adapter, .. }) =
                local.language_servers.get(&id)
        {
            return Some(adapter.clone());
        }
        // In remote (SSH/collab) mode there are no local `language_servers`, but
        // `language_server_statuses` is kept in sync with the upstream and carries each
        // server's registered name, which is enough to look the adapter up in the registry.
        let name = &self.language_server_statuses.get(&id)?.name;
        self.languages.adapter_for_name(name)
    }

    pub(super) fn update_local_worktree_language_servers(
        &mut self,
        worktree_handle: &Entity<Worktree>,
        changes: &[(Arc<RelPath>, ProjectEntryId, PathChange)],
        cx: &mut Context<Self>,
    ) {
        if changes.is_empty() {
            return;
        }

        let Some(local) = self.as_local_mut() else {
            return;
        };

        local.prettier_store.update(cx, |prettier_store, cx| {
            prettier_store.update_prettier_settings(worktree_handle, changes, cx)
        });

        let worktree_id = worktree_handle.read(cx).id();
        let mut language_server_ids = local
            .language_server_ids
            .iter()
            .filter_map(|(seed, v)| seed.worktree_id.eq(&worktree_id).then(|| v.id))
            .collect::<Vec<_>>();
        language_server_ids.sort_unstable();
        language_server_ids.dedup();

        // let abs_path = worktree_handle.read(cx).abs_path();
        for server_id in &language_server_ids {
            if let Some(LanguageServerState::Running { server, .. }) =
                local.language_servers.get(server_id)
                && let Some(watched_paths) = local
                    .language_server_watched_paths
                    .get_mut(server_id)
                    .and_then(|paths| paths.worktree_paths.get_mut(&worktree_id))
            {
                let params = lsp::DidChangeWatchedFilesParams {
                    changes: changes
                        .iter()
                        .filter_map(|(path, _, change)| {
                            let typ = match change {
                                PathChange::Loaded => return None,
                                PathChange::Added => lsp::FileChangeType::CREATED,
                                PathChange::Removed => lsp::FileChangeType::DELETED,
                                PathChange::Updated => lsp::FileChangeType::CHANGED,
                                PathChange::AddedOrUpdated => lsp::FileChangeType::CHANGED,
                            };
                            if !watched_paths.is_match(path.as_std_path()) {
                                return None;
                            }
                            let uri = lsp::Uri::from_file_path(
                                worktree_handle.read(cx).absolutize(&path),
                            )
                            .ok()?;
                            Some(lsp::FileEvent { uri, typ })
                        })
                        .collect(),
                };
                if !params.changes.is_empty() {
                    server
                        .notify::<lsp::notification::DidChangeWatchedFiles>(params)
                        .ok();
                }
            }
        }
        for (path, _, _) in changes {
            if let Some(file_name) = path.file_name()
                && local.watched_manifest_filenames.contains(file_name)
            {
                self.request_workspace_config_refresh();
                break;
            }
        }
    }

    pub fn wait_for_remote_buffer(
        &mut self,
        id: BufferId,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        self.buffer_store.update(cx, |buffer_store, cx| {
            buffer_store.wait_for_remote_buffer(id, cx)
        })
    }

    fn take_text_document_sync_options(
        capabilities: &mut lsp::ServerCapabilities,
    ) -> lsp::TextDocumentSyncOptions {
        match capabilities.text_document_sync.take() {
            Some(lsp::TextDocumentSyncCapability::Options(sync_options)) => sync_options,
            Some(lsp::TextDocumentSyncCapability::Kind(sync_kind)) => {
                let mut sync_options = lsp::TextDocumentSyncOptions::default();
                sync_options.change = Some(sync_kind);
                sync_options
            }
            None => lsp::TextDocumentSyncOptions::default(),
        }
    }

    pub fn downstream_client(&self) -> Option<(AnyProtoClient, u64)> {
        self.downstream_client.clone()
    }

    pub fn worktree_store(&self) -> Entity<WorktreeStore> {
        self.worktree_store.clone()
    }

    /// Gets what's stored in the LSP data for the given buffer.
    pub fn current_lsp_data(&mut self, buffer_id: BufferId) -> Option<&mut BufferLspData> {
        self.lsp_data.get_mut(&buffer_id)
    }

    /// Gets the most recent LSP data for the given buffer: if the data is absent or out of date,
    /// new [`BufferLspData`] will be created to replace the previous state.
    pub fn latest_lsp_data(&mut self, buffer: &Entity<Buffer>, cx: &mut App) -> &mut BufferLspData {
        let (buffer_id, buffer_version) =
            buffer.read_with(cx, |buffer, _| (buffer.remote_id(), buffer.version()));
        let lsp_data = self
            .lsp_data
            .entry(buffer_id)
            .or_insert_with(|| BufferLspData::new(buffer, cx));
        if buffer_version.changed_since(&lsp_data.buffer_version) {
            // To send delta requests for semantic tokens, the previous tokens
            // need to be kept between buffer changes.
            let semantic_tokens = lsp_data.semantic_tokens.take();
            *lsp_data = BufferLspData::new(buffer, cx);
            lsp_data.semantic_tokens = semantic_tokens;
        }
        lsp_data
    }
}

impl EventEmitter<LspStoreEvent> for LspStore {}
