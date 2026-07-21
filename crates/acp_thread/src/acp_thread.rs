mod connection;
mod content_block;
mod diff;
mod entries;
mod mention;
mod metadata;
mod terminal;
mod thread_checkpoint;
mod thread_files;
mod thread_markdown;
mod thread_messages;
mod thread_plan;
mod thread_state;
mod thread_terminals;
mod thread_tool_calls;
mod thread_turn;
mod thread_types;
mod thread_updates;
mod tool_call;
pub use ::terminal::HeadlessTerminal;
use action_log::{ActionLog, ActionLogTelemetry};
use agent_client_protocol::schema::{MaybeUndefined, v1 as acp};
use anyhow::{Context as _, Result, anyhow};
use collections::HashSet;
pub use connection::*;
pub use content_block::*;
pub use diff::*;
pub use entries::*;
use feature_flags::{AcpBetaFeatureFlag, FeatureFlagAppExt as _};
use futures::{FutureExt, channel::oneshot, future::BoxFuture};
use gpui::{
    AppContext, AsyncApp, Context, Entity, EventEmitter, SharedString, Subscription, Task,
    WeakEntity,
};
use itertools::Itertools;
use language::language_settings::FormatOnSave;
use language::{
    Anchor, Buffer, BufferEditSource, BufferSnapshot, LanguageRegistry, Point, ToPoint, text_diff,
};
use markdown::{Markdown, MarkdownOptions};
pub use mention::*;
pub use metadata::*;
use project::lsp_store::{FormatTrigger, LspFormatTarget};
use project::{
    AgentLocation, Project,
    git_store::{GitStoreCheckpoint, GitStoreEvent, RepositoryEvent},
};
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Formatter, Write};
use std::ops::Range;
use std::process::ExitStatus;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::{fmt::Display, mem, path::PathBuf, sync::Arc};
use task::{Shell, ShellBuilder};
pub use terminal::*;
use text::Bias;
use thread_markdown::markdown_for_raw_output;
pub use thread_types::*;
pub use tool_call::*;
use ui::App;
use util::markdown::MarkdownEscaped;
use util::path_list::PathList;
use util::{
    ResultExt, get_default_system_shell_preferring_bash,
    paths::{PathStyle, is_absolute},
};
use uuid::Uuid;

/// Returned when the model stops because it exhausted its output token budget.
#[derive(Debug)]
pub struct MaxOutputTokensError;

impl std::fmt::Display for MaxOutputTokensError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "output token limit reached")
    }
}

impl std::error::Error for MaxOutputTokensError {}

struct RunningTurn {
    id: u32,
    send_task: Task<()>,
}

pub struct AcpThread {
    session_id: acp::SessionId,
    work_dirs: Option<PathList>,
    parent_session_id: Option<acp::SessionId>,
    title: Option<SharedString>,
    provisional_title: Option<SharedString>,
    entries: Vec<AgentThreadEntry>,
    plan: Plan,
    project: Entity<Project>,
    action_log: Entity<ActionLog>,
    _git_store_subscription: Subscription,
    update_last_checkpoint_if_changed_task: Option<Task<Result<()>>>,
    shared_buffers: HashMap<Entity<Buffer>, BufferSnapshot>,
    turn_id: u32,
    running_turn: Option<RunningTurn>,
    connection: Rc<dyn AgentConnection>,
    token_usage: Option<TokenUsage>,
    cost: Option<SessionCost>,
    prompt_capabilities: acp::PromptCapabilities,
    available_commands: Vec<acp::AvailableCommand>,
    _observe_prompt_capabilities: Task<anyhow::Result<()>>,
    terminals: HashMap<acp::TerminalId, Entity<Terminal>>,
    pending_terminal_output: HashMap<acp::TerminalId, Vec<Vec<u8>>>,
    pending_terminal_exit: HashMap<acp::TerminalId, acp::TerminalExitStatus>,
    had_error: bool,
    /// The user's unsent prompt text, persisted so it can be restored when reloading the thread.
    draft_prompt: Option<Vec<acp::ContentBlock>>,
    /// The initial scroll position for the thread view, set during session registration.
    ui_scroll_position: Option<gpui::ListOffset>,
    /// Buffer for smooth text streaming. Holds text that has been received from
    /// the model but not yet revealed in the UI. A timer task drains this buffer
    /// gradually to create a fluid typing effect instead of choppy chunk-at-a-time
    /// updates.
    streaming_text_buffer: Option<StreamingTextBuffer>,
}

