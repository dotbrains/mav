#[path = "thread/agent_message.rs"]
mod agent_message;
#[path = "thread/agent_tool.rs"]
mod agent_tool;
#[path = "thread/compaction.rs"]
mod compaction;
#[cfg(test)]
#[path = "thread/compaction_flow_tests.rs"]
mod compaction_flow_tests;
#[cfg(test)]
#[path = "thread/compaction_request_tests.rs"]
mod compaction_request_tests;
#[cfg(test)]
#[path = "thread/compaction_summary_tests.rs"]
mod compaction_summary_tests;
#[cfg(test)]
#[path = "thread/compaction_threshold_tests.rs"]
mod compaction_threshold_tests;
#[path = "thread/completion_events.rs"]
mod completion_events;
#[path = "thread/completion_request.rs"]
mod completion_request;
#[path = "thread/construction.rs"]
mod construction;
#[path = "thread/environment.rs"]
mod environment;
#[path = "thread/event_stream.rs"]
mod event_stream;
#[path = "thread/lifecycle.rs"]
mod lifecycle;
#[path = "thread/manual_compaction.rs"]
mod manual_compaction;
#[cfg(test)]
#[path = "thread/manual_compaction_tests.rs"]
mod manual_compaction_tests;
#[path = "thread/markdown.rs"]
mod markdown;
mod message;
#[path = "thread/message_ingress.rs"]
mod message_ingress;
#[path = "thread/message_state.rs"]
mod message_state;
#[path = "thread/model_settings.rs"]
mod model_settings;
#[path = "thread/permission_context.rs"]
mod permission_context;
#[path = "thread/persistence.rs"]
mod persistence;
#[path = "thread/replay.rs"]
mod replay;
#[path = "thread/restore.rs"]
mod restore;
#[path = "thread/running_turn.rs"]
mod running_turn;
#[cfg(test)]
#[path = "thread/sandbox_authorization_tests.rs"]
mod sandbox_authorization_tests;
#[path = "thread/sandbox_status.rs"]
mod sandbox_status;
#[path = "thread/subagent.rs"]
mod subagent;
#[cfg(test)]
#[path = "thread/subagent_settings_tests.rs"]
mod subagent_settings_tests;
#[path = "thread/support_types.rs"]
mod support_types;
#[path = "thread/title_generation.rs"]
mod title_generation;
#[path = "thread/title_request.rs"]
mod title_request;
#[path = "thread/token_usage.rs"]
mod token_usage;
#[path = "thread/tool_call_authorization.rs"]
mod tool_call_authorization;
#[path = "thread/tool_call_decision_prompt.rs"]
mod tool_call_decision_prompt;
#[path = "thread/tool_call_event_core.rs"]
mod tool_call_event_core;
#[cfg(any(test, feature = "test-support"))]
#[path = "thread/tool_call_event_receiver.rs"]
mod tool_call_event_receiver;
#[path = "thread/tool_call_event_updates.rs"]
mod tool_call_event_updates;
#[path = "thread/tool_call_sandbox_authorization.rs"]
mod tool_call_sandbox_authorization;
#[path = "thread/tool_input.rs"]
mod tool_input;
#[cfg(test)]
#[path = "thread/tool_replay_tests.rs"]
mod tool_replay_tests;
#[path = "thread/tools.rs"]
mod tools;
#[path = "thread/turn.rs"]
mod turn;
#[path = "thread/turn_support.rs"]
mod turn_support;

