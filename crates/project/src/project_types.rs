use super::*;

#[derive(Clone, Copy, Debug)]
pub struct LocalProjectFlags {
    pub init_worktree_trust: bool,
    pub watch_global_configs: bool,
}

impl Default for LocalProjectFlags {
    fn default() -> Self {
        Self {
            init_worktree_trust: true,
            watch_global_configs: true,
        }
    }
}

pub trait ProjectItem: 'static {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<Result<Entity<Self>>>>
    where
        Self: Sized;
    fn entry_id(&self, cx: &App) -> Option<ProjectEntryId>;
    fn project_path(&self, cx: &App) -> Option<ProjectPath>;
    fn is_dirty(&self) -> bool;
}

#[derive(Clone)]
pub enum OpenedBufferEvent {
    Disconnected,
    Ok(BufferId),
    Err(BufferId, Arc<anyhow::Error>),
}

pub(super) struct DownloadingFile {
    pub(super) destination_path: PathBuf,
    pub(super) chunks: Vec<u8>,
    pub(super) total_size: u64,
    pub(super) file_id: Option<u64>, // Set when we receive the State message
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentLocation {
    pub buffer: WeakEntity<Buffer>,
    pub position: Anchor,
}

#[derive(Default)]
pub(super) struct RemotelyCreatedModels {
    pub(super) worktrees: Vec<Entity<Worktree>>,
    pub(super) buffers: Vec<Entity<Buffer>>,
    pub(super) retain_count: usize,
}

pub(super) struct RemotelyCreatedModelGuard {
    pub(super) remote_models: std::sync::Weak<Mutex<RemotelyCreatedModels>>,
}

impl Drop for RemotelyCreatedModelGuard {
    fn drop(&mut self) {
        if let Some(remote_models) = self.remote_models.upgrade() {
            let mut remote_models = remote_models.lock();
            assert!(
                remote_models.retain_count > 0,
                "RemotelyCreatedModelGuard dropped too many times"
            );
            remote_models.retain_count -= 1;
            if remote_models.retain_count == 0 {
                remote_models.buffers.clear();
                remote_models.worktrees.clear();
            }
        }
    }
}
/// Message ordered with respect to buffer operations
#[derive(Debug)]
pub(super) enum BufferOrderedMessage {
    Operation {
        buffer_id: BufferId,
        operation: proto::Operation,
    },
    LanguageServerUpdate {
        language_server_id: LanguageServerId,
        message: proto::update_language_server::Variant,
        name: Option<LanguageServerName>,
    },
    Resync,
}

#[derive(Debug)]
pub(super) enum ProjectClientState {
    /// Single-player mode.
    Local,
    /// Multi-player mode but still a local project.
    Shared { remote_id: u64 },
    /// Multi-player mode but working on a remote project.
    Collab {
        sharing_has_stopped: bool,
        capability: Capability,
        remote_id: u64,
        replica_id: ReplicaId,
    },
}

/// A link to display in a toast notification, useful to point to documentation.
#[derive(PartialEq, Debug, Clone)]
pub struct ToastLink {
    pub label: &'static str,
    pub url: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    LanguageServerAdded(LanguageServerId, LanguageServerName, Option<WorktreeId>),
    LanguageServerRemoved(LanguageServerId),
    LanguageServerLog(LanguageServerId, LanguageServerLogType, String),
    // [`lsp::notification::DidOpenTextDocument`] was sent to this server using the buffer data.
    // Mav's buffer-related data is updated accordingly.
    LanguageServerBufferRegistered {
        server_id: LanguageServerId,
        buffer_id: BufferId,
        buffer_abs_path: PathBuf,
        name: Option<LanguageServerName>,
    },
    ToggleLspLogs {
        server_id: LanguageServerId,
        enabled: bool,
        toggled_log_kind: LogKind,
    },
    Toast {
        notification_id: SharedString,
        message: String,
        /// Optional link to display as a button in the toast.
        link: Option<ToastLink>,
    },
    HideToast {
        notification_id: SharedString,
    },
    LanguageServerPrompt(LanguageServerPromptRequest),
    LanguageNotFound(Entity<Buffer>),
    ActiveEntryChanged(Option<ProjectEntryId>),
    ActivateProjectPanel,
    WorktreeAdded(WorktreeId),
    WorktreeOrderChanged,
    WorktreeRemoved(WorktreeId),
    WorktreeUpdatedEntries(WorktreeId, UpdatedEntriesSet),
    WorktreeUpdatedRootRepoCommonDir(WorktreeId),
    WorktreePathsChanged {
        old_worktree_paths: WorktreePaths,
    },
    DiskBasedDiagnosticsStarted {
        language_server_id: LanguageServerId,
    },
    DiskBasedDiagnosticsFinished {
        language_server_id: LanguageServerId,
    },
    DiagnosticsUpdated {
        paths: Vec<ProjectPath>,
        language_server_id: LanguageServerId,
    },
    RemoteIdChanged(Option<u64>),
    DisconnectedFromHost,
    DisconnectedFromRemote {
        server_not_running: bool,
    },
    Closed,
    DeletedEntry(WorktreeId, ProjectEntryId),
    CollaboratorUpdated {
        old_peer_id: proto::PeerId,
        new_peer_id: proto::PeerId,
    },
    CollaboratorJoined(proto::PeerId),
    CollaboratorLeft(proto::PeerId),
    HostReshared,
    Reshared,
    Rejoined,
    RefreshInlayHints {
        server_id: LanguageServerId,
        request_id: Option<usize>,
    },
    RefreshSemanticTokens {
        server_id: LanguageServerId,
        request_id: Option<usize>,
    },
    RefreshCodeLens,
    RevealInProjectPanel(ProjectEntryId),
    SnippetEdit(BufferId, Vec<(lsp::Range, Snippet)>),
    ExpandedAllForEntry(WorktreeId, ProjectEntryId),
    EntryRenamed(ProjectTransaction, ProjectPath, PathBuf),
    WorkspaceEditApplied(ProjectTransaction),
    AgentLocationChanged,
    BufferEdited {
        source: BufferEditSource,
    },
}

pub struct AgentLocationChanged;

pub enum DebugAdapterClientState {
    Starting(Task<Option<Arc<DebugAdapterClient>>>),
    Running(Arc<DebugAdapterClient>),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ProjectPath {
    pub worktree_id: WorktreeId,
    pub path: Arc<RelPath>,
}

impl ProjectPath {
    pub fn from_file(value: &dyn language::File, cx: &App) -> Self {
        ProjectPath {
            worktree_id: value.worktree_id(cx),
            path: value.path().clone(),
        }
    }