struct StreamingTextBuffer {
    /// Text received from the model but not yet appended to the Markdown source.
    pending: String,
    /// The number of bytes to reveal per timer turn.
    bytes_to_reveal_per_tick: usize,
    /// The Markdown entity being streamed into.
    target: Entity<Markdown>,
    /// Timer task that periodically moves text from `pending` into `source`.
    _reveal_task: Task<()>,
}

impl StreamingTextBuffer {
    /// The number of milliseconds between each timer tick, controlling how quickly
    /// text is revealed.
    const TASK_UPDATE_MS: u64 = 16;
    /// The time in milliseconds to reveal the entire pending text.
    const REVEAL_TARGET: f32 = 200.0;
}

impl From<&AcpThread> for ActionLogTelemetry {
    fn from(value: &AcpThread) -> Self {
        Self {
            agent_telemetry_id: value.connection().telemetry_id(),
            session_id: value.session_id.0.clone(),
        }
    }
}

#[derive(Debug)]
pub enum AcpThreadEvent {
    StatusChanged,
    PromptUpdated,
    NewEntry,
    TitleUpdated,
    TokenUsageUpdated,
    EntryUpdated(usize),
    EntriesRemoved(Range<usize>),
    ToolAuthorizationRequested(acp::ToolCallId),
    ToolAuthorizationReceived(acp::ToolCallId),
    Retry(RetryStatus),
    SubagentSpawned(acp::SessionId),
    Stopped(acp::StopReason),
    Error,
    LoadError(LoadError),
    PromptCapabilitiesUpdated,
    Refusal,
    AvailableCommandsUpdated(Vec<acp::AvailableCommand>),
    ModeUpdated(acp::SessionModeId),
    ConfigOptionsUpdated(Vec<acp::SessionConfigOption>),
    WorkingDirectoriesUpdated,
}

impl EventEmitter<AcpThreadEvent> for AcpThread {}

#[derive(Debug, Clone)]
pub enum TerminalProviderEvent {
    Created {
        terminal_id: acp::TerminalId,
        label: String,
        cwd: Option<PathBuf>,
        output_byte_limit: Option<u64>,
        terminal: Entity<::terminal::Terminal>,
    },
    Output {
        terminal_id: acp::TerminalId,
        data: Vec<u8>,
    },
    TitleChanged {
        terminal_id: acp::TerminalId,
        title: String,
    },
    Exit {
        terminal_id: acp::TerminalId,
        status: acp::TerminalExitStatus,
    },
}

#[derive(Debug, Clone)]
pub enum TerminalProviderCommand {
    WriteInput {
        terminal_id: acp::TerminalId,
        bytes: Vec<u8>,
    },
    Resize {
        terminal_id: acp::TerminalId,
        cols: u16,
        rows: u16,
    },
    Close {
        terminal_id: acp::TerminalId,
    },
}

#[derive(PartialEq, Eq, Debug)]
pub enum ThreadStatus {
    Idle,
    Generating,
}

#[derive(Debug, Clone)]
pub enum LoadError {
    Unsupported {
        command: SharedString,
        current_version: SharedString,
        minimum_version: SharedString,
    },
    FailedToInstall(SharedString),
    Exited {
        status: ExitStatus,
        stderr: Option<SharedString>,
    },
    Other(SharedString),
}

impl Display for LoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Unsupported {
                command: path,
                current_version,
                minimum_version,
            } => {
                write!(
                    f,
                    "version {current_version} from {path} is not supported (need at least {minimum_version})"
                )
            }
            LoadError::FailedToInstall(msg) => write!(f, "Failed to install: {msg}"),
            LoadError::Exited { status, .. } => write!(f, "Server exited with status {status}"),
            LoadError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl Error for LoadError {}

#[cfg(test)]
#[path = "acp_thread/tests.rs"]
mod tests;
