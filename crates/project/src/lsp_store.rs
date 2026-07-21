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
mod inlay_hints;
pub mod json_language_server_ext;
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
mod store_events;
mod store_initialization;
mod store_mode;
mod symbol_types;
pub mod vue_language_server_ext;
mod workspace_config_refresh;
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
