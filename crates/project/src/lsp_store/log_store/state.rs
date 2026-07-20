use std::collections::VecDeque;

use gpui::WeakEntity;
use lsp::{LanguageServerName, MessageType, TraceValue};
use settings::WorktreeId;

use crate::{LanguageServerLogType, LspStore, Project};

use super::MAX_STORED_LOG_ENTRIES;

pub trait Message: AsRef<str> {
    type Level: Copy + std::fmt::Debug;
    fn should_include(&self, _: Self::Level) -> bool {
        true
    }
}

#[derive(Debug)]
pub struct LogMessage {
    pub(super) message: String,
    pub(super) typ: MessageType,
}

impl LogMessage {
    pub(super) fn new(message: String, typ: MessageType) -> Self {
        Self { message, typ }
    }
}

impl AsRef<str> for LogMessage {
    fn as_ref(&self) -> &str {
        &self.message
    }
}

impl Message for LogMessage {
    type Level = MessageType;

    fn should_include(&self, level: Self::Level) -> bool {
        match (self.typ, level) {
            (MessageType::ERROR, _) => true,
            (_, MessageType::ERROR) => false,
            (MessageType::WARNING, _) => true,
            (_, MessageType::WARNING) => false,
            (MessageType::INFO, _) => true,
            (_, MessageType::INFO) => false,
            _ => true,
        }
    }
}

#[derive(Debug)]
pub struct TraceMessage {
    pub(super) message: String,
    pub(super) is_verbose: bool,
}

impl TraceMessage {
    pub(super) fn new(message: String, is_verbose: bool) -> Self {
        Self {
            message,
            is_verbose,
        }
    }
}

impl AsRef<str> for TraceMessage {
    fn as_ref(&self) -> &str {
        &self.message
    }
}

impl Message for TraceMessage {
    type Level = TraceValue;

    fn should_include(&self, level: Self::Level) -> bool {
        match level {
            TraceValue::Off => false,
            TraceValue::Messages => !self.is_verbose,
            TraceValue::Verbose => true,
        }
    }
}

#[derive(Debug)]
pub struct RpcMessage {
    pub(super) message: String,
}

impl RpcMessage {
    pub(super) fn new(message: String) -> Self {
        Self { message }
    }
}

impl AsRef<str> for RpcMessage {
    fn as_ref(&self) -> &str {
        &self.message
    }
}

impl Message for RpcMessage {
    type Level = ();
}

pub struct LanguageServerState {
    pub name: Option<LanguageServerName>,
    pub worktree_id: Option<WorktreeId>,
    pub kind: LanguageServerKind,
    pub(super) log_messages: VecDeque<LogMessage>,
    pub(super) trace_messages: VecDeque<TraceMessage>,
    pub rpc_state: Option<LanguageServerRpcState>,
    pub trace_level: TraceValue,
    pub log_level: MessageType,
    pub(super) io_logs_subscription: Option<lsp::Subscription>,
    pub toggled_log_kind: Option<LogKind>,
}

impl LanguageServerState {
    pub(super) fn new(kind: LanguageServerKind) -> Self {
        Self {
            name: None,
            worktree_id: None,
            kind,
            rpc_state: None,
            log_messages: VecDeque::with_capacity(MAX_STORED_LOG_ENTRIES),
            trace_messages: VecDeque::with_capacity(MAX_STORED_LOG_ENTRIES),
            trace_level: TraceValue::Off,
            log_level: MessageType::LOG,
            io_logs_subscription: None,
            toggled_log_kind: None,
        }
    }
}

impl std::fmt::Debug for LanguageServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageServerState")
            .field("name", &self.name)
            .field("worktree_id", &self.worktree_id)
            .field("kind", &self.kind)
            .field("log_messages", &self.log_messages)
            .field("trace_messages", &self.trace_messages)
            .field("rpc_state", &self.rpc_state)
            .field("trace_level", &self.trace_level)
            .field("log_level", &self.log_level)
            .field("toggled_log_kind", &self.toggled_log_kind)
            .finish_non_exhaustive()
    }
}

#[derive(PartialEq, Clone)]
pub enum LanguageServerKind {
    Local { project: WeakEntity<Project> },
    Remote { project: WeakEntity<Project> },
    LocalSsh { lsp_store: WeakEntity<LspStore> },
    Global,
}

impl std::fmt::Debug for LanguageServerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LanguageServerKind::Local { .. } => write!(f, "LanguageServerKind::Local"),
            LanguageServerKind::Remote { .. } => write!(f, "LanguageServerKind::Remote"),
            LanguageServerKind::LocalSsh { .. } => write!(f, "LanguageServerKind::LocalSsh"),
            LanguageServerKind::Global => write!(f, "LanguageServerKind::Global"),
        }
    }
}

impl LanguageServerKind {
    pub fn project(&self) -> Option<&WeakEntity<Project>> {
        match self {
            Self::Local { project } => Some(project),
            Self::Remote { project } => Some(project),
            Self::LocalSsh { .. } => None,
            Self::Global { .. } => None,
        }
    }
}

#[derive(Debug)]
pub struct LanguageServerRpcState {
    pub rpc_messages: VecDeque<RpcMessage>,
    pub(super) last_message_kind: Option<MessageKind>,
}

impl LanguageServerRpcState {
    pub(super) fn new() -> Self {
        Self {
            rpc_messages: VecDeque::with_capacity(MAX_STORED_LOG_ENTRIES),
            last_message_kind: None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MessageKind {
    Send,
    Receive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LogKind {
    Rpc,
    Trace,
    #[default]
    Logs,
    ServerInfo,
}

impl LogKind {
    pub fn from_server_log_type(log_type: &LanguageServerLogType) -> Self {
        match log_type {
            LanguageServerLogType::Log(_) => Self::Logs,
            LanguageServerLogType::Trace { .. } => Self::Trace,
            LanguageServerLogType::Rpc { .. } => Self::Rpc,
        }
    }
}