use crate::{
    ApplyCodeActionTool, CodeActionStore, ContextServerRegistry, CopyPathTool, CreateDirectoryTool,
    CreateThreadTool, DbLanguageModel, DbThread, DeletePathTool, DiagnosticsTool, EditFileTool,
    FetchTool, FindPathTool, FindReferencesTool, GetCodeActionsTool, GoToDefinitionTool, GrepTool,
    ListAgentsAndModelsTool, ListDirectoryTool, MovePathTool, ProjectSnapshot, ReadFileTool,
    RenameTool, SandboxedTerminalTool, SpawnAgentTool, SystemPromptTemplate, Template, Templates,
    TerminalTool, ToolPermissionDecision, WebSearchTool, WriteFileTool,
    decide_permission_from_settings,
};
use acp_thread::{ClientUserMessageId, MentionUri};
use action_log::ActionLog;
use agent_settings::UserAgentsMd;

use crate::sandboxing::{
    SandboxRequest, ThreadSandbox, ThreadSandboxGrants, sandboxing_available_for_project,
    sandboxing_enabled_for_project,
};
use crate::tools::{SandboxGitPathCandidates, sandbox_git_paths};
use agent_client_protocol::schema::v1 as acp;
use agent_settings::{
    AgentProfileId, AgentSettings, AutoCompactThreshold, COMPACTION_PROMPT,
    SUMMARIZE_THREAD_DETAILED_PROMPT, SUMMARIZE_THREAD_PROMPT,
};
use anyhow::{Context as _, Result, anyhow};
use chrono::{DateTime, Local, Utc};
use client::UserStore;
use cloud_api_types::Plan;
use collections::{HashMap, HashSet, IndexMap};
use fs::Fs;
use futures::{
    FutureExt,
    channel::{mpsc, oneshot},
    future::Shared,
    stream::FuturesUnordered,
};
use futures::{StreamExt, stream};
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, EventEmitter, SharedString, Task, WeakEntity,
};
use heck::ToSnakeCase as _;
use language_model::{
    CompletionIntent, LanguageModel, LanguageModelCompletionError, LanguageModelCompletionEvent,
    LanguageModelId, LanguageModelImage, LanguageModelProviderId, LanguageModelRegistry,
    LanguageModelRequest, LanguageModelRequestMessage, LanguageModelRequestTool,
    LanguageModelToolResult, LanguageModelToolResultContent, LanguageModelToolSchemaFormat,
    LanguageModelToolUse, LanguageModelToolUseId, MAV_CLOUD_PROVIDER_ID, MessageContent, Role,
    SelectedModel, Speed, StopReason, TokenUsage,
};
use project::Project;
use prompt_store::ProjectContext;
use schemars::{JsonSchema, Schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use settings::{
    LanguageModelSelection, Settings, SettingsStore, ToolPermissionMode, update_settings_file,
};
use std::fmt::Write;
use std::{cell::RefCell, ops::ControlFlow};
use std::{
    collections::BTreeMap,
    marker::PhantomData,
    ops::RangeInclusive,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use util::{ResultExt, debug_panic, markdown::MarkdownCodeBlock, paths::PathStyle};
use uuid::Uuid;

pub use agent_tool::{AgentTool, AgentToolOutput, AnyAgentTool, Erased};
use compaction::{
    CompactionInsertion, CompactionTelemetry, extend_request_history_until, total_input_tokens,
};
pub use environment::{
    AvailableAgent, AvailableAgents, AvailableModel, SiblingThreadInfo, SiblingThreadRequest,
    SubagentHandle, TerminalHandle, ThreadEnvironment,
};
use event_stream::ThreadEventStream;
pub(crate) use markdown::messages_to_markdown;
pub use message::*;
pub(crate) use permission_context::auto_resolve_permission_outcome;
pub use permission_context::{ToolCallAuthorization, ToolPermissionContext, ToolPermissionScope};
use running_turn::RunningTurn;
pub(crate) use support_types::{
    BASE_RETRY_DELAY, COMPACTION_RETAINED_USER_MESSAGES_BYTE_BUDGET, CompletionError,
    MAX_RETRY_ATTEMPTS, MIN_COMPACTION_CONTEXT_WINDOW, RetryStrategy, ThreadModel,
};
pub use support_types::{
    NoModelConfiguredError, PromptId, SandboxStatusKey, SandboxStatusRefresh, SubagentContext,
    VerifiedSandboxStatus,
};
pub use title_request::{
    TitleUpdated, TokenUsageUpdated, build_thread_title_request, stream_thread_title,
};
#[cfg(any(test, feature = "test-support"))]
pub use tool_call_event_receiver::ToolCallEventStreamReceiver;
pub use tool_input::{ToolInput, ToolInputPayload, ToolInputSender};

const TOOL_CANCELED_MESSAGE: &str = "Tool canceled by user";
pub const MAX_TOOL_NAME_LENGTH: usize = 64;
pub const MAX_SUBAGENT_DEPTH: u8 = 1;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub(crate) content: Vec<AgentMessageContent>,
    pub(crate) tool_results: IndexMap<LanguageModelToolUseId, LanguageModelToolResult>,
    pub(crate) reasoning_details: Option<Arc<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentMessageContent {
    Text(String),
    Thinking {
        text: String,
        signature: Option<String>,
    },
    RedactedThinking(String),
    ToolUse(LanguageModelToolUse),
}

#[derive(Debug)]
pub enum ThreadEvent {
    UserMessage(UserMessage),
    AgentText(String),
    AgentThinking(String),
    ToolCall(acp::ToolCall),
    ToolCallUpdate(acp_thread::ToolCallUpdate),
    ToolCallAuthorization(ToolCallAuthorization),
    ToolCallAuthorizationResolved {
        tool_call_id: acp::ToolCallId,
        outcome: acp_thread::SelectedPermissionOutcome,
    },
    SubagentSpawned(acp::SessionId),
    Retry(acp_thread::RetryStatus),
    ContextCompaction(acp_thread::ContextCompaction),
    ContextCompactionUpdate(acp_thread::ContextCompactionUpdate),
    Stop(acp::StopReason),
}

#[derive(Debug)]
pub struct NewTerminal {
    pub command: String,
    pub output_byte_limit: Option<u64>,
    pub cwd: Option<PathBuf>,
    pub response: oneshot::Sender<Result<Entity<acp_thread::Terminal>>>,
}

pub struct Thread {
    id: acp::SessionId,
    prompt_id: PromptId,
    updated_at: DateTime<Utc>,
    title: Option<SharedString>,
    pending_title_generation: Option<Task<()>>,
    title_generation_failed: bool,
    pending_summary_generation: Option<Shared<Task<Option<SharedString>>>>,
    summary: Option<SharedString>,
    messages: Vec<Arc<Message>>,
    user_store: Entity<UserStore>,
    /// Holds the task that handles agent interaction until the end of the turn.
    /// Survives across multiple requests as the model performs tool calls and
    /// we run tools, report their results.
    running_turn: Option<RunningTurn>,
    /// When set, the current turn ends at the next message boundary instead of
    /// running to completion. The UI sets this to deliver a "steering" queued
    /// message mid-task; by default queued messages wait for the turn to finish.
    end_turn_at_next_boundary: bool,
    pending_message: Option<AgentMessage>,
    pub(crate) tools: BTreeMap<SharedString, Arc<dyn AnyAgentTool>>,
    request_token_usage: HashMap<ClientUserMessageId, language_model::TokenUsage>,
    cumulative_token_usage: TokenUsage,
    /// The per-field maximum usage snapshot already added to
    /// `cumulative_token_usage` for the in-flight completion request. Reset at
    /// the start of each request.
    current_request_token_usage: TokenUsage,
    pending_compaction_telemetry: Option<CompactionTelemetry>,
    #[allow(unused)]
    initial_project_snapshot: Shared<Task<Option<Arc<ProjectSnapshot>>>>,
    pub(crate) context_server_registry: Entity<ContextServerRegistry>,
    profile_id: AgentProfileId,
    project_context: Entity<ProjectContext>,
    pub(crate) templates: Arc<Templates>,
    model: ThreadModel,
    summarization_model: Option<Arc<dyn LanguageModel>>,
    thinking_enabled: bool,
    thinking_effort: Option<String>,
    speed: Option<Speed>,
    prompt_capabilities_tx: watch::Sender<acp::PromptCapabilities>,
    pub(crate) prompt_capabilities_rx: watch::Receiver<acp::PromptCapabilities>,
    pub(crate) project: Entity<Project>,
    pub(crate) action_log: Entity<ActionLog>,
    /// If this is a subagent thread, contains context about the parent
    subagent_context: Option<SubagentContext>,
    /// The user's unsent prompt text, persisted so it can be restored when reloading the thread.
    draft_prompt: Option<Vec<acp::ContentBlock>>,
    ui_scroll_position: Option<gpui::ListOffset>,
    /// Weak references to running subagent threads for cancellation propagation
    running_subagents: Vec<WeakEntity<Thread>>,
    inherits_parent_model_settings: bool,
    sandboxed_terminal_temp_dir: Option<PathBuf>,
    /// Sandbox permissions the user approved "for the rest of the thread".
    /// Shared with each tool call's event stream so repeated requests for
    /// already-granted permissions skip the approval prompt.
    /// Never persisted — lives and dies with this thread.
    sandbox_grants: Rc<RefCell<ThreadSandboxGrants>>,
}

impl Thread {
    pub fn id(&self) -> &acp::SessionId {
        &self.id
    }

    // Only used by Seatbelt-style sandboxes (macOS); Linux relies on bwrap's
    // tmpfs `/tmp` and Windows on the WSL bwrap tmpfs, so neither needs a
    // per-thread temp directory.
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    pub(crate) fn sandboxed_terminal_temp_dir(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf> {
        if let Some(temp_dir) = &self.sandboxed_terminal_temp_dir {
            std::fs::create_dir_all(temp_dir).with_context(|| {
                format!(
                    "failed to recreate sandboxed terminal temp directory {}",
                    temp_dir.display()
                )
            })?;
            return Ok(temp_dir.clone());
        }

        let temp_dir = tempfile::Builder::new()
            .prefix("mav-agent-terminal-")
            .tempdir()
            .context("failed to create sandboxed terminal temp directory")?;
        let temp_dir = temp_dir.keep();
        self.sandboxed_terminal_temp_dir = Some(temp_dir.clone());
        cx.notify();
        Ok(temp_dir)
    }

    pub(super) fn advance_prompt_id(&mut self) {
        self.prompt_id = PromptId::new();
    }
}

/// The user's choice when the OS sandbox could not be created for a command
/// (see [`ToolCallEventStream::authorize_sandbox_fallback`]). Only the
/// Bubblewrap sandboxes (Linux directly, Windows via WSL) can fail to create a
/// sandbox, so this is gated to those platforms.
#[cfg(any(target_os = "linux", target_os = "windows"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SandboxFallbackDecision {
    /// Try creating the sandbox again (e.g. after the user installed `bwrap`).
    Retry,
    /// Run the command without a sandbox.
    RunUnsandboxed,
    /// Don't run the command at all.
    Deny,
}

#[derive(Clone)]
pub struct ToolCallEventStream {
    tool_use_id: LanguageModelToolUseId,
    stream: ThreadEventStream,
    fs: Option<Arc<dyn Fs>>,
    cancellation_rx: watch::Receiver<bool>,
    /// Shared, thread-scoped sandbox grants (see [`Thread::sandbox_grants`]).
    sandbox_grants: Rc<RefCell<ThreadSandboxGrants>>,
    /// The owning thread, used to trigger a save when a "for this thread"
    /// sandbox grant is recorded so it survives reopening. `None` in tests and
    /// for streams not tied to a live thread.
    thread: Option<WeakEntity<Thread>>,
}

#[cfg(test)]
#[path = "thread/test_fixtures.rs"]
mod tests;
