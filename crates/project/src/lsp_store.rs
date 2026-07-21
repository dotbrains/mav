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
mod buffer_language_maintenance;
mod buffer_lsp_data;
mod capability_registration;
mod capability_unregistration;
pub mod clangd_ext;
mod code_action_resolution;
pub mod code_lens;
mod completion_documentation;
mod completion_labels;
mod diagnostic_summary;
mod diagnostic_updates;
mod diagnostics_types;
mod document_colors;
mod document_links;
mod document_symbols;
mod folding_ranges;
mod formatting_flow;
mod formatting_requests;
mod formatting_transaction;
mod formatting_types;
mod init_handlers;
mod inlay_hints;
pub mod json_language_server_ext;
mod local_code_actions;
mod local_formatting;
mod local_lsp_adapter_delegate;
mod local_server_lookup;
pub mod log_store;
mod lsp_buffer_snapshot;
pub mod lsp_ext_command;
mod lsp_query_serving;
mod lsp_store_events;
mod progress_handlers;
mod progress_token;
mod prompt_and_log;
mod proto_serialization;
mod query_types;
mod registration_options;
mod rename_watchers;
mod request_failure;
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

    async fn apply_formatter(
        formatter: &Formatter,
        lsp_store: &WeakEntity<LspStore>,
        buffer: &FormattableBuffer,
        formatting_transaction_id: clock::Lamport,
        adapters_and_servers: &[(Arc<CachedLspAdapter>, Arc<LanguageServer>)],
        settings: &LanguageSettings,
        request_timeout: Duration,
        logger: zlog::Logger,
        cx: &mut AsyncApp,
    ) -> anyhow::Result<()> {
        match formatter {
            Formatter::None => {
                zlog::trace!(logger => "skipping formatter 'none'");
                return Ok(());
            }
            Formatter::Auto => {
                debug_panic!("Auto resolved above");
                return Ok(());
            }
            Formatter::Prettier => {
                let logger = zlog::scoped!(logger => "prettier");
                zlog::trace!(logger => "formatting");
                let _timer = zlog::time!(logger => "Formatting buffer via prettier");

                // When selection ranges are provided (via FormatSelections), we pass the
                // encompassing UTF-16 range to Prettier so it can scope its formatting.
                // After diffing, we filter the resulting edits to only keep those that
                // overlap with the original byte-level selection ranges.
                let (range_utf16, byte_ranges) = match buffer.ranges.as_ref() {
                    Some(ranges) if !ranges.is_empty() => {
                        let (utf16_range, byte_ranges) =
                            buffer.handle.read_with(cx, |buffer, _cx| {
                                let snapshot = buffer.snapshot();
                                let mut min_start_utf16 = OffsetUtf16(usize::MAX);
                                let mut max_end_utf16 = OffsetUtf16(0);
                                let mut byte_ranges = Vec::with_capacity(ranges.len());
                                for range in ranges {
                                    let start_utf16 = range.start.to_offset_utf16(&snapshot);
                                    let end_utf16 = range.end.to_offset_utf16(&snapshot);
                                    min_start_utf16.0 = min_start_utf16.0.min(start_utf16.0);
                                    max_end_utf16.0 = max_end_utf16.0.max(end_utf16.0);

                                    let start_byte = range.start.to_offset(&snapshot);
                                    let end_byte = range.end.to_offset(&snapshot);
                                    byte_ranges.push(start_byte..end_byte);
                                }
                                (min_start_utf16..max_end_utf16, byte_ranges)
                            });
                        (Some(utf16_range), Some(byte_ranges))
                    }
                    _ => (None, None),
                };

                let prettier = lsp_store.read_with(cx, |lsp_store, _cx| {
                    lsp_store.prettier_store().unwrap().downgrade()
                })?;
                let diff = prettier_store::format_with_prettier(
                    &prettier,
                    &buffer.handle,
                    range_utf16,
                    cx,
                )
                .await
                .transpose()?;
                let Some(mut diff) = diff else {
                    zlog::trace!(logger => "No changes");
                    return Ok(());
                };

                if let Some(byte_ranges) = byte_ranges {
                    diff.edits.retain(|(edit_range, _)| {
                        byte_ranges.iter().any(|selection_range| {
                            edit_range.start < selection_range.end
                                && edit_range.end > selection_range.start
                        })
                    });
                    if diff.edits.is_empty() {
                        zlog::trace!(logger => "No changes within selection");
                        return Ok(());
                    }
                }

                extend_formatting_transaction(
                    buffer,
                    formatting_transaction_id,
                    cx,
                    |buffer, cx| {
                        buffer.apply_diff(diff, cx);
                    },
                )?;
            }
            Formatter::External { command, arguments } => {
                let logger = zlog::scoped!(logger => "command");

                if buffer.ranges.is_some() {
                    zlog::debug!(logger => "External formatter does not support range formatting; skipping");
                    return Ok(());
                }

                zlog::trace!(logger => "formatting");
                let _timer = zlog::time!(logger => "Formatting buffer via external command");

                let diff =
                    Self::format_via_external_command(buffer, &command, arguments.as_deref(), cx)
                        .await
                        .with_context(|| {
                            format!("Failed to format buffer via external command: {}", command)
                        })?;
                let Some(diff) = diff else {
                    zlog::trace!(logger => "No changes");
                    return Ok(());
                };

                extend_formatting_transaction(
                    buffer,
                    formatting_transaction_id,
                    cx,
                    |buffer, cx| {
                        buffer.apply_diff(diff, cx);
                    },
                )?;
            }
            Formatter::LanguageServer(specifier) => {
                let logger = zlog::scoped!(logger => "language-server");
                zlog::trace!(logger => "formatting");
                let _timer = zlog::time!(logger => "Formatting buffer using language server");

                let Some(buffer_path_abs) = buffer.abs_path.as_ref() else {
                    zlog::warn!(logger => "Cannot format buffer that is not backed by a file on disk using language servers. Skipping");
                    return Ok(());
                };

                let language_server = match specifier {
                    settings::LanguageServerFormatterSpecifier::Specific { name } => {
                        adapters_and_servers.iter().find_map(|(adapter, server)| {
                            if adapter.name.0.as_ref() == name {
                                Some(server.clone())
                            } else {
                                None
                            }
                        })
                    }
                    settings::LanguageServerFormatterSpecifier::Current => adapters_and_servers
                        .iter()
                        .find(|(_, server)| Self::server_supports_formatting(server))
                        .map(|(_, server)| server.clone()),
                };

                let Some(language_server) = language_server else {
                    log::debug!(
                        "No language server found to format buffer '{:?}'. Skipping",
                        buffer_path_abs.as_path().to_string_lossy()
                    );
                    return Ok(());
                };

                zlog::trace!(
                    logger =>
                    "Formatting buffer '{:?}' using language server '{:?}'",
                    buffer_path_abs.as_path().to_string_lossy(),
                    language_server.name()
                );

                let edits = if let Some(ranges) = buffer.ranges.as_ref() {
                    zlog::trace!(logger => "formatting ranges");
                    Self::format_ranges_via_lsp(
                        &lsp_store,
                        &buffer.handle,
                        ranges,
                        buffer_path_abs,
                        &language_server,
                        &settings,
                        cx,
                    )
                    .await
                    .context("Failed to format ranges via language server")?
                } else {
                    zlog::trace!(logger => "formatting full");
                    Self::format_via_lsp(
                        &lsp_store,
                        &buffer.handle,
                        buffer_path_abs,
                        &language_server,
                        &settings,
                        cx,
                    )
                    .await
                    .context("failed to format via language server")?
                };

                if edits.is_empty() {
                    zlog::trace!(logger => "No changes");
                    return Ok(());
                }
                extend_formatting_transaction(
                    buffer,
                    formatting_transaction_id,
                    cx,
                    |buffer, cx| {
                        buffer.edit(edits, None, cx);
                    },
                )?;
            }
            Formatter::CodeAction(code_action_name) => {
                let logger = zlog::scoped!(logger => "code-actions");
                zlog::trace!(logger => "formatting");
                let _timer = zlog::time!(logger => "Formatting buffer using code actions");

                let Some(buffer_path_abs) = buffer.abs_path.as_ref() else {
                    zlog::warn!(logger => "Cannot format buffer that is not backed by a file on disk using code actions. Skipping");
                    return Ok(());
                };

                let code_action_kind: CodeActionKind = code_action_name.clone().into();
                zlog::trace!(logger => "Attempting to resolve code actions {:?}", &code_action_kind);

                let mut actions_and_servers = Vec::new();

                for (index, (_, language_server)) in adapters_and_servers.iter().enumerate() {
                    let actions_result = Self::get_server_code_actions_from_action_kinds(
                        &lsp_store,
                        language_server.server_id(),
                        vec![code_action_kind.clone()],
                        &buffer.handle,
                        cx,
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to resolve code action {:?} with language server {}",
                            code_action_kind,
                            language_server.name()
                        )
                    });
                    let Ok(actions) = actions_result else {
                        // note: it may be better to set result to the error and break formatters here
                        // but for now we try to execute the actions that we can resolve and skip the rest
                        zlog::error!(
                            logger =>
                            "Failed to resolve code action {:?} with language server {}",
                            code_action_kind,
                            language_server.name()
                        );
                        continue;
                    };
                    for action in actions {
                        actions_and_servers.push((action, index));
                    }
                }

                if actions_and_servers.is_empty() {
                    zlog::warn!(logger => "No code actions were resolved, continuing");
                    return Ok(());
                }

                'actions: for (mut action, server_index) in actions_and_servers {
                    let server = &adapters_and_servers[server_index].1;

                    let describe_code_action = |action: &CodeAction| {
                        format!(
                            "code action '{}' with title \"{}\" on server {}",
                            action
                                .lsp_action
                                .action_kind()
                                .unwrap_or("unknown".into())
                                .as_str(),
                            action.lsp_action.title(),
                            server.name(),
                        )
                    };

                    zlog::trace!(logger => "Executing {}", describe_code_action(&action));

                    if let Err(err) =
                        Self::try_resolve_code_action(server, &mut action, request_timeout).await
                    {
                        zlog::error!(
                            logger =>
                            "Failed to resolve {}. Error: {}",
                            describe_code_action(&action),
                            err
                        );
                        continue;
                    }

                    if let Some(edit) = action.lsp_action.edit().cloned() {
                        // NOTE: code below duplicated from `Self::deserialize_workspace_edit`
                        // but filters out and logs warnings for code actions that require unreasonably
                        // difficult handling on our part, such as:
                        // - applying edits that call commands
                        //   which can result in arbitrary workspace edits being sent from the server that
                        //   have no way of being tied back to the command that initiated them (i.e. we
                        //   can't know which edits are part of the format request, or if the server is done sending
                        //   actions in response to the command)
                        // - actions that create/delete/modify/rename files other than the one we are formatting
                        //   as we then would need to handle such changes correctly in the local history as well
                        //   as the remote history through the ProjectTransaction
                        // - actions with snippet edits, as these simply don't make sense in the context of a format request
                        // Supporting these actions is not impossible, but not supported as of yet.
                        if edit.changes.is_none() && edit.document_changes.is_none() {
                            zlog::trace!(
                                logger =>
                                "No changes for code action. Skipping {}",
                                describe_code_action(&action),
                            );
                            continue;
                        }

                        let mut operations = Vec::new();
                        if let Some(document_changes) = edit.document_changes {
                            match document_changes {
                                lsp::DocumentChanges::Edits(edits) => operations.extend(
                                    edits.into_iter().map(lsp::DocumentChangeOperation::Edit),
                                ),
                                lsp::DocumentChanges::Operations(ops) => operations = ops,
                            }
                        } else if let Some(changes) = edit.changes {
                            operations.extend(changes.into_iter().map(|(uri, edits)| {
                                lsp::DocumentChangeOperation::Edit(lsp::TextDocumentEdit {
                                    text_document: lsp::OptionalVersionedTextDocumentIdentifier {
                                        uri,
                                        version: None,
                                    },
                                    edits: edits.into_iter().map(Edit::Plain).collect(),
                                })
                            }));
                        }

                        let mut edits = Vec::with_capacity(operations.len());

                        if operations.is_empty() {
                            zlog::trace!(
                                logger =>
                                "No changes for code action. Skipping {}",
                                describe_code_action(&action),
                            );
                            continue;
                        }
                        for operation in operations {
                            let op = match operation {
                                lsp::DocumentChangeOperation::Edit(op) => op,
                                lsp::DocumentChangeOperation::Op(_) => {
                                    zlog::warn!(
                                        logger =>
                                        "Code actions which create, delete, or rename files are not supported on format. Skipping {}",
                                        describe_code_action(&action),
                                    );
                                    continue 'actions;
                                }
                            };
                            let Ok(file_path) = op.text_document.uri.to_file_path() else {
                                zlog::warn!(
                                    logger =>
                                    "Failed to convert URI '{:?}' to file path. Skipping {}",
                                    &op.text_document.uri,
                                    describe_code_action(&action),
                                );
                                continue 'actions;
                            };
                            if &file_path != buffer_path_abs {
                                zlog::warn!(
                                    logger =>
                                    "File path '{:?}' does not match buffer path '{:?}'. Skipping {}",
                                    file_path,
                                    buffer_path_abs,
                                    describe_code_action(&action),
                                );
                                continue 'actions;
                            }

                            let mut lsp_edits = Vec::new();
                            for edit in op.edits {
                                match edit {
                                    Edit::Plain(edit) => {
                                        if !lsp_edits.contains(&edit) {
                                            lsp_edits.push(edit);
                                        }
                                    }
                                    Edit::Annotated(edit) => {
                                        if !lsp_edits.contains(&edit.text_edit) {
                                            lsp_edits.push(edit.text_edit);
                                        }
                                    }
                                    Edit::Snippet(_) => {
                                        zlog::warn!(
                                            logger =>
                                            "Code actions which produce snippet edits are not supported during formatting. Skipping {}",
                                            describe_code_action(&action),
                                        );
                                        continue 'actions;
                                    }
                                }
                            }
                            let edits_result = lsp_store
                                .update(cx, |lsp_store, cx| {
                                    lsp_store.as_local_mut().unwrap().edits_from_lsp(
                                        &buffer.handle,
                                        lsp_edits,
                                        server.server_id(),
                                        op.text_document.version,
                                        cx,
                                    )
                                })?
                                .await;
                            let Ok(resolved_edits) = edits_result else {
                                zlog::warn!(
                                    logger =>
                                    "Failed to resolve edits from LSP for buffer {:?} while handling {}",
                                    buffer_path_abs.as_path(),
                                    describe_code_action(&action),
                                );
                                continue 'actions;
                            };
                            edits.extend(resolved_edits);
                        }

                        if edits.is_empty() {
                            zlog::warn!(logger => "No edits resolved from LSP");
                            continue;
                        }

                        extend_formatting_transaction(
                            buffer,
                            formatting_transaction_id,
                            cx,
                            |buffer, cx| {
                                zlog::info!(
                                    "Applying edits {edits:?}. Content: {:?}",
                                    buffer.text()
                                );
                                buffer.edit(edits, None, cx);
                                zlog::info!("Applied edits. New Content: {:?}", buffer.text());
                            },
                        )?;
                    }

                    let Some(command) = action.lsp_action.command() else {
                        continue;
                    };

                    zlog::warn!(
                        logger =>
                        "Executing code action command '{}'. This may cause formatting to abort unnecessarily as well as splitting formatting into two entries in the undo history",
                        &command.command,
                    );

                    let server_capabilities = server.capabilities();
                    let available_commands = server_capabilities
                        .execute_command_provider
                        .as_ref()
                        .map(|options| options.commands.as_slice())
                        .unwrap_or_default();
                    if !available_commands.contains(&command.command) {
                        zlog::warn!(
                            logger =>
                            "Cannot execute a command {} not listed in the language server capabilities of server {}",
                            command.command,
                            server.name(),
                        );
                        continue;
                    }

                    extend_formatting_transaction(
                        buffer,
                        formatting_transaction_id,
                        cx,
                        |_, _| {},
                    )?;
                    zlog::info!(logger => "Executing command {}", &command.command);

                    lsp_store.update(cx, |this, _| {
                        this.as_local_mut()
                            .unwrap()
                            .last_workspace_edits_by_language_server
                            .remove(&server.server_id());
                    })?;

                    let execute_command_result = server
                        .request::<lsp::request::ExecuteCommand>(
                            lsp::ExecuteCommandParams {
                                command: command.command.clone(),
                                arguments: command.arguments.clone().unwrap_or_default(),
                                ..Default::default()
                            },
                            request_timeout,
                        )
                        .await
                        .into_response();

                    if execute_command_result.is_err() {
                        zlog::error!(
                            logger =>
                            "Failed to execute command '{}' as part of {}",
                            &command.command,
                            describe_code_action(&action),
                        );
                        continue 'actions;
                    }

                    let mut project_transaction_command = lsp_store.update(cx, |this, _| {
                        this.as_local_mut()
                            .unwrap()
                            .last_workspace_edits_by_language_server
                            .remove(&server.server_id())
                            .unwrap_or_default()
                    })?;

                    if let Some(transaction) = project_transaction_command.0.remove(&buffer.handle)
                    {
                        zlog::trace!(
                            logger =>
                            "Successfully captured {} edits that resulted from command {}",
                            transaction.edit_ids.len(),
                            &command.command,
                        );
                        let transaction_id_project_transaction = transaction.id;
                        buffer.handle.update(cx, |buffer, _| {
                            // it may have been removed from history if push_to_history was
                            // false in deserialize_workspace_edit. If so push it so we
                            // can merge it with the format transaction
                            // and pop the combined transaction off the history stack
                            // later if push_to_history is false
                            if buffer.get_transaction(transaction.id).is_none() {
                                buffer.push_transaction(transaction, Instant::now());
                            }
                            buffer.merge_transactions(
                                transaction_id_project_transaction,
                                formatting_transaction_id,
                            );
                        });
                    }

                    if project_transaction_command.0.is_empty() {
                        continue;
                    }

                    let mut extra_buffers = String::new();
                    for buffer in project_transaction_command.0.keys() {
                        buffer.read_with(cx, |b, cx| {
                            let Some(path) = b.project_path(cx) else {
                                return;
                            };

                            if !extra_buffers.is_empty() {
                                extra_buffers.push_str(", ");
                            }
                            extra_buffers.push_str(path.path.as_unix_str());
                        });
                    }
                    zlog::warn!(
                        logger =>
                        "Unexpected edits to buffers other than the buffer actively being formatted due to command {}. Impacted buffers: [{}].",
                        &command.command,
                        extra_buffers,
                    );
                    // NOTE: if this case is hit, the proper thing to do is to for each buffer, merge the extra transaction
                    // into the existing transaction in project_transaction if there is one, and if there isn't one in project_transaction,
                    // add it so it's included, and merge it into the format transaction when its created later
                }
            }
        }

        Ok(())
    }

    fn initialize_buffer(&mut self, buffer_handle: &Entity<Buffer>, cx: &mut Context<LspStore>) {
        let buffer = buffer_handle.read(cx);

        let file = buffer.file().cloned();

        let Some(file) = File::from_dyn(file.as_ref()) else {
            return;
        };
        if !file.is_local() {
            return;
        }
        let path = ProjectPath::from_file(file, cx);
        let worktree_id = file.worktree_id(cx);
        let language = buffer.language().cloned();

        if let Some(diagnostics) = self.diagnostics.get(&worktree_id) {
            for (server_id, diagnostics) in
                diagnostics.get(file.path()).cloned().unwrap_or_default()
            {
                self.update_buffer_diagnostics(
                    buffer_handle,
                    server_id,
                    None,
                    None,
                    None,
                    Vec::new(),
                    diagnostics,
                    cx,
                )
                .log_err();
            }
        }
        let Some(language) = language else {
            return;
        };
        let Some(snapshot) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(worktree_id, cx)
            .map(|worktree| worktree.read(cx).snapshot())
        else {
            return;
        };
        let delegate: Arc<dyn ManifestDelegate> = Arc::new(ManifestQueryDelegate::new(snapshot));

        for server_id in
            self.lsp_tree
                .get(path, language.name(), language.manifest(), &delegate, cx)
        {
            let server = self
                .language_servers
                .get(&server_id)
                .and_then(|server_state| {
                    if let LanguageServerState::Running { server, .. } = server_state {
                        Some(server.clone())
                    } else {
                        None
                    }
                });
            let server = match server {
                Some(server) => server,
                None => continue,
            };

            buffer_handle.update(cx, |buffer, cx| {
                buffer.set_completion_triggers(
                    server.server_id(),
                    server
                        .capabilities()
                        .completion_provider
                        .as_ref()
                        .and_then(|provider| {
                            provider
                                .trigger_characters
                                .as_ref()
                                .map(|characters| characters.iter().cloned().collect())
                        })
                        .unwrap_or_default(),
                    cx,
                );
            });
        }
    }

    pub(crate) fn reset_buffer(&mut self, buffer: &Entity<Buffer>, old_file: &File, cx: &mut App) {
        buffer.update(cx, |buffer, cx| {
            let Some(language) = buffer.language() else {
                return;
            };
            let path = ProjectPath {
                worktree_id: old_file.worktree_id(cx),
                path: old_file.path.clone(),
            };
            for server_id in self.language_server_ids_for_project_path(path, language, cx) {
                buffer.update_diagnostics(server_id, DiagnosticSet::new([], buffer), cx);
                buffer.set_completion_triggers(server_id, Default::default(), cx);
            }
        });
    }

    fn update_buffer_diagnostics(
        &mut self,
        buffer: &Entity<Buffer>,
        server_id: LanguageServerId,
        registration_id: Option<Option<SharedString>>,
        result_id: Option<SharedString>,
        version: Option<i32>,
        new_diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
        reused_diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
        cx: &mut Context<LspStore>,
    ) -> Result<()> {
        fn compare_diagnostics(a: &Diagnostic, b: &Diagnostic) -> Ordering {
            Ordering::Equal
                .then_with(|| b.is_primary.cmp(&a.is_primary))
                .then_with(|| a.is_disk_based.cmp(&b.is_disk_based))
                .then_with(|| a.severity.cmp(&b.severity))
                .then_with(|| a.message.cmp(&b.message))
        }

        let mut diagnostics = Vec::with_capacity(new_diagnostics.len() + reused_diagnostics.len());
        diagnostics.extend(new_diagnostics.into_iter().map(|d| (true, d)));
        diagnostics.extend(reused_diagnostics.into_iter().map(|d| (false, d)));

        diagnostics.sort_unstable_by(|(_, a), (_, b)| {
            Ordering::Equal
                .then_with(|| a.range.start.cmp(&b.range.start))
                .then_with(|| b.range.end.cmp(&a.range.end))
                .then_with(|| compare_diagnostics(&a.diagnostic, &b.diagnostic))
        });

        let snapshot = self.buffer_snapshot_for_lsp_version(buffer, server_id, version, cx)?;

        let edits_since_save = std::cell::LazyCell::new(|| {
            let saved_version = buffer.read(cx).saved_version();
            Patch::new(snapshot.edits_since::<PointUtf16>(saved_version).collect())
        });

        let mut sanitized_diagnostics = Vec::with_capacity(diagnostics.len());

        for (new_diagnostic, entry) in diagnostics {
            let start;
            let end;
            if new_diagnostic && entry.diagnostic.is_disk_based {
                // Some diagnostics are based on files on disk instead of buffers'
                // current contents. Adjust these diagnostics' ranges to reflect
                // any unsaved edits.
                // Do not alter the reused ones though, as their coordinates were stored as anchors
                // and were properly adjusted on reuse.
                start = Unclipped((*edits_since_save).old_to_new(entry.range.start.0));
                end = Unclipped((*edits_since_save).old_to_new(entry.range.end.0));
            } else {
                start = entry.range.start;
                end = entry.range.end;
            }

            let mut range = snapshot.clip_point_utf16(start, Bias::Left)
                ..snapshot.clip_point_utf16(end, Bias::Right);

            // Expand empty ranges by one codepoint
            if range.start == range.end {
                // This will be go to the next boundary when being clipped
                range.end.column += 1;
                range.end = snapshot.clip_point_utf16(Unclipped(range.end), Bias::Right);
                if range.start == range.end && range.end.column > 0 {
                    range.start.column -= 1;
                    range.start = snapshot.clip_point_utf16(Unclipped(range.start), Bias::Left);
                }
            }

            sanitized_diagnostics.push(DiagnosticEntry {
                range,
                diagnostic: entry.diagnostic,
            });
        }
        drop(edits_since_save);

        let set = DiagnosticSet::new(sanitized_diagnostics, &snapshot);
        buffer.update(cx, |buffer, cx| {
            if let Some(registration_id) = registration_id {
                if let Some(abs_path) = File::from_dyn(buffer.file()).map(|f| f.abs_path(cx)) {
                    self.buffer_pull_diagnostics_result_ids
                        .entry(server_id)
                        .or_default()
                        .entry(registration_id)
                        .or_default()
                        .insert(abs_path, result_id);
                }
            }

            buffer.update_diagnostics(server_id, set, cx)
        });

        Ok(())
    }

    fn register_language_server_for_invisible_worktree(
        &mut self,
        worktree: &Entity<Worktree>,
        language_server_id: LanguageServerId,
        cx: &mut App,
    ) {
        let worktree = worktree.read(cx);
        let worktree_id = worktree.id();
        debug_assert!(!worktree.is_visible());
        let Some(mut origin_seed) = self
            .language_server_ids
            .iter()
            .find_map(|(seed, state)| (state.id == language_server_id).then(|| seed.clone()))
        else {
            return;
        };
        origin_seed.worktree_id = worktree_id;
        self.language_server_ids
            .entry(origin_seed)
            .or_insert_with(|| UnifiedLanguageServer {
                id: language_server_id,
                project_roots: Default::default(),
            });
    }

    fn register_buffer_with_language_servers(
        &mut self,
        buffer_handle: &Entity<Buffer>,
        only_register_servers: HashSet<LanguageServerSelector>,
        cx: &mut Context<LspStore>,
    ) {
        if self.all_language_servers_stopped {
            return;
        }
        let buffer = buffer_handle.read(cx);
        let buffer_id = buffer.remote_id();

        let Some(file) = File::from_dyn(buffer.file()) else {
            return;
        };
        if !file.is_local() {
            return;
        }

        let abs_path = file.abs_path(cx);
        let Some(uri) = file_path_to_lsp_url(&abs_path).log_err() else {
            return;
        };
        let initial_snapshot = buffer.text_snapshot();
        let worktree_id = file.worktree_id(cx);

        let Some(language) = buffer.language().cloned() else {
            return;
        };
        let path: Arc<RelPath> = file
            .path()
            .parent()
            .map(Arc::from)
            .unwrap_or_else(|| file.path().clone());
        let Some(worktree) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(worktree_id, cx)
        else {
            return;
        };
        let language_name = language.name();
        let (reused, delegate, servers) = self
            .reuse_existing_language_server(&self.lsp_tree, &worktree, &language_name, cx)
            .map(|(delegate, apply)| (true, delegate, apply(&mut self.lsp_tree)))
            .unwrap_or_else(|| {
                let lsp_delegate = LocalLspAdapterDelegate::from_local_lsp(self, &worktree, cx);
                let delegate: Arc<dyn ManifestDelegate> =
                    Arc::new(ManifestQueryDelegate::new(worktree.read(cx).snapshot()));

                let servers = self
                    .lsp_tree
                    .walk(
                        ProjectPath { worktree_id, path },
                        language.name(),
                        language.manifest(),
                        &delegate,
                        cx,
                    )
                    .collect::<Vec<_>>();
                (false, lsp_delegate, servers)
            });
        let servers_and_adapters = servers
            .into_iter()
            .filter_map(|server_node| {
                if reused && server_node.server_id().is_none() {
                    return None;
                }
                if let Some(name) = server_node.name()
                    && self.stopped_language_servers.contains(&name)
                {
                    return None;
                }
                if !only_register_servers.is_empty() {
                    if let Some(server_id) = server_node.server_id()
                        && !only_register_servers.contains(&LanguageServerSelector::Id(server_id))
                    {
                        return None;
                    }
                    if let Some(name) = server_node.name()
                        && !only_register_servers.contains(&LanguageServerSelector::Name(name))
                    {
                        return None;
                    }
                }

                let server_id = server_node.server_id_or_init(|disposition| {
                    let path = &disposition.path;

                    {
                        let uri = Uri::from_file_path(worktree.read(cx).absolutize(&path.path));

                        let server_id = self.get_or_insert_language_server(
                            &worktree,
                            delegate.clone(),
                            disposition,
                            &language_name,
                            cx,
                        );

                        if let Some(state) = self.language_servers.get(&server_id)
                            && let Ok(uri) = uri
                        {
                            state.add_workspace_folder(uri);
                        };
                        server_id
                    }
                })?;
                let server_state = self.language_servers.get(&server_id)?;
                if let LanguageServerState::Running {
                    server, adapter, ..
                } = server_state
                {
                    Some((server.clone(), adapter.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (server, adapter) in servers_and_adapters {
            buffer_handle.update(cx, |buffer, cx| {
                buffer.set_completion_triggers(
                    server.server_id(),
                    server
                        .capabilities()
                        .completion_provider
                        .as_ref()
                        .and_then(|provider| {
                            provider
                                .trigger_characters
                                .as_ref()
                                .map(|characters| characters.iter().cloned().collect())
                        })
                        .unwrap_or_default(),
                    cx,
                );
            });

            let snapshot = LspBufferSnapshot {
                version: 0,
                snapshot: initial_snapshot.clone(),
            };

            let mut registered = false;
            self.buffer_snapshots
                .entry(buffer_id)
                .or_default()
                .entry(server.server_id())
                .or_insert_with(|| {
                    registered = true;
                    server.register_buffer(
                        uri.clone(),
                        adapter.language_id(&language.name()),
                        0,
                        initial_snapshot.text(),
                    );

                    vec![snapshot]
                });

            self.buffers_opened_in_servers
                .entry(buffer_id)
                .or_default()
                .insert(server.server_id());
            if registered {
                cx.emit(LspStoreEvent::LanguageServerUpdate {
                    language_server_id: server.server_id(),
                    name: None,
                    message: proto::update_language_server::Variant::RegisteredForBuffer(
                        proto::RegisteredForBuffer {
                            buffer_abs_path: abs_path.to_string_lossy().into_owned(),
                            buffer_id: buffer_id.to_proto(),
                        },
                    ),
                });
            }
        }
    }

    fn reuse_existing_language_server<'lang_name>(
        &self,
        server_tree: &LanguageServerTree,
        worktree: &Entity<Worktree>,
        language_name: &'lang_name LanguageName,
        cx: &mut App,
    ) -> Option<(
        Arc<LocalLspAdapterDelegate>,
        impl FnOnce(&mut LanguageServerTree) -> Vec<LanguageServerTreeNode> + use<'lang_name>,
    )> {
        if worktree.read(cx).is_visible() {
            return None;
        }

        let worktree_store = self.worktree_store.read(cx);
        let servers = server_tree
            .instances
            .iter()
            .filter(|(worktree_id, _)| {
                worktree_store
                    .worktree_for_id(**worktree_id, cx)
                    .is_some_and(|worktree| worktree.read(cx).is_visible())
            })
            .flat_map(|(worktree_id, servers)| {
                servers
                    .roots
                    .values()
                    .flatten()
                    .map(move |(_, (server_node, server_languages))| {
                        (worktree_id, server_node, server_languages)
                    })
                    .filter(|(_, _, server_languages)| server_languages.contains(language_name))
                    .map(|(worktree_id, server_node, _)| {
                        (
                            *worktree_id,
                            LanguageServerTreeNode::from(Arc::downgrade(server_node)),
                        )
                    })
            })
            .fold(HashMap::default(), |mut acc, (worktree_id, server_node)| {
                acc.entry(worktree_id)
                    .or_insert_with(Vec::new)
                    .push(server_node);
                acc
            })
            .into_values()
            .max_by_key(|servers| servers.len())?;

        let worktree_id = worktree.read(cx).id();
        let apply = move |tree: &mut LanguageServerTree| {
            for server_node in &servers {
                tree.register_reused(worktree_id, language_name.clone(), server_node.clone());
            }
            servers
        };

        let delegate = LocalLspAdapterDelegate::from_local_lsp(self, worktree, cx);
        Some((delegate, apply))
    }

    pub(crate) fn unregister_old_buffer_from_language_servers(
        &mut self,
        buffer: &Entity<Buffer>,
        old_file: &File,
        cx: &mut App,
    ) {
        let old_path = match old_file.as_local() {
            Some(local) => local.abs_path(cx),
            None => return,
        };

        let Ok(file_url) = lsp::Uri::from_file_path(old_path.as_path()) else {
            return;
        };
        self.unregister_buffer_from_language_servers(buffer, &file_url, cx);
    }

    pub(crate) fn unregister_buffer_from_language_servers(
        &mut self,
        buffer: &Entity<Buffer>,
        file_url: &lsp::Uri,
        cx: &mut App,
    ) {
        buffer.update(cx, |buffer, cx| {
            let mut snapshots = self.buffer_snapshots.remove(&buffer.remote_id());

            for (_, language_server) in self.language_servers_for_buffer(buffer, cx) {
                if snapshots
                    .as_mut()
                    .is_some_and(|map| map.remove(&language_server.server_id()).is_some())
                {
                    language_server.unregister_buffer(file_url.clone());
                }
            }
        });
    }

    fn buffer_snapshot_for_lsp_version(
        &mut self,
        buffer: &Entity<Buffer>,
        server_id: LanguageServerId,
        version: Option<i32>,
        cx: &App,
    ) -> Result<TextBufferSnapshot> {
        const OLD_VERSIONS_TO_RETAIN: i32 = 10;

        if let Some(version) = version {
            let buffer_id = buffer.read(cx).remote_id();
            let snapshots = if let Some(snapshots) = self
                .buffer_snapshots
                .get_mut(&buffer_id)
                .and_then(|m| m.get_mut(&server_id))
            {
                snapshots
            } else if version == 0 {
                // Some language servers report version 0 even if the buffer hasn't been opened yet.
                // We detect this case and treat it as if the version was `None`.
                return Ok(buffer.read(cx).text_snapshot());
            } else {
                anyhow::bail!("no snapshots found for buffer {buffer_id} and server {server_id}");
            };

            let found_snapshot = snapshots
                    .binary_search_by_key(&version, |e| e.version)
                    .map(|ix| snapshots[ix].snapshot.clone())
                    .map_err(|_| {
                        anyhow!("snapshot not found for buffer {buffer_id} server {server_id} at version {version}")
                    })?;

            snapshots.retain(|snapshot| snapshot.version + OLD_VERSIONS_TO_RETAIN >= version);
            Ok(found_snapshot)
        } else {
            Ok((buffer.read(cx)).text_snapshot())
        }
    }

    fn remove_worktree(
        &mut self,
        id_to_remove: WorktreeId,
        cx: &mut Context<LspStore>,
    ) -> Vec<LanguageServerId> {
        self.restricted_worktrees_tasks.remove(&id_to_remove);
        self.diagnostics.remove(&id_to_remove);
        self.prettier_store.update(cx, |prettier_store, cx| {
            prettier_store.remove_worktree(id_to_remove, cx);
        });

        let mut servers_to_remove = BTreeSet::default();
        let mut servers_to_preserve = HashSet::default();
        for (seed, state) in &self.language_server_ids {
            if seed.worktree_id == id_to_remove {
                servers_to_remove.insert(state.id);
            } else {
                servers_to_preserve.insert(state.id);
            }
        }
        servers_to_remove.retain(|server_id| !servers_to_preserve.contains(server_id));
        self.language_server_ids.retain(|seed, state| {
            seed.worktree_id != id_to_remove && !servers_to_remove.contains(&state.id)
        });
        self.lsp_tree.instances.remove(&id_to_remove);
        for server_id_to_remove in &servers_to_remove {
            self.language_server_watched_paths
                .remove(server_id_to_remove);
            self.language_server_paths_watched_for_rename
                .remove(server_id_to_remove);
            self.last_workspace_edits_by_language_server
                .remove(server_id_to_remove);
            self.language_servers.remove(server_id_to_remove);
            self.buffer_pull_diagnostics_result_ids
                .remove(server_id_to_remove);
            self.workspace_pull_diagnostics_result_ids
                .remove(server_id_to_remove);
            for buffer_servers in self.buffers_opened_in_servers.values_mut() {
                buffer_servers.remove(server_id_to_remove);
            }
            cx.emit(LspStoreEvent::LanguageServerRemoved(*server_id_to_remove));
        }
        servers_to_remove.into_iter().collect()
    }

    fn register_watcher(
        &mut self,
        worktrees: &[Entity<Worktree>],
        watcher: &FileSystemWatcher,
        registration_id: &str,
        language_server_id: LanguageServerId,
        cx: &mut Context<LspStore>,
    ) {
        let watched = self
            .language_server_watched_paths
            .entry(language_server_id)
            .or_default();

        if let Some((worktree, literal_prefix, pattern)) =
            Self::worktree_and_path_for_file_watcher(worktrees, watcher, cx)
        {
            if worktree.read(cx).as_local().is_some() {
                if let Some(glob) = Glob::new(&pattern).log_err() {
                    let worktree_id = worktree.read(cx).id();
                    watched
                        .worktree_paths
                        .entry(worktree_id)
                        .or_default()
                        .add(registration_id, glob);
                    worktree.update(cx, |worktree, _| {
                        if let Some(tree) = worktree.as_local_mut() {
                            tree.add_path_prefix_to_scan(literal_prefix);
                        }
                    });
                }
            }

            return;
        }

        let (path, pattern) = match &watcher.glob_pattern {
            lsp::GlobPattern::String(s) => {
                let watcher_path = SanitizedPath::new(s);
                let path = glob_literal_prefix(watcher_path.as_path());
                let pattern = watcher_path
                    .as_path()
                    .strip_prefix(&path)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|e| {
                        debug_panic!(
                            "Failed to strip prefix for string pattern: {}, with prefix: {}, with error: {}",
                            s,
                            path.display(),
                            e
                        );
                        watcher_path.as_path().to_string_lossy().into_owned()
                    });
                (path, pattern)
            }
            lsp::GlobPattern::Relative(rp) => {
                let Ok(mut base_uri) = match &rp.base_uri {
                    lsp::OneOf::Left(workspace_folder) => &workspace_folder.uri,
                    lsp::OneOf::Right(base_uri) => base_uri,
                }
                .to_file_path() else {
                    return;
                };

                let path = glob_literal_prefix(Path::new(&rp.pattern));
                let pattern = Path::new(&rp.pattern)
                    .strip_prefix(&path)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|e| {
                        debug_panic!(
                            "Failed to strip prefix for relative pattern: {}, with prefix: {}, with error: {}",
                            rp.pattern,
                            path.display(),
                            e
                        );
                        rp.pattern.clone()
                    });
                base_uri.push(path);
                (base_uri, pattern)
            }
        };

        if let Some(glob) = Glob::new(&pattern).log_err() {
            if !path
                .components()
                .any(|c| matches!(c, path::Component::Normal(_)))
            {
                // For an unrooted glob like `**/Cargo.toml`, watch it within each worktree,
                // rather than adding a new watcher for `/`.
                for worktree in worktrees {
                    watched
                        .worktree_paths
                        .entry(worktree.read(cx).id())
                        .or_default()
                        .add(registration_id, glob.clone());
                }
            } else {
                let abs_path: Arc<Path> = path.into();
                let fs = self.fs.clone();
                let entry = watched
                    .abs_paths
                    .entry(abs_path.clone())
                    .or_insert_with(|| {
                        let task = LanguageServerWatchedPaths::spawn_abs_path_watcher(
                            abs_path,
                            fs,
                            language_server_id,
                            cx,
                        );
                        (LazyGlobSet::default(), task)
                    });
                entry.0.add(registration_id, glob);
            }
        }
    }

    fn worktree_and_path_for_file_watcher(
        worktrees: &[Entity<Worktree>],
        watcher: &FileSystemWatcher,
        cx: &App,
    ) -> Option<(Entity<Worktree>, Arc<RelPath>, String)> {
        worktrees.iter().find_map(|worktree| {
            let tree = worktree.read(cx);
            let worktree_root_path = tree.abs_path();
            let path_style = tree.path_style();
            match &watcher.glob_pattern {
                lsp::GlobPattern::String(s) => {
                    let watcher_path = SanitizedPath::new(s);
                    let relative = watcher_path
                        .as_path()
                        .strip_prefix(&worktree_root_path)
                        .ok()?;
                    let literal_prefix = glob_literal_prefix(relative);
                    Some((
                        worktree.clone(),
                        RelPath::new(&literal_prefix, path_style).ok()?.into_arc(),
                        relative.to_string_lossy().into_owned(),
                    ))
                }
                lsp::GlobPattern::Relative(rp) => {
                    let base_uri = match &rp.base_uri {
                        lsp::OneOf::Left(workspace_folder) => &workspace_folder.uri,
                        lsp::OneOf::Right(base_uri) => base_uri,
                    }
                    .to_file_path()
                    .ok()?;
                    let relative = base_uri.strip_prefix(&worktree_root_path).ok()?;
                    let mut literal_prefix = relative.to_owned();
                    literal_prefix.push(glob_literal_prefix(Path::new(&rp.pattern)));
                    Some((
                        worktree.clone(),
                        RelPath::new(&literal_prefix, path_style).ok()?.into_arc(),
                        rp.pattern.clone(),
                    ))
                }
            }
        })
    }

    fn on_lsp_did_change_watched_files(
        &mut self,
        language_server_id: LanguageServerId,
        registration_id: &str,
        params: DidChangeWatchedFilesRegistrationOptions,
        cx: &mut Context<LspStore>,
    ) {
        log::trace!(
            "Processing new watcher paths for language server with id {}",
            language_server_id
        );

        let worktrees: Vec<Entity<Worktree>> = self
            .worktree_store
            .read(cx)
            .worktrees()
            .filter_map(|worktree| {
                self.language_servers_for_worktree(worktree.read(cx).id())
                    .find(|server| server.server_id() == language_server_id)
                    .map(|_| worktree)
            })
            .collect();

        for watcher in &params.watchers {
            self.register_watcher(&worktrees, watcher, registration_id, language_server_id, cx);
        }

        let registrations = self
            .language_server_dynamic_registrations
            .entry(language_server_id)
            .or_default();
        registrations
            .did_change_watched_files
            .insert(registration_id.to_string());

        cx.notify();
    }

    fn on_lsp_unregister_did_change_watched_files(
        &mut self,
        language_server_id: LanguageServerId,
        registration_id: &str,
        cx: &mut Context<LspStore>,
    ) {
        let Some(registrations) = self
            .language_server_dynamic_registrations
            .get_mut(&language_server_id)
        else {
            return;
        };

        if registrations
            .did_change_watched_files
            .remove(registration_id)
        {
            log::info!(
                "language server {}: unregistered workspace/DidChangeWatchedFiles capability with id {}",
                language_server_id,
                registration_id
            );
        } else {
            log::warn!(
                "language server {}: failed to unregister workspace/DidChangeWatchedFiles capability with id {}. not registered.",
                language_server_id,
                registration_id
            );
            return;
        }

        if let Some(watched) = self
            .language_server_watched_paths
            .get_mut(&language_server_id)
        {
            watched.worktree_paths.retain(|_, glob_set| {
                glob_set.remove(registration_id);
                !glob_set.is_empty()
            });
            watched.abs_paths.retain(|_, (glob_set, _)| {
                glob_set.remove(registration_id);
                !glob_set.is_empty()
            });
        }

        cx.notify();
    }

    async fn initialization_options_for_adapter(
        adapter: Arc<dyn LspAdapter>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<Option<serde_json::Value>> {
        let Some(mut initialization_config) =
            adapter.clone().initialization_options(delegate, cx).await?
        else {
            return Ok(None);
        };

        for other_adapter in delegate.registered_lsp_adapters() {
            if other_adapter.name() == adapter.name() {
                continue;
            }
            if let Ok(Some(target_config)) = other_adapter
                .clone()
                .additional_initialization_options(adapter.name(), delegate)
                .await
            {
                merge_json_value_into(target_config.clone(), &mut initialization_config);
            }
        }

        Ok(Some(initialization_config))
    }

    async fn workspace_configuration_for_adapter(
        adapter: Arc<dyn LspAdapter>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        toolchain: Option<Toolchain>,
        requested_uri: Option<Uri>,
        cx: &mut AsyncApp,
    ) -> Result<serde_json::Value> {
        let mut workspace_config = adapter
            .clone()
            .workspace_configuration(delegate, toolchain, requested_uri, cx)
            .await?;

        for other_adapter in delegate.registered_lsp_adapters() {
            if other_adapter.name() == adapter.name() {
                continue;
            }
            if let Ok(Some(target_config)) = other_adapter
                .clone()
                .additional_workspace_configuration(adapter.name(), delegate, cx)
                .await
            {
                merge_json_value_into(target_config.clone(), &mut workspace_config);
            }
        }

        Ok(workspace_config)
    }

    fn language_server_for_id(&self, id: LanguageServerId) -> Option<Arc<LanguageServer>> {
        if let Some(LanguageServerState::Running { server, .. }) = self.language_servers.get(&id) {
            Some(server.clone())
        } else if let Some((_, server)) = self.supplementary_language_servers.get(&id) {
            Some(Arc::clone(server))
        } else {
            None
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

    fn is_capable_for_proto_request<R>(
        &self,
        buffer: &Entity<Buffer>,
        request: &R,
        cx: &App,
    ) -> bool
    where
        R: LspCommand,
    {
        self.check_if_capable_for_proto_request(
            buffer,
            |capabilities| {
                request.check_capabilities(AdapterServerCapabilities {
                    server_capabilities: capabilities.clone(),
                    code_action_kinds: None,
                })
            },
            cx,
        )
    }

    fn relevant_server_ids_for_capability_check(
        &self,
        buffer: &Entity<Buffer>,
        cx: &App,
    ) -> Vec<LanguageServerId> {
        let buffer_id = buffer.read(cx).remote_id();
        if let Some(local) = self.as_local() {
            return local
                .buffers_opened_in_servers
                .get(&buffer_id)
                .into_iter()
                .flatten()
                .copied()
                .collect();
        }

        let Some(language) = buffer.read(cx).language().cloned() else {
            return Vec::default();
        };
        let registered_language_servers = self
            .languages
            .lsp_adapters(&language.name())
            .into_iter()
            .map(|lsp_adapter| lsp_adapter.name())
            .collect::<HashSet<_>>();
        self.language_server_statuses
            .iter()
            .filter_map(|(server_id, server_status)| {
                registered_language_servers
                    .contains(&server_status.name)
                    .then_some(*server_id)
            })
            .collect()
    }

    fn check_if_any_relevant_server_matches<F>(
        &self,
        buffer: &Entity<Buffer>,
        mut check: F,
        cx: &App,
    ) -> bool
    where
        F: FnMut(&LanguageServerStatus, &lsp::ServerCapabilities) -> bool,
    {
        self.relevant_server_ids_for_capability_check(buffer, cx)
            .into_iter()
            .filter_map(|server_id| {
                Some((
                    self.language_server_statuses.get(&server_id)?,
                    self.lsp_server_capabilities.get(&server_id)?,
                ))
            })
            .any(|(server_status, capabilities)| check(server_status, capabilities))
    }

    fn check_if_capable_for_proto_request<F>(
        &self,
        buffer: &Entity<Buffer>,
        mut check: F,
        cx: &App,
    ) -> bool
    where
        F: FnMut(&lsp::ServerCapabilities) -> bool,
    {
        self.check_if_any_relevant_server_matches(buffer, |_, capabilities| check(capabilities), cx)
    }

    pub fn supports_range_formatting(&self, buffer: &Entity<Buffer>, cx: &App) -> bool {
        let settings = LanguageSettings::for_buffer(buffer.read(cx), cx);
        settings.formatter.as_ref().iter().any(|formatter| {
            match formatter {
                Formatter::None => false,
                Formatter::Auto => {
                    settings.prettier.allowed
                        || self.check_if_capable_for_proto_request(
                            buffer,
                            server_capabilities_support_range_formatting,
                            cx,
                        )
                }
                Formatter::Prettier => true,
                Formatter::External { .. } => false,
                Formatter::LanguageServer(settings::LanguageServerFormatterSpecifier::Current) => {
                    self.check_if_capable_for_proto_request(
                        buffer,
                        server_capabilities_support_range_formatting,
                        cx,
                    )
                }
                Formatter::LanguageServer(
                    settings::LanguageServerFormatterSpecifier::Specific { name },
                ) => self.check_if_any_relevant_server_matches(
                    buffer,
                    |server_status, capabilities| {
                        server_status.name.0.as_ref() == name
                            && server_capabilities_support_range_formatting(capabilities)
                    },
                    cx,
                ),
                // `FormatSelections` should only surface when a formatter can honor the
                // selected ranges. Code actions can still run as part of formatting, but
                // they operate on the whole buffer rather than the selected text.
                Formatter::CodeAction(_) => false,
            }
        })
    }

    fn all_capable_for_proto_request<F>(
        &self,
        buffer: &Entity<Buffer>,
        mut check: F,
        cx: &App,
    ) -> Vec<(lsp::LanguageServerId, lsp::LanguageServerName)>
    where
        F: FnMut(&lsp::LanguageServerName, &lsp::ServerCapabilities) -> bool,
    {
        self.relevant_server_ids_for_capability_check(buffer, cx)
            .into_iter()
            .filter_map(|server_id| {
                Some((
                    server_id,
                    &self.language_server_statuses.get(&server_id)?.name,
                    self.lsp_server_capabilities.get(&server_id)?,
                ))
            })
            .filter(|(_, server_name, capabilities)| check(server_name, capabilities))
            .map(|(server_id, server_name, _)| (server_id, server_name.clone()))
            .collect()
    }

    pub fn request_lsp<R>(
        &mut self,
        buffer: Entity<Buffer>,
        server: LanguageServerToQuery,
        request: R,
        cx: &mut Context<Self>,
    ) -> Task<Result<R::Response>>
    where
        R: LspCommand,
        <R::LspRequest as lsp::request::Request>::Result: Send,
        <R::LspRequest as lsp::request::Request>::Params: Send,
    {
        if let Some((upstream_client, upstream_project_id)) = self.upstream_client() {
            return self.send_lsp_proto_request(
                buffer,
                upstream_client,
                upstream_project_id,
                request,
                cx,
            );
        }

        let Some(language_server) = buffer.update(cx, |buffer, cx| match server {
            LanguageServerToQuery::FirstCapable => self.as_local().and_then(|local| {
                local
                    .language_servers_for_buffer(buffer, cx)
                    .find(|(_, server)| {
                        request.check_capabilities(server.adapter_server_capabilities())
                    })
                    .map(|(_, server)| server.clone())
            }),
            LanguageServerToQuery::Other(id) => self
                .language_server_for_local_buffer(buffer, id, cx)
                .and_then(|(_, server)| {
                    request
                        .check_capabilities(server.adapter_server_capabilities())
                        .then(|| Arc::clone(server))
                }),
        }) else {
            return Task::ready(Ok(Default::default()));
        };

        let file = File::from_dyn(buffer.read(cx).file()).and_then(File::as_local);

        let Some(file) = file else {
            return Task::ready(Ok(Default::default()));
        };

        let lsp_params = match request.to_lsp_params_or_response(
            &file.abs_path(cx),
            buffer.read(cx),
            &language_server,
            cx,
        ) {
            Ok(LspParamsOrResponse::Params(lsp_params)) => lsp_params,
            Ok(LspParamsOrResponse::Response(response)) => return Task::ready(Ok(response)),
            Err(err) => {
                let message = format!(
                    "{} via {} failed: {}",
                    request.display_name(),
                    language_server.name(),
                    err
                );
                if should_log_lsp_request_failure(&message) {
                    log::warn!("{message}");
                }
                return Task::ready(Err(anyhow!(message)));
            }
        };

        let status = request.status();
        let request_timeout = ProjectSettings::get_global(cx)
            .global_lsp_settings
            .get_request_timeout();

        cx.spawn(async move |this, cx| {
            let lsp_request = language_server.request::<R::LspRequest>(lsp_params, request_timeout);

            let id = lsp_request.id();
            let _cleanup = if status.is_some() {
                cx.update(|cx| {
                    this.update(cx, |this, cx| {
                        this.on_lsp_work_start(
                            language_server.server_id(),
                            ProgressToken::Number(id),
                            LanguageServerProgress {
                                is_disk_based_diagnostics_progress: false,
                                is_cancellable: false,
                                title: None,
                                message: status.clone(),
                                percentage: None,
                                last_update_at: cx.background_executor().now(),
                            },
                            cx,
                        );
                    })
                })
                .log_err();

                Some(defer(|| {
                    cx.update(|cx| {
                        this.update(cx, |this, cx| {
                            this.on_lsp_work_end(
                                language_server.server_id(),
                                ProgressToken::Number(id),
                                cx,
                            );
                        })
                    })
                    .log_err();
                }))
            } else {
                None
            };

            let result = lsp_request.await.into_response();

            let response = result.map_err(|err| {
                let message = format!(
                    "{} via {} failed: {}",
                    request.display_name(),
                    language_server.name(),
                    err
                );
                if should_log_lsp_request_failure(&message) {
                    log::warn!("{message}");
                }
                anyhow::anyhow!(message)
            })?;

            request
                .response_from_lsp(
                    response,
                    this.upgrade().context("no app context")?,
                    buffer,
                    language_server.server_id(),
                    cx.clone(),
                )
                .await
        })
    }

    fn on_settings_changed(&mut self, cx: &mut Context<Self>) {
        let mut language_formatters_to_check = Vec::new();
        for buffer in self.buffer_store.read(cx).buffers() {
            let buffer = buffer.read(cx);
            let settings = LanguageSettings::for_buffer(buffer, cx);
            if buffer.language().is_some() {
                let buffer_file = File::from_dyn(buffer.file());
                language_formatters_to_check.push((
                    buffer_file.map(|f| f.worktree_id(cx)),
                    settings.into_owned(),
                ));
            }
        }

        self.request_workspace_config_refresh();

        if let Some(prettier_store) = self.as_local().map(|s| s.prettier_store.clone()) {
            prettier_store.update(cx, |prettier_store, cx| {
                prettier_store.on_settings_changed(language_formatters_to_check, cx)
            })
        }

        let new_semantic_token_rules = crate::project_settings::ProjectSettings::get_global(cx)
            .global_lsp_settings
            .semantic_token_rules
            .clone();
        self.semantic_token_config
            .update_rules(new_semantic_token_rules);
        // Always clear cached stylizers so that changes to language-specific
        // semantic token rules (e.g. from extension install/uninstall) are
        // picked up. Stylizers are recreated lazily, so this is cheap.
        self.semantic_token_config.clear_stylizers();

        let new_global_semantic_tokens_mode =
            all_language_settings(None, cx).defaults.semantic_tokens;
        if self
            .semantic_token_config
            .update_global_mode(new_global_semantic_tokens_mode)
        {
            let all_stopped = self
                .as_local()
                .is_some_and(|local| local.all_language_servers_stopped);
            if !all_stopped {
                // Restart servers without clearing per-server stopped status.
                // Individually-stopped servers will be skipped by the guard in
                // register_buffer_with_language_servers.
                let buffers = self.buffer_store.read(cx).buffers().collect();
                self.restart_language_servers_for_buffers(buffers, HashSet::default(), false, cx);
            }
        }

        cx.notify();
    }

    fn refresh_server_tree(&mut self, cx: &mut Context<Self>) {
        let buffer_store = self.buffer_store.clone();
        let Some(local) = self.as_local_mut() else {
            return;
        };
        if local.all_language_servers_stopped {
            return;
        }
        let stopped_language_servers = local.stopped_language_servers.clone();
        let mut adapters = BTreeMap::default();
        let get_adapter = {
            let languages = local.languages.clone();
            let environment = local.environment.clone();
            let weak = local.weak.clone();
            let worktree_store = local.worktree_store.clone();
            let http_client = local.http_client.clone();
            let fs = local.fs.clone();
            move |worktree_id, cx: &mut App| {
                let worktree = worktree_store.read(cx).worktree_for_id(worktree_id, cx)?;
                Some(LocalLspAdapterDelegate::new(
                    languages.clone(),
                    &environment,
                    weak.clone(),
                    &worktree,
                    http_client.clone(),
                    fs.clone(),
                    cx,
                ))
            }
        };

        let mut messages_to_report = Vec::new();
        let (new_tree, to_stop) = {
            let mut rebase = local.lsp_tree.rebase();
            let buffers = buffer_store
                .read(cx)
                .buffers()
                .filter_map(|buffer| {
                    let raw_buffer = buffer.read(cx);
                    if !local
                        .registered_buffers
                        .contains_key(&raw_buffer.remote_id())
                    {
                        return None;
                    }
                    let file = File::from_dyn(raw_buffer.file()).cloned()?;
                    let language = raw_buffer.language().cloned()?;
                    Some((file, language, raw_buffer.remote_id()))
                })
                .sorted_by_key(|(file, _, _)| Reverse(file.worktree.read(cx).is_visible()));
            for (file, language, buffer_id) in buffers {
                let worktree_id = file.worktree_id(cx);
                let Some(worktree) = local
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(worktree_id, cx)
                else {
                    continue;
                };

                if let Some((_, apply)) = local.reuse_existing_language_server(
                    rebase.server_tree(),
                    &worktree,
                    &language.name(),
                    cx,
                ) {
                    (apply)(rebase.server_tree());
                } else if let Some(lsp_delegate) = adapters
                    .entry(worktree_id)
                    .or_insert_with(|| get_adapter(worktree_id, cx))
                    .clone()
                {
                    let delegate =
                        Arc::new(ManifestQueryDelegate::new(worktree.read(cx).snapshot()));
                    let path = file
                        .path()
                        .parent()
                        .map(Arc::from)
                        .unwrap_or_else(|| file.path().clone());
                    let worktree_path = ProjectPath { worktree_id, path };
                    let abs_path = file.abs_path(cx);
                    let nodes = rebase
                        .walk(
                            worktree_path,
                            language.name(),
                            language.manifest(),
                            delegate.clone(),
                            cx,
                        )
                        .collect::<Vec<_>>();
                    for node in nodes {
                        if let Some(name) = node.name()
                            && stopped_language_servers.contains(&name)
                        {
                            continue;
                        }
                        let server_id = node.server_id_or_init(|disposition| {
                            let path = &disposition.path;
                            let uri = Uri::from_file_path(worktree.read(cx).absolutize(&path.path));
                            let key = LanguageServerSeed {
                                worktree_id,
                                name: disposition.server_name.clone(),
                                settings: LanguageServerSeedSettings {
                                    binary: disposition.settings.binary.clone(),
                                    initialization_options: disposition
                                        .settings
                                        .initialization_options
                                        .clone(),
                                },
                                toolchain: local.toolchain_store.read(cx).active_toolchain(
                                    path.worktree_id,
                                    &path.path,
                                    language.name(),
                                ),
                            };
                            local.language_server_ids.remove(&key);

                            let server_id = local.get_or_insert_language_server(
                                &worktree,
                                lsp_delegate.clone(),
                                disposition,
                                &language.name(),
                                cx,
                            );
                            if let Some(state) = local.language_servers.get(&server_id)
                                && let Ok(uri) = uri
                            {
                                state.add_workspace_folder(uri);
                            };
                            server_id
                        });

                        if let Some(language_server_id) = server_id {
                            messages_to_report.push(LspStoreEvent::LanguageServerUpdate {
                                language_server_id,
                                name: node.name(),
                                message:
                                    proto::update_language_server::Variant::RegisteredForBuffer(
                                        proto::RegisteredForBuffer {
                                            buffer_abs_path: abs_path
                                                .to_string_lossy()
                                                .into_owned(),
                                            buffer_id: buffer_id.to_proto(),
                                        },
                                    ),
                            });
                        }
                    }
                } else {
                    continue;
                }
            }
            rebase.finish()
        };
        for message in messages_to_report {
            cx.emit(message);
        }
        local.lsp_tree = new_tree;
        for (id, _) in to_stop {
            self.stop_local_language_server(id, cx).detach();
        }
    }

    pub fn apply_code_action(
        &self,
        buffer_handle: Entity<Buffer>,
        mut action: CodeAction,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectTransaction>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = proto::ApplyCodeAction {
                project_id,
                buffer_id: buffer_handle.read(cx).remote_id().into(),
                action: Some(Self::serialize_code_action(&action)),
            };
            let buffer_store = self.buffer_store();
            cx.spawn(async move |_, cx| {
                let response = upstream_client
                    .request(request)
                    .await?
                    .transaction
                    .context("missing transaction")?;

                buffer_store
                    .update(cx, |buffer_store, cx| {
                        buffer_store.deserialize_project_transaction(response, push_to_history, cx)
                    })
                    .await
            })
        } else if self.mode.is_local() {
            let Some((_, lang_server, request_timeout)) = buffer_handle.update(cx, |buffer, cx| {
                let request_timeout = ProjectSettings::get_global(cx)
                    .global_lsp_settings
                    .get_request_timeout();
                self.language_server_for_local_buffer(buffer, action.server_id, cx)
                    .map(|(adapter, server)| (adapter.clone(), server.clone(), request_timeout))
            }) else {
                return Task::ready(Ok(ProjectTransaction::default()));
            };

            cx.spawn(async move |this, cx| {
                LocalLspStore::try_resolve_code_action(&lang_server, &mut action, request_timeout)
                    .await
                    .context("resolving a code action")?;
                if let Some(edit) = action.lsp_action.edit()
                    && (edit.changes.is_some() || edit.document_changes.is_some())
                {
                    return LocalLspStore::deserialize_workspace_edit(
                        this.upgrade().context("no app present")?,
                        edit.clone(),
                        push_to_history,
                        lang_server.clone(),
                        cx,
                    )
                    .await;
                }

                let Some(command) = action.lsp_action.command() else {
                    return Ok(ProjectTransaction::default());
                };

                let server_capabilities = lang_server.capabilities();
                let available_commands = server_capabilities
                    .execute_command_provider
                    .as_ref()
                    .map(|options| options.commands.as_slice())
                    .unwrap_or_default();

                if !available_commands.contains(&command.command) {
                    log::debug!(
                        "Skipping executeCommand for {}, not listed in language server capabilities",
                        command.command
                    );
                    return Ok(ProjectTransaction::default());
                }

                let request_timeout = cx.update(|app| {
                    ProjectSettings::get_global(app)
                        .global_lsp_settings
                        .get_request_timeout()
                });

                this.update(cx, |this, _| {
                    this.as_local_mut()
                        .unwrap()
                        .last_workspace_edits_by_language_server
                        .remove(&lang_server.server_id());
                })?;

                let _result = lang_server
                    .request::<lsp::request::ExecuteCommand>(
                        lsp::ExecuteCommandParams {
                            command: command.command.clone(),
                            arguments: command.arguments.clone().unwrap_or_default(),
                            ..lsp::ExecuteCommandParams::default()
                        },
                        request_timeout,
                    )
                    .await
                    .into_response()
                    .context("execute command")?;

                return this.update(cx, |this, _| {
                    this.as_local_mut()
                        .unwrap()
                        .last_workspace_edits_by_language_server
                        .remove(&lang_server.server_id())
                        .unwrap_or_default()
                });
            })
        } else {
            Task::ready(Err(anyhow!("no upstream client and not local")))
        }
    }

    pub fn resolve_code_action(
        &self,
        buffer: &Entity<Buffer>,
        mut action: CodeAction,
        cx: &mut Context<Self>,
    ) -> Task<Result<CodeAction>> {
        if action.resolved {
            return Task::ready(Ok(action));
        }
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = proto::ResolveCodeAction {
                project_id,
                buffer_id: buffer.read(cx).remote_id().into(),
                action: Some(Self::serialize_code_action(&action)),
            };
            cx.background_spawn(async move {
                let response = upstream_client
                    .request(request)
                    .await
                    .context("resolve code action proto request")?;
                let action = response.action.context("missing resolved action")?;
                Self::deserialize_code_action(action)
            })
        } else if self.mode.is_local() {
            let server_id = action.server_id;
            let Some(lang_server) = buffer.update(cx, |buffer, cx| {
                self.language_server_for_local_buffer(buffer, server_id, cx)
                    .map(|(_, server)| server.clone())
            }) else {
                return Task::ready(Ok(action));
            };
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            cx.background_spawn(async move {
                LocalLspStore::try_resolve_code_action(&lang_server, &mut action, request_timeout)
                    .await
                    .context("resolving a code action")?;
                Ok(action)
            })
        } else {
            Task::ready(Err(anyhow!("no upstream client and not local")))
        }
    }

    pub(super) async fn handle_resolve_code_action(
        lsp_store: Entity<Self>,
        envelope: TypedEnvelope<proto::ResolveCodeAction>,
        mut cx: AsyncApp,
    ) -> Result<proto::ResolveCodeActionResponse> {
        let action =
            Self::deserialize_code_action(envelope.payload.action.context("invalid action")?)?;
        let buffer = lsp_store.update(&mut cx, |lsp_store, cx| {
            let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
            lsp_store.buffer_store.read(cx).get_existing(buffer_id)
        })?;
        let resolved = lsp_store
            .update(&mut cx, |lsp_store, cx| {
                lsp_store.resolve_code_action(&buffer, action, cx)
            })
            .await
            .context("resolving code action")?;
        Ok(proto::ResolveCodeActionResponse {
            action: Some(Self::serialize_code_action(&resolved)),
        })
    }

    pub fn apply_code_action_kind(
        &mut self,
        buffers: HashSet<Entity<Buffer>>,
        kind: CodeActionKind,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<ProjectTransaction>> {
        if self.as_local().is_some() {
            cx.spawn(async move |lsp_store, cx| {
                let buffers = buffers.into_iter().collect::<Vec<_>>();
                let result = LocalLspStore::execute_code_action_kind_locally(
                    lsp_store.clone(),
                    buffers,
                    kind,
                    push_to_history,
                    cx,
                )
                .await;
                lsp_store.update(cx, |lsp_store, _| {
                    lsp_store.update_last_formatting_failure(&result);
                })?;
                result
            })
        } else if let Some((client, project_id)) = self.upstream_client() {
            let buffer_store = self.buffer_store();
            cx.spawn(async move |lsp_store, cx| {
                let result = client
                    .request(proto::ApplyCodeActionKind {
                        project_id,
                        kind: kind.as_str().to_owned(),
                        buffer_ids: buffers
                            .iter()
                            .map(|buffer| {
                                buffer.read_with(cx, |buffer, _| buffer.remote_id().into())
                            })
                            .collect(),
                    })
                    .await
                    .and_then(|result| result.transaction.context("missing transaction"));
                lsp_store.update(cx, |lsp_store, _| {
                    lsp_store.update_last_formatting_failure(&result);
                })?;

                let transaction_response = result?;
                buffer_store
                    .update(cx, |buffer_store, cx| {
                        buffer_store.deserialize_project_transaction(
                            transaction_response,
                            push_to_history,
                            cx,
                        )
                    })
                    .await
            })
        } else {
            Task::ready(Ok(ProjectTransaction::default()))
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

    pub fn definitions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        self.definitions_with_filter(buffer, position, false, cx)
    }

    pub fn workspace_definitions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        self.definitions_with_filter(buffer, position, true, cx)
    }

    fn definitions_with_filter(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        workspace_only: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetDefinitions {
                position,
                workspace_only,
            };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }

            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();

            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetDefinitions {
                        position,
                        workspace_only,
                    }
                    .response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .dedup()
                        .collect(),
                ))
            })
        } else {
            let definitions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetDefinitions {
                    position,
                    workspace_only,
                },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    definitions_task
                        .await
                        .into_iter()
                        .flat_map(|(_, definitions)| definitions)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn declarations(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetDeclarations { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetDeclarations { position }.response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .dedup()
                        .collect(),
                ))
            })
        } else {
            let declarations_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetDeclarations { position },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    declarations_task
                        .await
                        .into_iter()
                        .flat_map(|(_, declarations)| declarations)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn type_definitions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        self.type_definitions_with_filter(buffer, position, false, cx)
    }

    pub fn workspace_type_definitions(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        self.type_definitions_with_filter(buffer, position, true, cx)
    }

    fn type_definitions_with_filter(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        workspace_only: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetTypeDefinitions {
                position,
                workspace_only,
            };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetTypeDefinitions {
                        position,
                        workspace_only,
                    }
                    .response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .dedup()
                        .collect(),
                ))
            })
        } else {
            let type_definitions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetTypeDefinitions {
                    position,
                    workspace_only,
                },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    type_definitions_task
                        .await
                        .into_iter()
                        .flat_map(|(_, type_definitions)| type_definitions)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn implementations(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LocationLink>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetImplementations { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }

            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetImplementations { position }.response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .dedup()
                        .collect(),
                ))
            })
        } else {
            let implementations_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetImplementations { position },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    implementations_task
                        .await
                        .into_iter()
                        .flat_map(|(_, implementations)| implementations)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn references(
        &mut self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<Location>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetReferences { position };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }

            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };

                let locations = join_all(responses.payload.into_iter().map(|lsp_response| {
                    GetReferences { position }.response_from_proto(
                        lsp_response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<Vec<_>>>>()?
                .into_iter()
                .flatten()
                .dedup()
                .collect();
                Ok(Some(locations))
            })
        } else {
            let references_task = self.request_multiple_lsp_locally(
                buffer,
                Some(position),
                GetReferences { position },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    references_task
                        .await
                        .into_iter()
                        .flat_map(|(_, references)| references)
                        .dedup()
                        .collect(),
                ))
            })
        }
    }

    pub fn code_actions(
        &mut self,
        buffer: &Entity<Buffer>,
        range: Range<Anchor>,
        kinds: Option<Vec<CodeActionKind>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<CodeAction>>>> {
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let request = GetCodeActions {
                range: range.clone(),
                kinds: kinds.clone(),
            };
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(None));
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                None,
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(None);
                };
                let Some(responses) = request_task.await? else {
                    return Ok(None);
                };
                let actions = join_all(responses.payload.into_iter().map(|response| {
                    GetCodeActions {
                        range: range.clone(),
                        kinds: kinds.clone(),
                    }
                    .response_from_proto(
                        response.response,
                        lsp_store.clone(),
                        buffer.clone(),
                        cx.clone(),
                    )
                }))
                .await;

                Ok(Some(
                    actions
                        .into_iter()
                        .collect::<Result<Vec<Vec<_>>>>()?
                        .into_iter()
                        .flatten()
                        .collect(),
                ))
            })
        } else {
            let all_actions_task = self.request_multiple_lsp_locally(
                buffer,
                Some(range.start),
                GetCodeActions { range, kinds },
                cx,
            );
            cx.background_spawn(async move {
                Ok(Some(
                    all_actions_task
                        .await
                        .into_iter()
                        .flat_map(|(_, actions)| actions)
                        .collect(),
                ))
            })
        }
    }

    #[inline(never)]
    pub fn completions(
        &self,
        buffer: &Entity<Buffer>,
        position: PointUtf16,
        context: CompletionContext,
        cx: &mut Context<Self>,
    ) -> Task<Result<Vec<CompletionResponse>>> {
        let language_registry = self.languages.clone();

        if let Some((upstream_client, project_id)) = self.upstream_client() {
            let snapshot = buffer.read(cx).snapshot();
            let offset = position.to_offset(&snapshot);
            let scope = snapshot.language_scope_at(offset);
            let capable_lsps = self.all_capable_for_proto_request(
                buffer,
                |server_name, capabilities| {
                    capabilities.completion_provider.is_some()
                        && scope
                            .as_ref()
                            .map(|scope| scope.language_allowed(server_name))
                            .unwrap_or(true)
                },
                cx,
            );
            if capable_lsps.is_empty() {
                return Task::ready(Ok(Vec::new()));
            }

            let language = buffer.read(cx).language().cloned();

            let buffer = buffer.clone();

            cx.spawn(async move |this, cx| {
                let requests = join_all(
                    capable_lsps
                        .into_iter()
                        .map(|(id, server_name)| {
                            let request = GetCompletions {
                                position,
                                context: context.clone(),
                                server_id: Some(id),
                            };
                            let buffer = buffer.clone();
                            let language = language.clone();
                            let lsp_adapter = language.as_ref().and_then(|language| {
                                let adapters = language_registry.lsp_adapters(&language.name());
                                adapters
                                    .iter()
                                    .find(|adapter| adapter.name() == server_name)
                                    .or_else(|| adapters.first())
                                    .cloned()
                            });
                            let upstream_client = upstream_client.clone();
                            let response = this
                                .update(cx, |this, cx| {
                                    this.send_lsp_proto_request(
                                        buffer,
                                        upstream_client,
                                        project_id,
                                        request,
                                        cx,
                                    )
                                })
                                .log_err();
                            async move {
                                let response = response?.await.log_err()?;

                                let completions = populate_labels_for_completions(
                                    response.completions,
                                    language,
                                    lsp_adapter,
                                )
                                .await;

                                Some(CompletionResponse {
                                    completions,
                                    display_options: CompletionDisplayOptions::default(),
                                    is_incomplete: response.is_incomplete,
                                })
                            }
                        })
                        .collect::<Vec<_>>(),
                );
                Ok(requests.await.into_iter().flatten().collect::<Vec<_>>())
            })
        } else if let Some(local) = self.as_local() {
            let snapshot = buffer.read(cx).snapshot();
            let offset = position.to_offset(&snapshot);
            let scope = snapshot.language_scope_at(offset);
            let language = snapshot.language().cloned();
            let completion_settings = LanguageSettings::for_buffer(&buffer.read(cx), cx)
                .completions
                .clone();
            if !completion_settings.lsp {
                return Task::ready(Ok(Vec::new()));
            }

            let server_ids: Vec<_> = buffer.update(cx, |buffer, cx| {
                local
                    .language_servers_for_buffer(buffer, cx)
                    .filter(|(_, server)| server.capabilities().completion_provider.is_some())
                    .filter(|(adapter, _)| {
                        scope
                            .as_ref()
                            .map(|scope| scope.language_allowed(&adapter.name))
                            .unwrap_or(true)
                    })
                    .map(|(_, server)| server.server_id())
                    .collect()
            });

            let buffer = buffer.clone();
            let lsp_timeout = completion_settings.lsp_fetch_timeout_ms;
            let lsp_timeout = if lsp_timeout > 0 {
                Some(Duration::from_millis(lsp_timeout))
            } else {
                None
            };
            cx.spawn(async move |this,  cx| {
                let mut tasks = Vec::with_capacity(server_ids.len());
                this.update(cx, |lsp_store, cx| {
                    for server_id in server_ids {
                        let lsp_adapter = lsp_store.language_server_adapter_for_id(server_id);
                        let lsp_timeout = lsp_timeout
                            .map(|lsp_timeout| cx.background_executor().timer(lsp_timeout));
                        let mut timeout = cx.background_spawn(async move {
                            match lsp_timeout {
                                Some(lsp_timeout) => {
                                    lsp_timeout.await;
                                    true
                                },
                                None => false,
                            }
                        }).fuse();
                        let mut lsp_request = lsp_store.request_lsp(
                            buffer.clone(),
                            LanguageServerToQuery::Other(server_id),
                            GetCompletions {
                                position,
                                context: context.clone(),
                                server_id: Some(server_id),
                            },
                            cx,
                        ).fuse();
                        let new_task = cx.background_spawn(async move {
                            select_biased! {
                                response = lsp_request => anyhow::Ok(Some(response?)),
                                timeout_happened = timeout => {
                                    if timeout_happened {
                                        log::warn!("Fetching completions from server {server_id} timed out, timeout ms: {}", completion_settings.lsp_fetch_timeout_ms);
                                        Ok(None)
                                    } else {
                                        let completions = lsp_request.await?;
                                        Ok(Some(completions))
                                    }
                                },
                            }
                        });
                        tasks.push((lsp_adapter, new_task));
                    }
                })?;

                let futures = tasks.into_iter().map(async |(lsp_adapter, task)| {
                    let completion_response = task.await.ok()??;
                    let completions = populate_labels_for_completions(
                            completion_response.completions,
                            language.clone(),
                            lsp_adapter,
                        )
                        .await;
                    Some(CompletionResponse {
                        completions,
                        display_options: CompletionDisplayOptions::default(),
                        is_incomplete: completion_response.is_incomplete,
                    })
                });

                let responses: Vec<Option<CompletionResponse>> = join_all(futures).await;

                Ok(responses.into_iter().flatten().collect())
            })
        } else {
            Task::ready(Err(anyhow!("No upstream client or local language server")))
        }
    }

    pub fn resolve_completions(
        &self,
        buffer: Entity<Buffer>,
        completion_indices: Vec<usize>,
        completions: Rc<RefCell<Box<[Completion]>>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<bool>> {
        let client = self.upstream_client();
        let buffer_id = buffer.read(cx).remote_id();
        let buffer_snapshot = buffer.read(cx).snapshot();

        if !self.check_if_capable_for_proto_request(
            &buffer,
            GetCompletions::can_resolve_completions,
            cx,
        ) {
            return Task::ready(Ok(false));
        }
        cx.spawn(async move |lsp_store, cx| {
            let request_timeout = cx.update(|app| {
                ProjectSettings::get_global(app)
                    .global_lsp_settings
                    .get_request_timeout()
            });

            let mut did_resolve = false;
            if let Some((client, project_id)) = client {
                for completion_index in completion_indices {
                    let server_id = {
                        let completion = &completions.borrow()[completion_index];
                        completion.source.server_id()
                    };
                    if let Some(server_id) = server_id {
                        if Self::resolve_completion_remote(
                            project_id,
                            server_id,
                            buffer_id,
                            completions.clone(),
                            completion_index,
                            client.clone(),
                        )
                        .await
                        .log_err()
                        .is_some()
                        {
                            did_resolve = true;
                        }
                    } else {
                        resolve_word_completion(
                            &buffer_snapshot,
                            &mut completions.borrow_mut()[completion_index],
                        );
                    }
                }
            } else {
                for completion_index in completion_indices {
                    let server_id = {
                        let completion = &completions.borrow()[completion_index];
                        completion.source.server_id()
                    };
                    if let Some(server_id) = server_id {
                        let server_and_adapter = lsp_store
                            .read_with(cx, |lsp_store, _| {
                                let server = lsp_store.language_server_for_id(server_id)?;
                                let adapter =
                                    lsp_store.language_server_adapter_for_id(server.server_id())?;
                                Some((server, adapter))
                            })
                            .ok()
                            .flatten();
                        let Some((server, adapter)) = server_and_adapter else {
                            continue;
                        };

                        let resolved = Self::resolve_completion_local(
                            server,
                            completions.clone(),
                            completion_index,
                            request_timeout,
                        )
                        .await
                        .log_err()
                        .is_some();
                        if resolved {
                            Self::regenerate_completion_labels(
                                adapter,
                                &buffer_snapshot,
                                completions.clone(),
                                completion_index,
                            )
                            .await
                            .log_err();
                            did_resolve = true;
                        }
                    } else {
                        resolve_word_completion(
                            &buffer_snapshot,
                            &mut completions.borrow_mut()[completion_index],
                        );
                    }
                }
            }

            Ok(did_resolve)
        })
    }

    async fn resolve_completion_local(
        server: Arc<lsp::LanguageServer>,
        completions: Rc<RefCell<Box<[Completion]>>>,
        completion_index: usize,
        request_timeout: Duration,
    ) -> Result<()> {
        let server_id = server.server_id();
        if !GetCompletions::can_resolve_completions(&server.capabilities()) {
            return Ok(());
        }

        let request = {
            let completion = &completions.borrow()[completion_index];
            match &completion.source {
                CompletionSource::Lsp {
                    lsp_completion,
                    resolved,
                    server_id: completion_server_id,
                    ..
                } => {
                    if *resolved {
                        return Ok(());
                    }
                    anyhow::ensure!(
                        server_id == *completion_server_id,
                        "server_id mismatch, querying completion resolve for {server_id} but completion server id is {completion_server_id}"
                    );
                    server.request::<lsp::request::ResolveCompletionItem>(
                        *lsp_completion.clone(),
                        request_timeout,
                    )
                }
                CompletionSource::BufferWord { .. }
                | CompletionSource::Dap { .. }
                | CompletionSource::Custom => {
                    return Ok(());
                }
            }
        };
        let resolved_completion = request
            .await
            .into_response()
            .context("resolve completion")?;

        let mut completions = completions.borrow_mut();
        let completion = &mut completions[completion_index];
        if let CompletionSource::Lsp {
            lsp_completion,
            resolved,
            server_id: completion_server_id,
            ..
        } = &mut completion.source
        {
            if *resolved {
                return Ok(());
            }
            anyhow::ensure!(
                server_id == *completion_server_id,
                "server_id mismatch, applying completion resolve for {server_id} but completion server id is {completion_server_id}"
            );
            **lsp_completion = resolved_completion;
            *resolved = true;

            // We must not use any data such as sortText, filterText, insertText and textEdit to edit `Completion` since they are not supposed to change during resolve.
            // Refer: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_completion
            //
            // We still re-derive new_text here as a workaround for the specific
            // VS Code TypeScript completion resolve flow that vtsls wraps:
            // https://github.com/microsoft/vscode/blob/838b48504cd9a2338e2ca9e854da9cec990c4d57/extensions/typescript-language-features/src/languageFeatures/completions.ts#L218
            //
            // Some servers (e.g. vtsls with completeFunctionCalls) update
            // insertText/textEdit during resolve to add snippet content like
            // function call parentheses.
            //
            // vtsls resolve flow:
            //   https://github.com/yioneko/vtsls/blob/fecf52324a30e72dfab1537047556076720c1a5f/packages/service/src/service/completion.ts#L228-L244
            // vtsls converter (isSnippet / insertTextFormat):
            //   https://github.com/yioneko/vtsls/blob/28e075105d7711d635ebf8aefc971bb8e1d2fe65/packages/service/src/utils/converter.ts#L149-L200
            //
            // NB: We only update the text content here, NOT the replace/insert
            // ranges on `Completion`. Those ranges were converted to anchors from
            // the original response and stay valid across buffer edits. The LSP
            // ranges in the resolved text_edit are stale when completions are
            // cached across keystrokes (see #34094).
            let resolved_new_text = lsp_completion
                .text_edit
                .as_ref()
                .map(|edit| match edit {
                    lsp::CompletionTextEdit::Edit(e) => e.new_text.clone(),
                    lsp::CompletionTextEdit::InsertAndReplace(e) => e.new_text.clone(),
                })
                .or_else(|| lsp_completion.insert_text.clone());
            if let Some(mut resolved_new_text) = resolved_new_text {
                LineEnding::normalize(&mut resolved_new_text);
                completion.new_text = resolved_new_text;
            }
        }
        Ok(())
    }

    async fn regenerate_completion_labels(
        adapter: Arc<CachedLspAdapter>,
        snapshot: &BufferSnapshot,
        completions: Rc<RefCell<Box<[Completion]>>>,
        completion_index: usize,
    ) -> Result<()> {
        let completion_item = completions.borrow()[completion_index]
            .source
            .lsp_completion(true)
            .map(Cow::into_owned);
        if let Some(lsp_documentation) = completion_item
            .as_ref()
            .and_then(|completion_item| completion_item.documentation.clone())
        {
            let mut completions = completions.borrow_mut();
            let completion = &mut completions[completion_index];
            completion.documentation = Some(lsp_documentation.into());
        } else {
            let mut completions = completions.borrow_mut();
            let completion = &mut completions[completion_index];
            completion.documentation = Some(CompletionDocumentation::Undocumented);
        }

        let mut new_label = match completion_item {
            Some(completion_item) => {
                // Some language servers always return `detail` lazily via resolve, regardless of
                // the resolvable properties Mav advertises. Regenerate labels here to handle this.
                // See: https://github.com/yioneko/vtsls/issues/213
                let language = snapshot.language();
                match language {
                    Some(language) => {
                        adapter
                            .labels_for_completions(
                                std::slice::from_ref(&completion_item),
                                language,
                            )
                            .await?
                    }
                    None => Vec::new(),
                }
                .pop()
                .flatten()
                .unwrap_or_else(|| {
                    CodeLabel::fallback_for_completion(
                        &completion_item,
                        language.map(|language| language.as_ref()),
                    )
                })
            }
            None => CodeLabel::plain(
                completions.borrow()[completion_index].new_text.clone(),
                None,
            ),
        };
        ensure_uniform_list_compatible_label(&mut new_label);

        let mut completions = completions.borrow_mut();
        let completion = &mut completions[completion_index];
        if completion.label.filter_text() == new_label.filter_text() {
            completion.label = new_label;
        } else {
            log::error!(
                "Resolved completion changed display label from {} to {}. \
                 Refusing to apply this because it changes the fuzzy match text from {} to {}",
                completion.label.text(),
                new_label.text(),
                completion.label.filter_text(),
                new_label.filter_text()
            );
        }

        Ok(())
    }

    async fn resolve_completion_remote(
        project_id: u64,
        server_id: LanguageServerId,
        buffer_id: BufferId,
        completions: Rc<RefCell<Box<[Completion]>>>,
        completion_index: usize,
        client: AnyProtoClient,
    ) -> Result<()> {
        let lsp_completion = {
            let completion = &completions.borrow()[completion_index];
            match &completion.source {
                CompletionSource::Lsp {
                    lsp_completion,
                    resolved,
                    server_id: completion_server_id,
                    ..
                } => {
                    anyhow::ensure!(
                        server_id == *completion_server_id,
                        "remote server_id mismatch, querying completion resolve for {server_id} but completion server id is {completion_server_id}"
                    );
                    if *resolved {
                        return Ok(());
                    }
                    serde_json::to_string(lsp_completion).unwrap().into_bytes()
                }
                CompletionSource::Custom
                | CompletionSource::Dap { .. }
                | CompletionSource::BufferWord { .. } => {
                    return Ok(());
                }
            }
        };
        let request = proto::ResolveCompletionDocumentation {
            project_id,
            language_server_id: server_id.0 as u64,
            lsp_completion,
            buffer_id: buffer_id.into(),
        };

        let response = client
            .request(request)
            .await
            .context("completion documentation resolve proto request")?;
        let resolved_lsp_completion = serde_json::from_slice(&response.lsp_completion)?;

        let documentation = if response.documentation.is_empty() {
            CompletionDocumentation::Undocumented
        } else if response.documentation_is_markdown {
            CompletionDocumentation::MultiLineMarkdown(response.documentation.into())
        } else if response.documentation.lines().count() <= 1 {
            CompletionDocumentation::SingleLine(response.documentation.into())
        } else {
            CompletionDocumentation::MultiLinePlainText(response.documentation.into())
        };

        let mut completions = completions.borrow_mut();
        let completion = &mut completions[completion_index];
        completion.documentation = Some(documentation);
        if let CompletionSource::Lsp {
            insert_range,
            lsp_completion,
            resolved,
            server_id: completion_server_id,
            lsp_defaults: _,
        } = &mut completion.source
        {
            let completion_insert_range = response
                .old_insert_start
                .and_then(deserialize_anchor)
                .zip(response.old_insert_end.and_then(deserialize_anchor));
            *insert_range = completion_insert_range.map(|(start, end)| start..end);

            if *resolved {
                return Ok(());
            }
            anyhow::ensure!(
                server_id == *completion_server_id,
                "remote server_id mismatch, applying completion resolve for {server_id} but completion server id is {completion_server_id}"
            );
            **lsp_completion = resolved_lsp_completion;
            *resolved = true;
        }

        let replace_range = response
            .old_replace_start
            .and_then(deserialize_anchor)
            .zip(response.old_replace_end.and_then(deserialize_anchor));
        if let Some((old_replace_start, old_replace_end)) = replace_range
            && !response.new_text.is_empty()
        {
            completion.new_text = response.new_text;
            completion.replace_range = old_replace_start..old_replace_end;
        }

        Ok(())
    }

    pub fn apply_additional_edits_for_completion(
        &self,
        buffer_handle: Entity<Buffer>,
        completions: Rc<RefCell<Box<[Completion]>>>,
        completion_index: usize,
        push_to_history: bool,
        all_commit_ranges: Vec<Range<language::Anchor>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Transaction>>> {
        if let Some((client, project_id)) = self.upstream_client() {
            let buffer = buffer_handle.read(cx);
            let buffer_id = buffer.remote_id();
            cx.spawn(async move |_, cx| {
                let request = {
                    let completion = completions.borrow()[completion_index].clone();
                    proto::ApplyCompletionAdditionalEdits {
                        project_id,
                        buffer_id: buffer_id.into(),
                        completion: Some(Self::serialize_completion(&CoreCompletion {
                            replace_range: completion.replace_range,
                            new_text: completion.new_text,
                            source: completion.source,
                        })),
                        all_commit_ranges: all_commit_ranges
                            .iter()
                            .cloned()
                            .map(language::proto::serialize_anchor_range)
                            .collect(),
                    }
                };

                let Some(transaction) = client.request(request).await?.transaction else {
                    return Ok(None);
                };

                let transaction = language::proto::deserialize_transaction(transaction)?;
                buffer_handle
                    .update(cx, |buffer, _| {
                        buffer.wait_for_edits(transaction.edit_ids.iter().copied())
                    })
                    .await?;
                if push_to_history {
                    buffer_handle.update(cx, |buffer, _| {
                        buffer.push_transaction(transaction.clone(), Instant::now());
                        buffer.finalize_last_transaction();
                    });
                }
                Ok(Some(transaction))
            })
        } else {
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();

            let Some(server) = buffer_handle.update(cx, |buffer, cx| {
                let completion = &completions.borrow()[completion_index];
                let server_id = completion.source.server_id()?;
                Some(
                    self.language_server_for_local_buffer(buffer, server_id, cx)?
                        .1
                        .clone(),
                )
            }) else {
                return Task::ready(Ok(None));
            };

            cx.spawn(async move |this, cx| {
                Self::resolve_completion_local(
                    server.clone(),
                    completions.clone(),
                    completion_index,
                    request_timeout,
                )
                .await
                .context("resolving completion")?;
                let completion = completions.borrow()[completion_index].clone();
                let additional_text_edits = completion
                    .source
                    .lsp_completion(true)
                    .as_ref()
                    .and_then(|lsp_completion| lsp_completion.additional_text_edits.clone());
                if let Some(edits) = additional_text_edits {
                    let edits = this
                        .update(cx, |this, cx| {
                            this.as_local_mut().unwrap().edits_from_lsp(
                                &buffer_handle,
                                edits,
                                server.server_id(),
                                None,
                                cx,
                            )
                        })?
                        .await?;

                    buffer_handle.update(cx, |buffer, cx| {
                        buffer.finalize_last_transaction();
                        buffer.start_transaction();

                        for (range, text) in edits {
                            let primary = &completion.replace_range;

                            // Special case: if both ranges start at the very beginning of the file (line 0, column 0),
                            // and the primary completion is just an insertion (empty range), then this is likely
                            // an auto-import scenario and should not be considered overlapping
                            // https://github.com/mav-industries/mav/issues/26136
                            let is_file_start_auto_import = {
                                let snapshot = buffer.snapshot();
                                let primary_start_point = primary.start.to_point(&snapshot);
                                let range_start_point = range.start.to_point(&snapshot);

                                let result = primary_start_point.row == 0
                                    && primary_start_point.column == 0
                                    && range_start_point.row == 0
                                    && range_start_point.column == 0;

                                result
                            };

                            let has_overlap = if is_file_start_auto_import {
                                false
                            } else {
                                all_commit_ranges.iter().any(|commit_range| {
                                    let start_within =
                                        commit_range.start.cmp(&range.start, buffer).is_le()
                                            && commit_range.end.cmp(&range.start, buffer).is_ge();
                                    let end_within =
                                        range.start.cmp(&commit_range.end, buffer).is_le()
                                            && range.end.cmp(&commit_range.end, buffer).is_ge();
                                    start_within || end_within
                                })
                            };

                            //Skip additional edits which overlap with the primary completion edit
                            //https://github.com/mav-industries/mav/pull/1871
                            if !has_overlap {
                                buffer.edit([(range, text)], None, cx);
                            }
                        }

                        let transaction = if buffer.end_transaction(cx).is_some() {
                            let transaction = buffer.finalize_last_transaction().unwrap().clone();
                            if !push_to_history {
                                buffer.forget_transaction(transaction.id);
                            }
                            Some(transaction)
                        } else {
                            None
                        };
                        Ok(transaction)
                    })
                } else {
                    Ok(None)
                }
            })
        }
    }

    pub fn pull_diagnostics(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Option<Vec<LspPullDiagnostics>>>> {
        let buffer_id = buffer.read(cx).remote_id();

        if let Some((client, upstream_project_id)) = self.upstream_client() {
            let mut suitable_capabilities = None;
            // Are we capable for proto request?
            let any_server_has_diagnostics_provider = self.check_if_capable_for_proto_request(
                &buffer,
                |capabilities| {
                    if let Some(caps) = &capabilities.diagnostic_provider {
                        suitable_capabilities = Some(caps.clone());
                        true
                    } else {
                        false
                    }
                },
                cx,
            );
            // We don't really care which caps are passed into the request, as they're ignored by RPC anyways.
            let Some(dynamic_caps) = suitable_capabilities else {
                return Task::ready(Ok(None));
            };
            assert!(any_server_has_diagnostics_provider);

            let identifier = buffer_diagnostic_identifier(&dynamic_caps);
            let request = GetDocumentDiagnostics {
                previous_result_id: None,
                identifier,
                registration_id: None,
            };
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
            cx.background_spawn(async move {
                // Proto requests cause the diagnostics to be pulled from language server(s) on the local side
                // and then, buffer state updated with the diagnostics received, which will be later propagated to the client.
                // Do not attempt to further process the dummy responses here.
                let _response = request_task.await?;
                Ok(None)
            })
        } else {
            let servers = buffer.update(cx, |buffer, cx| {
                self.running_language_servers_for_local_buffer(buffer, cx)
                    .map(|(_, server)| server.clone())
                    .collect::<Vec<_>>()
            });

            let pull_diagnostics = servers
                .into_iter()
                .flat_map(|server| {
                    let result = maybe!({
                        let local = self.as_local()?;
                        let server_id = server.server_id();
                        let providers_with_identifiers = local
                            .language_server_dynamic_registrations
                            .get(&server_id)
                            .into_iter()
                            .flat_map(|registrations| registrations.diagnostics.clone())
                            .collect::<Vec<_>>();
                        Some(
                            providers_with_identifiers
                                .into_iter()
                                .map(|(registration_id, dynamic_caps)| {
                                    let identifier = buffer_diagnostic_identifier(&dynamic_caps);
                                    let registration_id = registration_id.map(SharedString::from);
                                    let result_id = self.result_id_for_buffer_pull(
                                        server_id,
                                        buffer_id,
                                        &registration_id,
                                        cx,
                                    );
                                    self.request_lsp(
                                        buffer.clone(),
                                        LanguageServerToQuery::Other(server_id),
                                        GetDocumentDiagnostics {
                                            previous_result_id: result_id,
                                            registration_id,
                                            identifier,
                                        },
                                        cx,
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                    });

                    result.unwrap_or_default()
                })
                .collect::<Vec<_>>();

            cx.background_spawn(async move {
                let mut responses = Vec::new();
                for diagnostics in join_all(pull_diagnostics).await {
                    responses.extend(diagnostics?);
                }
                Ok(Some(responses))
            })
        }
    }

    pub fn applicable_inlay_chunks(
        &mut self,
        buffer: &Entity<Buffer>,
        ranges: &[Range<text::Anchor>],
        cx: &mut Context<Self>,
    ) -> Vec<Range<BufferRow>> {
        let buffer_snapshot = buffer.read(cx).snapshot();
        let ranges = ranges
            .iter()
            .map(|range| range.to_point(&buffer_snapshot))
            .collect::<Vec<_>>();

        self.latest_lsp_data(buffer, cx)
            .inlay_hints
            .applicable_chunks(ranges.as_slice())
            .map(|chunk| chunk.row_range())
            .collect()
    }

    pub fn invalidate_inlay_hints<'a>(
        &'a mut self,
        for_buffers: impl IntoIterator<Item = &'a BufferId> + 'a,
    ) {
        for buffer_id in for_buffers {
            if let Some(lsp_data) = self.lsp_data.get_mut(buffer_id) {
                lsp_data.inlay_hints.clear();
            }
        }
    }

    pub fn inlay_hints(
        &mut self,
        invalidate: InvalidationStrategy,
        buffer: Entity<Buffer>,
        ranges: Vec<Range<text::Anchor>>,
        known_chunks: Option<(clock::Global, HashSet<Range<BufferRow>>)>,
        cx: &mut Context<Self>,
    ) -> HashMap<Range<BufferRow>, Task<Result<CacheInlayHints>>> {
        let next_hint_id = self.next_hint_id.clone();
        let lsp_data = self.latest_lsp_data(&buffer, cx);
        let query_version = lsp_data.buffer_version.clone();
        let mut lsp_refresh_requested = false;
        let for_server = if let InvalidationStrategy::RefreshRequested {
            server_id,
            request_id,
        } = invalidate
        {
            let invalidated = lsp_data
                .inlay_hints
                .invalidate_for_server_refresh(server_id, request_id);
            lsp_refresh_requested = invalidated;
            Some(server_id)
        } else {
            None
        };
        let existing_inlay_hints = &mut lsp_data.inlay_hints;
        let known_chunks = known_chunks
            .filter(|(known_version, _)| !lsp_data.buffer_version.changed_since(known_version))
            .map(|(_, known_chunks)| known_chunks)
            .unwrap_or_default();

        let buffer_snapshot = buffer.read(cx).snapshot();
        let ranges = ranges
            .iter()
            .map(|range| range.to_point(&buffer_snapshot))
            .collect::<Vec<_>>();

        let mut hint_fetch_tasks = Vec::new();
        let mut cached_inlay_hints = None;
        let mut ranges_to_query = None;
        let applicable_chunks = existing_inlay_hints
            .applicable_chunks(ranges.as_slice())
            .filter(|chunk| !known_chunks.contains(&chunk.row_range()))
            .collect::<Vec<_>>();
        if applicable_chunks.is_empty() {
            return HashMap::default();
        }

        for row_chunk in applicable_chunks {
            match (
                existing_inlay_hints
                    .cached_hints(&row_chunk)
                    .filter(|_| !lsp_refresh_requested)
                    .cloned(),
                existing_inlay_hints
                    .fetched_hints(&row_chunk)
                    .as_ref()
                    .filter(|_| !lsp_refresh_requested)
                    .cloned(),
            ) {
                (None, None) => {
                    let chunk_range = row_chunk.anchor_range();
                    ranges_to_query
                        .get_or_insert_with(Vec::new)
                        .push((row_chunk, chunk_range));
                }
                (None, Some(fetched_hints)) => hint_fetch_tasks.push((row_chunk, fetched_hints)),
                (Some(cached_hints), None) => {
                    for (server_id, cached_hints) in cached_hints {
                        if for_server.is_none_or(|for_server| for_server == server_id) {
                            cached_inlay_hints
                                .get_or_insert_with(HashMap::default)
                                .entry(row_chunk.row_range())
                                .or_insert_with(HashMap::default)
                                .entry(server_id)
                                .or_insert_with(Vec::new)
                                .extend(cached_hints);
                        }
                    }
                }
                (Some(cached_hints), Some(fetched_hints)) => {
                    hint_fetch_tasks.push((row_chunk, fetched_hints));
                    for (server_id, cached_hints) in cached_hints {
                        if for_server.is_none_or(|for_server| for_server == server_id) {
                            cached_inlay_hints
                                .get_or_insert_with(HashMap::default)
                                .entry(row_chunk.row_range())
                                .or_insert_with(HashMap::default)
                                .entry(server_id)
                                .or_insert_with(Vec::new)
                                .extend(cached_hints);
                        }
                    }
                }
            }
        }

        if hint_fetch_tasks.is_empty()
            && ranges_to_query
                .as_ref()
                .is_none_or(|ranges| ranges.is_empty())
            && let Some(cached_inlay_hints) = cached_inlay_hints
        {
            cached_inlay_hints
                .into_iter()
                .map(|(row_chunk, hints)| (row_chunk, Task::ready(Ok(hints))))
                .collect()
        } else {
            for (chunk, range_to_query) in ranges_to_query.into_iter().flatten() {
                // When a server refresh was requested, other servers' cached hints
                // are unaffected by the refresh and must be included in the result.
                // Otherwise apply_fetched_hints (with should_invalidate()=true)
                // removes all visible hints but only adds back the requesting
                // server's new hints, permanently losing other servers' hints.
                let other_servers_cached: CacheInlayHints = if lsp_refresh_requested {
                    lsp_data
                        .inlay_hints
                        .cached_hints(&chunk)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    HashMap::default()
                };

                let next_hint_id = next_hint_id.clone();
                let buffer = buffer.clone();
                let query_version = query_version.clone();
                let new_inlay_hints = cx
                    .spawn(async move |lsp_store, cx| {
                        let new_fetch_task = lsp_store.update(cx, |lsp_store, cx| {
                            lsp_store.fetch_inlay_hints(for_server, &buffer, range_to_query, cx)
                        })?;
                        new_fetch_task
                            .await
                            .and_then(|new_hints_by_server| {
                                lsp_store.update(cx, |lsp_store, cx| {
                                    let lsp_data = lsp_store.latest_lsp_data(&buffer, cx);
                                    let update_cache = lsp_data.buffer_version == query_version;
                                    if new_hints_by_server.is_empty() {
                                        if update_cache {
                                            lsp_data.inlay_hints.invalidate_for_chunk(chunk);
                                        }
                                        other_servers_cached
                                    } else {
                                        let mut result = other_servers_cached;
                                        for (server_id, new_hints) in new_hints_by_server {
                                            let new_hints = new_hints
                                                .into_iter()
                                                .map(|new_hint| {
                                                    (
                                                        InlayId::Hint(next_hint_id.fetch_add(
                                                            1,
                                                            atomic::Ordering::AcqRel,
                                                        )),
                                                        new_hint,
                                                    )
                                                })
                                                .collect::<Vec<_>>();
                                            if update_cache {
                                                lsp_data.inlay_hints.insert_new_hints(
                                                    chunk,
                                                    server_id,
                                                    new_hints.clone(),
                                                );
                                            }
                                            result.insert(server_id, new_hints);
                                        }
                                        result
                                    }
                                })
                            })
                            .map_err(Arc::new)
                    })
                    .shared();

                let fetch_task = lsp_data.inlay_hints.fetched_hints(&chunk);
                *fetch_task = Some(new_inlay_hints.clone());
                hint_fetch_tasks.push((chunk, new_inlay_hints));
            }

            cached_inlay_hints
                .unwrap_or_default()
                .into_iter()
                .map(|(row_chunk, hints)| (row_chunk, Task::ready(Ok(hints))))
                .chain(hint_fetch_tasks.into_iter().map(|(chunk, hints_fetch)| {
                    (
                        chunk.row_range(),
                        cx.spawn(async move |_, _| {
                            hints_fetch.await.map_err(|e| {
                                if e.error_code() != ErrorCode::Internal {
                                    anyhow!(e.error_code())
                                } else {
                                    anyhow!("{e:#}")
                                }
                            })
                        }),
                    )
                }))
                .collect()
        }
    }

    fn fetch_inlay_hints(
        &mut self,
        for_server: Option<LanguageServerId>,
        buffer: &Entity<Buffer>,
        range: Range<Anchor>,
        cx: &mut Context<Self>,
    ) -> Task<Result<HashMap<LanguageServerId, Vec<InlayHint>>>> {
        let request = InlayHints {
            range: range.clone(),
        };
        if let Some((upstream_client, project_id)) = self.upstream_client() {
            if !self.is_capable_for_proto_request(buffer, &request, cx) {
                return Task::ready(Ok(HashMap::default()));
            }
            let request_timeout = ProjectSettings::get_global(cx)
                .global_lsp_settings
                .get_request_timeout();
            let request_task = upstream_client.request_lsp(
                project_id,
                for_server.map(|id| id.to_proto()),
                request_timeout,
                cx.background_executor().clone(),
                request.to_proto(project_id, buffer.read(cx)),
            );
            let buffer = buffer.clone();
            cx.spawn(async move |weak_lsp_store, cx| {
                let Some(lsp_store) = weak_lsp_store.upgrade() else {
                    return Ok(HashMap::default());
                };
                let Some(responses) = request_task.await? else {
                    return Ok(HashMap::default());
                };

                let inlay_hints = join_all(responses.payload.into_iter().map(|response| {
                    let lsp_store = lsp_store.clone();
                    let buffer = buffer.clone();
                    let cx = cx.clone();
                    let request = request.clone();
                    async move {
                        (
                            LanguageServerId::from_proto(response.server_id),
                            request
                                .response_from_proto(response.response, lsp_store, buffer, cx)
                                .await,
                        )
                    }
                }))
                .await;

                let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
                let mut has_errors = false;
                let inlay_hints = inlay_hints
                    .into_iter()
                    .filter_map(|(server_id, inlay_hints)| match inlay_hints {
                        Ok(inlay_hints) => Some((server_id, inlay_hints)),
                        Err(e) => {
                            has_errors = true;
                            log::error!("{e:#}");
                            None
                        }
                    })
                    .map(|(server_id, mut new_hints)| {
                        new_hints.retain(|hint| {
                            hint.position.is_valid(&buffer_snapshot)
                                && range.start.is_valid(&buffer_snapshot)
                                && range.end.is_valid(&buffer_snapshot)
                                && hint.position.cmp(&range.start, &buffer_snapshot).is_ge()
                                && hint.position.cmp(&range.end, &buffer_snapshot).is_lt()
                        });
                        (server_id, new_hints)
                    })
                    .collect::<HashMap<_, _>>();
                anyhow::ensure!(
                    !has_errors || !inlay_hints.is_empty(),
                    "Failed to fetch inlay hints"
                );
                Ok(inlay_hints)
            })
        } else {
            let inlay_hints_task = match for_server {
                Some(server_id) => {
                    let server_task = self.request_lsp(
                        buffer.clone(),
                        LanguageServerToQuery::Other(server_id),
                        request,
                        cx,
                    );
                    cx.background_spawn(async move {
                        let mut responses = Vec::new();
                        match server_task.await {
                            Ok(response) => responses.push((server_id, response)),
                            // rust-analyzer likes to error with this when its still loading up
                            Err(e) if format!("{e:#}").ends_with("content modified") => (),
                            Err(e) => log::error!(
                                "Error handling response for inlay hints request: {e:#}"
                            ),
                        }
                        responses
                    })
                }
                None => self.request_multiple_lsp_locally(buffer, None::<usize>, request, cx),
            };
            let buffer_snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot());
            cx.background_spawn(async move {
                Ok(inlay_hints_task
                    .await
                    .into_iter()
                    .map(|(server_id, mut new_hints)| {
                        new_hints.retain(|hint| {
                            hint.position.is_valid(&buffer_snapshot)
                                && range.start.is_valid(&buffer_snapshot)
                                && range.end.is_valid(&buffer_snapshot)
                                && hint.position.cmp(&range.start, &buffer_snapshot).is_ge()
                                && hint.position.cmp(&range.end, &buffer_snapshot).is_lt()
                        });
                        (server_id, new_hints)
                    })
                    .collect())
            })
        }
    }

    fn diagnostic_registration_exists(
        &self,
        server_id: LanguageServerId,
        registration_id: &Option<SharedString>,
    ) -> bool {
        let Some(local) = self.as_local() else {
            return false;
        };
        let Some(registrations) = local.language_server_dynamic_registrations.get(&server_id)
        else {
            return false;
        };
        let registration_key = registration_id.as_ref().map(|s| s.to_string());
        registrations.diagnostics.contains_key(&registration_key)
    }

    pub fn pull_diagnostics_for_buffer(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<()>> {
        let diagnostics = self.pull_diagnostics(buffer, cx);
        cx.spawn(async move |lsp_store, cx| {
            let diagnostics = match diagnostics.await {
                Ok(Some(diagnostics)) => diagnostics,
                Ok(None) => return Ok(()),
                Err(error) if should_log_lsp_request_failure(&format!("{error:#}")) => {
                    return Err(error).context("pulling diagnostics");
                }
                // This is a weird way to suppress diagnostic failures on server side cancellation,
                // we should actually retry the request here?
                Err(_) => return Ok(()),
            };
            lsp_store.update(cx, |lsp_store, cx| {
                if lsp_store.as_local().is_none() {
                    return;
                }

                let mut unchanged_buffers = HashMap::default();
                let server_diagnostics_updates = diagnostics
                    .into_iter()
                    .filter_map(|diagnostics_set| match diagnostics_set {
                        LspPullDiagnostics::Response {
                            server_id,
                            uri,
                            diagnostics,
                            registration_id,
                        } => Some((server_id, uri, diagnostics, registration_id)),
                        LspPullDiagnostics::Default => None,
                    })
                    .filter(|(server_id, _, _, registration_id)| {
                        lsp_store.diagnostic_registration_exists(*server_id, registration_id)
                    })
                    .fold(
                        HashMap::default(),
                        |mut acc, (server_id, uri, diagnostics, new_registration_id)| {
                            let (result_id, diagnostics) = match diagnostics {
                                PulledDiagnostics::Unchanged { result_id } => {
                                    unchanged_buffers
                                        .entry(new_registration_id.clone())
                                        .or_insert_with(HashSet::default)
                                        .insert(uri.clone());
                                    (Some(result_id), Vec::new())
                                }
                                PulledDiagnostics::Changed {
                                    result_id,
                                    diagnostics,
                                } => (result_id, diagnostics),
                            };
                            let disk_based_sources = Cow::Owned(
                                lsp_store
                                    .language_server_adapter_for_id(server_id)
                                    .as_ref()
                                    .map(|adapter| adapter.disk_based_diagnostic_sources.as_slice())
                                    .unwrap_or(&[])
                                    .to_vec(),
                            );
                            acc.entry(server_id)
                                .or_insert_with(HashMap::default)
                                .entry(new_registration_id.clone())
                                .or_insert_with(Vec::new)
                                .push(DocumentDiagnosticsUpdate {
                                    server_id,
                                    diagnostics: lsp::PublishDiagnosticsParams {
                                        uri,
                                        diagnostics,
                                        version: None,
                                    },
                                    result_id: result_id.map(SharedString::new),
                                    disk_based_sources,
                                    registration_id: new_registration_id,
                                });
                            acc
                        },
                    );

                for diagnostic_updates in server_diagnostics_updates.into_values() {
                    for (registration_id, diagnostic_updates) in diagnostic_updates {
                        lsp_store
                            .merge_lsp_diagnostics(
                                DiagnosticSourceKind::Pulled,
                                diagnostic_updates,
                                |document_uri, old_diagnostic, _| match old_diagnostic.source_kind {
                                    DiagnosticSourceKind::Pulled => {
                                        old_diagnostic.registration_id != registration_id
                                            || unchanged_buffers
                                                .get(&old_diagnostic.registration_id)
                                                .is_some_and(|unchanged_buffers| {
                                                    unchanged_buffers.contains(&document_uri)
                                                })
                                    }
                                    DiagnosticSourceKind::Other | DiagnosticSourceKind::Pushed => {
                                        true
                                    }
                                },
                                cx,
                            )
                            .log_err();
                    }
                }
            })
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
