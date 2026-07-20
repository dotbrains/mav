use super::{
    LanguageServerLogType, LanguageServerProgress, LanguageServerPromptRequest, ProgressToken,
};
use crate::{ProjectPath, ProjectTransaction, Snippet};
use client::proto;
use clock::Lamport;
use collections::{BTreeMap, BTreeSet, HashSet};
use gpui::{Entity, SharedString};
use language::{Buffer, Language, LanguageName};
use lsp::{LanguageServerBinary, LanguageServerId, LanguageServerName, Uri};
use serde::Serialize;
use std::sync::Arc;
use worktree::WorktreeId;

#[derive(Debug)]
pub enum LspStoreEvent {
    LanguageServerAdded(LanguageServerId, LanguageServerName, Option<WorktreeId>),
    LanguageServerRemoved(LanguageServerId),
    LanguageServerUpdate {
        language_server_id: LanguageServerId,
        name: Option<LanguageServerName>,
        message: proto::update_language_server::Variant,
    },
    LanguageServerLog(LanguageServerId, LanguageServerLogType, String),
    LanguageServerPrompt(LanguageServerPromptRequest),
    LanguageDetected {
        buffer: Entity<Buffer>,
        new_language: Option<Arc<Language>>,
    },
    Notification(String),
    RefreshInlayHints {
        server_id: LanguageServerId,
        request_id: Option<usize>,
    },
    RefreshSemanticTokens {
        server_id: LanguageServerId,
        request_id: Option<usize>,
    },
    RefreshCodeLens,
    DiagnosticsUpdated {
        server_id: LanguageServerId,
        paths: Vec<ProjectPath>,
    },
    DiskBasedDiagnosticsStarted {
        language_server_id: LanguageServerId,
    },
    DiskBasedDiagnosticsFinished {
        language_server_id: LanguageServerId,
    },
    SnippetEdit {
        buffer_id: language::BufferId,
        edits: Vec<(lsp::Range, Snippet)>,
        most_recent_edit: Lamport,
    },
    WorkspaceEditApplied(ProjectTransaction),
}

#[derive(Clone, Debug, Serialize)]
pub struct LanguageServerStatus {
    pub name: LanguageServerName,
    pub language_name: Option<LanguageName>,
    pub server_version: Option<SharedString>,
    pub server_readable_version: Option<SharedString>,
    pub pending_work: BTreeMap<ProgressToken, LanguageServerProgress>,
    pub has_pending_diagnostic_updates: bool,
    pub progress_tokens: HashSet<ProgressToken>,
    pub worktree: Option<WorktreeId>,
    pub binary: Option<LanguageServerBinary>,
    pub configuration: Option<serde_json::Value>,
    pub workspace_folders: BTreeSet<Uri>,
    pub process_id: Option<u32>,
}