    pub fn from_proto(p: proto::ProjectPath) -> Option<Self> {
        Some(Self {
            worktree_id: WorktreeId::from_proto(p.worktree_id),
            path: RelPath::from_proto(&p.path).log_err()?,
        })
    }

    pub fn to_proto(&self) -> proto::ProjectPath {
        proto::ProjectPath {
            worktree_id: self.worktree_id.to_proto(),
            path: self.path.as_ref().to_proto(),
        }
    }

    pub fn root_path(worktree_id: WorktreeId) -> Self {
        Self {
            worktree_id,
            path: RelPath::empty_arc(),
        }
    }

    pub fn starts_with(&self, other: &ProjectPath) -> bool {
        self.worktree_id == other.worktree_id && self.path.starts_with(&other.path)
    }
}

pub(super) enum EntitySubscription {
    Project(PendingEntitySubscription<Project>),
    BufferStore(PendingEntitySubscription<BufferStore>),
    GitStore(PendingEntitySubscription<GitStore>),
    WorktreeStore(PendingEntitySubscription<WorktreeStore>),
    LspStore(PendingEntitySubscription<LspStore>),
    SettingsObserver(PendingEntitySubscription<SettingsObserver>),
    DapStore(PendingEntitySubscription<DapStore>),
    BreakpointStore(PendingEntitySubscription<BreakpointStore>),
}

pub const CURRENT_PROJECT_FEATURES: &[&str] = &["new-style-anchors"];

#[cfg(feature = "test-support")]
pub const DEFAULT_COMPLETION_CONTEXT: CompletionContext = CompletionContext {
    trigger_kind: lsp::CompletionTriggerKind::INVOKED,
    trigger_character: None,
};

/// An LSP diagnostics associated with a certain language server.
#[derive(Clone, Debug, Default)]
pub enum LspPullDiagnostics {
    #[default]
    Default,
    Response {
        /// The id of the language server that produced diagnostics.
        server_id: LanguageServerId,
        /// URI of the resource,
        uri: lsp::Uri,
        /// The ID provided by the dynamic registration that produced diagnostics.
        registration_id: Option<SharedString>,
        /// The diagnostics produced by this language server.
        diagnostics: PulledDiagnostics,
    },
}

#[derive(Clone, Debug)]
pub enum PulledDiagnostics {
    Unchanged {
        /// An ID the current pulled batch for this file.
        /// If given, can be used to query workspace diagnostics partially.
        result_id: SharedString,
    },
    Changed {
        result_id: Option<SharedString>,
        diagnostics: Vec<lsp::Diagnostic>,
    },
}

/// Whether to disable all AI features in Mav.
///
/// Default: false
#[derive(Copy, Clone, Debug, RegisterSetting)]
pub struct DisableAiSettings {
    pub disable_ai: bool,
}

impl settings::Settings for DisableAiSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        Self {
            disable_ai: content.project.disable_ai.unwrap().0,
        }
    }
}

impl DisableAiSettings {
    /// Returns whether AI is disabled for the given file.
    ///
    /// This checks the project-level settings for the file's worktree,
    /// allowing `disable_ai` to be configured per-project in `.mav/settings.json`.
    pub fn is_ai_disabled_for_buffer(buffer: Option<&Entity<Buffer>>, cx: &App) -> bool {
        Self::is_ai_disabled_for_file(buffer.and_then(|buffer| buffer.read(cx).file()), cx)
    }

    pub fn is_ai_disabled_for_file(file: Option<&Arc<dyn language::File>>, cx: &App) -> bool {
        let location = file.map(|f| settings::SettingsLocation {
            worktree_id: f.worktree_id(cx),
            path: f.path().as_ref(),
        });
        Self::get(location, cx).disable_ai
    }
}
