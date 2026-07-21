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
#[cfg(test)]
#[path = "thread/manual_compaction_tests.rs"]
mod manual_compaction_tests;
#[path = "thread/markdown.rs"]
mod markdown;
mod message;
#[path = "thread/running_turn.rs"]
mod running_turn;
#[cfg(test)]
#[path = "thread/sandbox_authorization_tests.rs"]
mod sandbox_authorization_tests;
#[cfg(test)]
#[path = "thread/subagent_settings_tests.rs"]
mod subagent_settings_tests;
#[path = "thread/tool_input.rs"]
mod tool_input;
#[cfg(test)]
#[path = "thread/tool_replay_tests.rs"]
mod tool_replay_tests;

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
pub(crate) use markdown::messages_to_markdown;
pub use message::*;
use running_turn::RunningTurn;
pub use tool_input::{ToolInput, ToolInputPayload, ToolInputSender};

const TOOL_CANCELED_MESSAGE: &str = "Tool canceled by user";
pub const MAX_TOOL_NAME_LENGTH: usize = 64;
pub const MAX_SUBAGENT_DEPTH: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxStatusKey {
    pub settings_sandbox: ThreadSandbox,
    pub thread_sandbox: ThreadSandbox,
    pub baseline_writable_paths: Vec<PathBuf>,
    pub git_paths: Vec<PathBuf>,
    pub repository_paths: Vec<(PathBuf, PathBuf, PathBuf, PathBuf)>,
    pub settings_allow_git_access: bool,
    pub thread_allow_git_access: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedSandboxStatus {
    pub settings_sandbox: ThreadSandbox,
    pub thread_sandbox: ThreadSandbox,
    pub baseline_writable_paths: Vec<PathBuf>,
}

pub enum SandboxStatusRefresh {
    Ready(VerifiedSandboxStatus),
    Pending(Task<VerifiedSandboxStatus>),
}

/// Auto-compaction is only available for models whose context window is at least
/// this large. For smaller models there isn't enough headroom for a compaction
/// pass to be worthwhile, so we leave the thread uncompacted and let the UI warn
/// the user instead.
pub const MIN_COMPACTION_CONTEXT_WINDOW: u64 = 80_000;

// Using the heuristic that 1 token is about 4 bytes, keep the last 80K bytes of user-message content (~20k tokens).
const COMPACTION_RETAINED_USER_MESSAGES_BYTE_BUDGET: usize = 80_000;

/// Returned when a turn is attempted but no language model has been selected.
#[derive(Debug)]
pub struct NoModelConfiguredError;

impl std::fmt::Display for NoModelConfiguredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no language model configured")
    }
}

impl std::error::Error for NoModelConfiguredError {}

/// Context passed to a subagent thread for lifecycle management
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubagentContext {
    /// ID of the parent thread
    pub parent_thread_id: acp::SessionId,

    /// Current depth level (0 = root agent, 1 = first-level subagent, etc.)
    pub depth: u8,
}

/// The ID of the user prompt that initiated a request.
///
/// This equates to the user physically submitting a message to the model (e.g., by pressing the Enter key).
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
pub struct PromptId(Arc<str>);

impl PromptId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string().into())
    }
}

impl std::fmt::Display for PromptId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub(crate) const MAX_RETRY_ATTEMPTS: u8 = 4;
pub(crate) const BASE_RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
enum RetryStrategy {
    ExponentialBackoff {
        initial_delay: Duration,
        max_attempts: u8,
    },
    Fixed {
        delay: Duration,
        max_attempts: u8,
    },
}

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

pub trait TerminalHandle {
    fn id(&self, cx: &AsyncApp) -> Result<acp::TerminalId>;
    fn current_output(&self, cx: &AsyncApp) -> Result<acp::TerminalOutputResponse>;
    fn wait_for_exit(&self, cx: &AsyncApp) -> Result<Shared<Task<acp::TerminalExitStatus>>>;
    fn kill(&self, cx: &AsyncApp) -> Result<()>;
    fn was_stopped_by_user(&self, cx: &AsyncApp) -> Result<bool>;
}

pub trait SubagentHandle {
    /// The session ID of this subagent thread
    fn id(&self) -> acp::SessionId;
    /// The current number of entries in the thread.
    /// Useful for knowing where the next turn will begin
    fn num_entries(&self, cx: &App) -> usize;
    /// Runs a turn for a given message and returns both the response and the index of that output message.
    fn send(&self, message: String, cx: &AsyncApp) -> Task<Result<String>>;
}

pub trait ThreadEnvironment {
    fn create_terminal(
        &self,
        command: String,
        extra_env: Vec<acp::EnvVariable>,
        cwd: Option<PathBuf>,
        output_byte_limit: Option<u64>,
        sandbox_wrap: Option<acp_thread::SandboxWrap>,
        cx: &mut AsyncApp,
    ) -> Task<Result<Rc<dyn TerminalHandle>>>;

    fn create_subagent(&self, label: String, cx: &mut App) -> Result<Rc<dyn SubagentHandle>>;

    fn resume_subagent(
        &self,
        _session_id: acp::SessionId,
        _cx: &mut App,
    ) -> Result<Rc<dyn SubagentHandle>> {
        Err(anyhow::anyhow!(
            "Resuming subagent sessions is not supported"
        ))
    }

    /// Creates an independent sibling thread visible in the agent sidebar.
    /// Unlike subagents, sibling threads are first-class threads that persist
    /// and run in parallel without reporting results back to the parent.
    fn create_sibling_thread(
        &self,
        request: SiblingThreadRequest,
        cx: &mut AsyncApp,
    ) -> Task<Result<SiblingThreadInfo>> {
        let _ = request;
        let _ = cx;
        Task::ready(Err(anyhow::anyhow!(
            "Creating sibling threads is not supported in this environment"
        )))
    }

    /// Lists the agents and models available for use with `create_sibling_thread`.
    fn list_available_agents(&self, cx: &mut App) -> Result<AvailableAgents> {
        let _ = cx;
        Err(anyhow::anyhow!(
            "Listing available agents is not supported in this environment"
        ))
    }
}

/// A request to create a new sibling thread.
#[derive(Debug, Clone)]
pub struct SiblingThreadRequest {
    /// A short title for the new thread, shown in the sidebar.
    pub title: SharedString,
    /// The initial prompt to send to the new thread.
    pub prompt: String,
    /// Optional agent ID to use. Defaults to the native Mav agent.
    pub agent_id: Option<String>,
    /// Optional model override, as `provider/model-id`.
    /// Defaults to the user's configured default model for the agent.
    pub model: Option<String>,
    /// Whether to create the thread in a new git worktree workspace.
    pub use_new_worktree: bool,
    /// Optional worktree directory name. When `None`, the UI generates a
    /// random non-colliding name (matching the manual "Create worktree"
    /// flow). Only relevant when `use_new_worktree` is true.
    pub worktree_name: Option<String>,
    /// Git ref (branch, tag, or commit) to base the new worktree on.
    /// Only relevant when `use_new_worktree` is true.
    pub base_ref: Option<String>,
}

/// Information returned when a sibling thread is successfully created.
#[derive(Debug, Clone)]
pub struct SiblingThreadInfo {
    /// The title assigned to the thread.
    pub title: SharedString,
    /// The agent ID used for the thread.
    pub agent_id: String,
    /// The model ID used for the thread, if known.
    pub model: Option<String>,
    /// An optional, non-fatal heads-up about the created thread that the
    /// caller should relay or take into account (e.g., the project had an
    /// unusual worktree layout that affected how the new worktree was set
    /// up). Empty when nothing noteworthy happened.
    pub warning: Option<String>,
}

/// A list of agents and, for each, the models available for use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableAgents {
    pub agents: Vec<AvailableAgent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableAgent {
    /// Identifier used when creating a thread.
    pub id: String,
    /// Human-readable name shown in the UI.
    pub name: SharedString,
    /// Whether this is Mav's built-in native agent.
    pub is_native: bool,
    /// Models available for this agent. May be empty if models are not
    /// enumerated up front (e.g., external agents that choose their own).
    pub models: Vec<AvailableModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableModel {
    /// Identifier to pass as the `model` field when creating a thread.
    pub id: String,
    /// Human-readable name.
    pub name: SharedString,
    /// Whether this is the default model for the agent.
    pub is_default: bool,
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

#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    pub tool_name: String,
    pub input_values: Vec<String>,
    pub scope: ToolPermissionScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermissionScope {
    ToolInput,
    SymlinkTarget,
    AgentSkills,
}

impl ToolPermissionContext {
    pub fn new(tool_name: impl Into<String>, input_values: Vec<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            input_values,
            scope: ToolPermissionScope::ToolInput,
        }
    }

    pub fn symlink_target(tool_name: impl Into<String>, target_paths: Vec<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            input_values: target_paths,
            scope: ToolPermissionScope::SymlinkTarget,
        }
    }

    pub fn for_agent_skills(mut self) -> Self {
        self.scope = ToolPermissionScope::AgentSkills;
        self
    }

    /// Builds the permission options for this tool context.
    ///
    /// This is the canonical source for permission option generation.
    /// Tests should use this function rather than manually constructing options.
    ///
    /// # Shell Compatibility for Terminal Tool
    ///
    /// For the terminal tool, "Always allow" options are only shown when the user's
    /// shell supports POSIX-like command chaining syntax (`&&`, `||`, `;`, `|`).
    ///
    /// **Why this matters:** When a user sets up an "always allow" pattern like `^cargo`,
    /// we need to parse the command to extract all sub-commands and verify that EVERY
    /// sub-command matches the pattern. Otherwise, an attacker could craft a command like
    /// `cargo build && rm -rf /` that would bypass the security check.
    ///
    /// **Supported shells:** Posix (sh, bash, dash, zsh), Fish 3.0+, PowerShell 7+/Pwsh,
    /// Cmd, Xonsh, Csh, Tcsh
    ///
    /// **Unsupported shells:** Nushell (uses `and`/`or` keywords), Elvish (uses `and`/`or`
    /// keywords), Rc (Plan 9 shell - no `&&`/`||` operators)
    ///
    /// For unsupported shells, we hide the "Always allow" UI options entirely, and if
    /// the user has `always_allow` rules configured in settings, `ToolPermissionDecision::from_input`
    /// will return a `Deny` with an explanatory error message.
    pub fn build_permission_options(&self) -> acp_thread::PermissionOptions {
        use crate::pattern_extraction::*;
        use util::shell::ShellKind;

        let tool_name = &self.tool_name;
        let input_values = &self.input_values;
        if self.scope == ToolPermissionScope::SymlinkTarget {
            return acp_thread::PermissionOptions::Flat(vec![
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Yes",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("deny"),
                    "No",
                    acp::PermissionOptionKind::RejectOnce,
                ),
            ]);
        }

        // Skills always prompt, so offer only once-only allow/deny.
        if self.scope == ToolPermissionScope::AgentSkills {
            return acp_thread::PermissionOptions::Flat(vec![
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                acp::PermissionOption::new(
                    acp::PermissionOptionId::new("deny"),
                    "Deny",
                    acp::PermissionOptionKind::RejectOnce,
                ),
            ]);
        }

        // Check if the user's shell supports POSIX-like command chaining.
        // See the doc comment above for the full explanation of why this is needed.
        let shell_supports_always_allow = if tool_name == TerminalTool::NAME {
            ShellKind::system().supports_posix_chaining()
        } else {
            true
        };

        // For terminal commands with multiple pipeline commands, use DropdownWithPatterns
        // to let users individually select which command patterns to always allow.
        if tool_name == TerminalTool::NAME && shell_supports_always_allow {
            if let Some(input) = input_values.first() {
                let all_patterns = extract_all_terminal_patterns(input);
                if all_patterns.len() > 1 {
                    let mut choices = Vec::new();
                    choices.push(acp_thread::PermissionOptionChoice {
                        allow: acp::PermissionOption::new(
                            acp::PermissionOptionId::new(format!("always_allow:{}", tool_name)),
                            format!("Always for {}", tool_name.replace('_', " ")),
                            acp::PermissionOptionKind::AllowAlways,
                        ),
                        deny: acp::PermissionOption::new(
                            acp::PermissionOptionId::new(format!("always_deny:{}", tool_name)),
                            format!("Always for {}", tool_name.replace('_', " ")),
                            acp::PermissionOptionKind::RejectAlways,
                        ),
                        sub_patterns: vec![],
                    });
                    choices.push(acp_thread::PermissionOptionChoice {
                        allow: acp::PermissionOption::new(
                            acp::PermissionOptionId::new("allow"),
                            "Only this time",
                            acp::PermissionOptionKind::AllowOnce,
                        ),
                        deny: acp::PermissionOption::new(
                            acp::PermissionOptionId::new("deny"),
                            "Only this time",
                            acp::PermissionOptionKind::RejectOnce,
                        ),
                        sub_patterns: vec![],
                    });
                    return acp_thread::PermissionOptions::DropdownWithPatterns {
                        choices,
                        patterns: all_patterns,
                        tool_name: tool_name.clone(),
                    };
                }
            }
        }

        let extract_for_value = |value: &str| -> (Option<String>, Option<String>) {
            if tool_name == TerminalTool::NAME {
                (
                    extract_terminal_pattern(value),
                    extract_terminal_pattern_display(value),
                )
            } else if tool_name == CopyPathTool::NAME
                || tool_name == MovePathTool::NAME
                || tool_name == EditFileTool::NAME
                || tool_name == WriteFileTool::NAME
                || tool_name == DeletePathTool::NAME
                || tool_name == CreateDirectoryTool::NAME
            {
                (
                    extract_path_pattern(value),
                    extract_path_pattern_display(value),
                )
            } else if tool_name == FetchTool::NAME {
                (
                    extract_url_pattern(value),
                    extract_url_pattern_display(value),
                )
            } else {
                (None, None)
            }
        };

        // Extract patterns from all input values. Only offer a pattern-specific
        // "always allow/deny" button when every value produces the same pattern.
        let (pattern, pattern_display) = match input_values.as_slice() {
            [single] => extract_for_value(single),
            _ => {
                let mut iter = input_values.iter().map(|v| extract_for_value(v));
                match iter.next() {
                    Some(first) => {
                        if iter.all(|pair| pair.0 == first.0) {
                            first
                        } else {
                            (None, None)
                        }
                    }
                    None => (None, None),
                }
            }
        };

        let mut choices = Vec::new();

        let mut push_choice =
            |label: String, allow_id, deny_id, allow_kind, deny_kind, sub_patterns: Vec<String>| {
                choices.push(acp_thread::PermissionOptionChoice {
                    allow: acp::PermissionOption::new(
                        acp::PermissionOptionId::new(allow_id),
                        label.clone(),
                        allow_kind,
                    ),
                    deny: acp::PermissionOption::new(
                        acp::PermissionOptionId::new(deny_id),
                        label,
                        deny_kind,
                    ),
                    sub_patterns,
                });
            };

        if shell_supports_always_allow {
            push_choice(
                format!("Always for {}", tool_name.replace('_', " ")),
                format!("always_allow:{}", tool_name),
                format!("always_deny:{}", tool_name),
                acp::PermissionOptionKind::AllowAlways,
                acp::PermissionOptionKind::RejectAlways,
                vec![],
            );

            if let (Some(pattern), Some(display)) = (pattern, pattern_display) {
                let button_text = if tool_name == TerminalTool::NAME {
                    format!("Always for `{}` commands", display)
                } else {
                    format!("Always for `{}`", display)
                };
                push_choice(
                    button_text,
                    format!("always_allow:{}", tool_name),
                    format!("always_deny:{}", tool_name),
                    acp::PermissionOptionKind::AllowAlways,
                    acp::PermissionOptionKind::RejectAlways,
                    vec![pattern],
                );
            }
        }

        push_choice(
            "Only this time".to_string(),
            "allow".to_string(),
            "deny".to_string(),
            acp::PermissionOptionKind::AllowOnce,
            acp::PermissionOptionKind::RejectOnce,
            vec![],
        );

        acp_thread::PermissionOptions::Dropdown(choices)
    }
}

#[derive(Debug)]
pub struct ToolCallAuthorization {
    pub tool_call: acp::ToolCallUpdate,
    pub options: acp_thread::PermissionOptions,
    pub response: oneshot::Sender<acp_thread::SelectedPermissionOutcome>,
    pub context: Option<ToolPermissionContext>,
    pub kind: acp_thread::AuthorizationKind,
}

fn auto_resolve_permission_outcome(
    options: &acp_thread::PermissionOptions,
    is_allow: bool,
) -> Result<acp_thread::SelectedPermissionOutcome> {
    let kind = if is_allow {
        acp::PermissionOptionKind::AllowOnce
    } else {
        acp::PermissionOptionKind::RejectOnce
    };
    let option = options
        .first_option_of_kind(kind)
        .ok_or_else(|| anyhow!("permission prompt has no auto-resolution option"))?;

    Ok(acp_thread::SelectedPermissionOutcome::new(
        option.option_id.clone(),
        option.kind,
    ))
}

#[derive(Debug, thiserror::Error)]
enum CompletionError {
    #[error("max tokens")]
    MaxTokens,
    #[error("refusal")]
    Refusal,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub(crate) enum ThreadModel {
    Ready(Arc<dyn LanguageModel>),
    Unresolved(SelectedModel),
    Unset,
}

impl ThreadModel {
    fn as_model(&self) -> Option<&Arc<dyn LanguageModel>> {
        match self {
            Self::Ready(model) => Some(model),
            Self::Unresolved(_) | Self::Unset => None,
        }
    }
}

impl From<&ThreadModel> for Option<DbLanguageModel> {
    fn from(model: &ThreadModel) -> Self {
        match model {
            ThreadModel::Ready(model) => Some(DbLanguageModel {
                provider: model.provider_id().to_string(),
                model: model.id().0.to_string(),
            }),
            ThreadModel::Unresolved(selection) => Some(DbLanguageModel {
                provider: selection.provider.0.to_string(),
                model: selection.model.0.to_string(),
            }),
            ThreadModel::Unset => None,
        }
    }
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
    fn prompt_capabilities(model: Option<&dyn LanguageModel>) -> acp::PromptCapabilities {
        let image = model.map_or(true, |model| model.supports_images());
        acp::PromptCapabilities::new()
            .image(image)
            .embedded_context(true)
    }

    pub fn new_subagent(parent_thread: &Entity<Thread>, cx: &mut Context<Self>) -> Self {
        let project = parent_thread.read(cx).project.clone();
        let project_context = parent_thread.read(cx).project_context.clone();
        let context_server_registry = parent_thread.read(cx).context_server_registry.clone();
        let templates = parent_thread.read(cx).templates.clone();
        let model = parent_thread.read(cx).model().cloned();
        let parent_action_log = parent_thread.read(cx).action_log().clone();
        let action_log =
            cx.new(|_cx| ActionLog::new(project.clone()).with_linked_action_log(parent_action_log));
        let mut thread = Self::new_internal(
            project,
            project_context,
            context_server_registry,
            templates,
            model,
            action_log,
            cx,
        );
        thread.subagent_context = Some(SubagentContext {
            parent_thread_id: parent_thread.read(cx).id().clone(),
            depth: parent_thread.read(cx).depth() + 1,
        });
        thread.inherit_parent_settings(parent_thread, cx);
        if let Some(subagent_model) = AgentSettings::get_global(cx).subagent_model.clone() {
            thread.inherits_parent_model_settings = false;
            thread.apply_model_selection(&subagent_model, cx);
        }
        thread
    }

    pub fn new(
        project: Entity<Project>,
        project_context: Entity<ProjectContext>,
        context_server_registry: Entity<ContextServerRegistry>,
        templates: Arc<Templates>,
        model: Option<Arc<dyn LanguageModel>>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(
            project.clone(),
            project_context,
            context_server_registry,
            templates,
            model,
            cx.new(|_cx| ActionLog::new(project)),
            cx,
        )
    }

    fn new_internal(
        project: Entity<Project>,
        project_context: Entity<ProjectContext>,
        context_server_registry: Entity<ContextServerRegistry>,
        templates: Arc<Templates>,
        model: Option<Arc<dyn LanguageModel>>,
        action_log: Entity<ActionLog>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = AgentSettings::get_global(cx);
        let profile_id = settings.default_profile.clone();
        let enable_thinking = settings
            .default_model
            .as_ref()
            .is_some_and(|model| model.enable_thinking);
        let thinking_effort = settings
            .default_model
            .as_ref()
            .and_then(|model| model.effort.clone());
        let speed = settings
            .default_model
            .as_ref()
            .and_then(|model| model.speed);
        let (prompt_capabilities_tx, prompt_capabilities_rx) =
            watch::channel(Self::prompt_capabilities(model.as_deref()));
        let model = model.map_or(ThreadModel::Unset, ThreadModel::Ready);
        Self {
            id: acp::SessionId::new(uuid::Uuid::new_v4().to_string()),
            prompt_id: PromptId::new(),
            updated_at: Utc::now(),
            title: None,
            pending_title_generation: None,
            title_generation_failed: false,
            pending_summary_generation: None,
            summary: None,
            messages: Vec::new(),
            user_store: project.read(cx).user_store(),
            running_turn: None,
            end_turn_at_next_boundary: false,
            pending_message: None,
            tools: BTreeMap::default(),
            request_token_usage: HashMap::default(),
            cumulative_token_usage: TokenUsage::default(),
            current_request_token_usage: TokenUsage::default(),
            pending_compaction_telemetry: None,
            initial_project_snapshot: {
                let project_snapshot = Self::project_snapshot(project.clone(), cx);
                cx.foreground_executor()
                    .spawn(async move { Some(project_snapshot.await) })
                    .shared()
            },
            context_server_registry,
            profile_id,
            project_context,
            templates,
            model,
            summarization_model: None,
            thinking_enabled: enable_thinking,
            speed,
            thinking_effort,
            prompt_capabilities_tx,
            prompt_capabilities_rx,
            project,
            action_log,
            subagent_context: None,
            draft_prompt: None,
            ui_scroll_position: None,
            running_subagents: Vec::new(),
            inherits_parent_model_settings: true,
            sandboxed_terminal_temp_dir: None,
            sandbox_grants: Rc::new(RefCell::new(ThreadSandboxGrants::default())),
        }
    }

    /// Copies runtime-mutable settings from the parent thread so that
    /// subagents start with the same configuration the user selected.
    /// Every property that `set_*` propagates to `running_subagents`
    /// should be inherited here as well.
    fn inherit_parent_settings(&mut self, parent_thread: &Entity<Thread>, cx: &mut Context<Self>) {
        let parent = parent_thread.read(cx);
        self.speed = parent.speed;
        self.thinking_enabled = parent.thinking_enabled;
        self.thinking_effort = parent.thinking_effort.clone();
        self.summarization_model = parent.summarization_model.clone();
        self.profile_id = parent.profile_id.clone();
    }

    fn apply_model_selection(
        &mut self,
        selection: &LanguageModelSelection,
        cx: &mut Context<Self>,
    ) {
        let Some(model) = Self::resolve_model_from_selection(selection, cx) else {
            log::warn!(
                "failed to resolve configured subagent model: {}/{}",
                selection.provider.0,
                selection.model
            );
            return;
        };

        self.thinking_enabled = selection.enable_thinking && model.supports_thinking();
        self.thinking_effort = selection.effort.clone();
        self.speed = selection.speed.filter(|_| model.supports_fast_mode());
        self.prompt_capabilities_tx
            .send(Self::prompt_capabilities(Some(model.as_ref())))
            .log_err();
        self.model = ThreadModel::Ready(model);
    }

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

    pub fn replay(
        &mut self,
        cx: &mut Context<Self>,
    ) -> mpsc::UnboundedReceiver<Result<ThreadEvent>> {
        let (tx, rx) = mpsc::unbounded();
        let stream = ThreadEventStream(tx);
        for (message_ix, message) in self.messages.iter().enumerate() {
            match &**message {
                Message::User(user_message) => stream.send_user_message(user_message),
                Message::Agent(assistant_message) => {
                    for content in &assistant_message.content {
                        match content {
                            AgentMessageContent::Text(text) => stream.send_text(text),
                            AgentMessageContent::Thinking { text, .. } => {
                                stream.send_thinking(text)
                            }
                            AgentMessageContent::RedactedThinking(_) => {}
                            AgentMessageContent::ToolUse(tool_use) => {
                                self.replay_tool_call(
                                    tool_use,
                                    assistant_message.tool_results.get(&tool_use.id),
                                    &stream,
                                    cx,
                                );
                            }
                        }
                    }
                }
                Message::Resume => {}
                Message::Compaction(info) => {
                    let compaction_id = acp_thread::ContextCompactionId(
                        format!("replay-compaction-{message_ix}").into(),
                    );
                    match info {
                        CompactionInfo::Summary(summary) => {
                            stream.send_context_compaction(
                                compaction_id.clone(),
                                acp_thread::ContextCompactionStatus::Completed,
                            );
                            stream.send_context_compaction_update(compaction_id.clone(), summary);
                        }
                        CompactionInfo::ProviderNative { .. } => {
                            stream.send_context_compaction(
                                compaction_id,
                                acp_thread::ContextCompactionStatus::Completed,
                            );
                        }
                    }
                }
            }
        }
        rx
    }

    fn replay_tool_call(
        &self,
        tool_use: &LanguageModelToolUse,
        tool_result: Option<&LanguageModelToolResult>,
        stream: &ThreadEventStream,
        cx: &mut Context<Self>,
    ) {
        // A tool call left only with the canceled sentinel produced nothing useful
        // (the sentinel is model-facing only, and is inserted exactly when a tool
        // had no real result). Don't replay it into the UI at all.
        if tool_result.is_some_and(Self::is_canceled_tool_result) {
            return;
        }

        let output = tool_result
            .as_ref()
            .and_then(|result| result.output.clone());
        let replay_content = tool_result.and_then(Self::tool_result_content_for_replay);
        let status = tool_result
            .as_ref()
            .map_or(acp::ToolCallStatus::Failed, |result| {
                if result.is_error {
                    acp::ToolCallStatus::Failed
                } else {
                    acp::ToolCallStatus::Completed
                }
            });

        // Recorded tool calls use the model-facing name, so a terminal call is
        // always keyed as `terminal` and resolves to the non-sandboxed
        // `TerminalTool` here, even if it originally ran under
        // `SandboxedTerminalTool`. That's safe because both variants share the
        // same `replay` behavior; replay only reconstructs UI state and never
        // re-runs the command or re-applies sandbox policy.
        let tool = self.tools.get(tool_use.name.as_ref()).cloned().or_else(|| {
            self.context_server_registry
                .read(cx)
                .servers()
                .find_map(|(_, tools)| {
                    if let Some(tool) = tools.get(tool_use.name.as_ref()) {
                        Some(tool.clone())
                    } else {
                        None
                    }
                })
        });

        let Some(tool) = tool else {
            // Tool not found (e.g., MCP server not connected after restart),
            // but still display the saved result if available.
            // We need to send both ToolCall and ToolCallUpdate events because the UI
            // only converts raw_output to displayable content in update_fields, not from_acp.
            stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCall(
                    acp::ToolCall::new(tool_use.id.to_string(), tool_use.name.to_string())
                        .status(status)
                        .raw_input(tool_use.input.clone()),
                )))
                .ok();
            let mut fields = acp::ToolCallUpdateFields::new()
                .status(status)
                .raw_output(output);
            if let Some(content) = replay_content {
                fields = fields.content(content);
            }
            stream.update_tool_call_fields(&tool_use.id, fields, None);
            return;
        };

        let title = tool.initial_title(tool_use.input.clone(), cx);
        let kind = tool.kind();
        stream.send_tool_call(
            &tool_use.id,
            &tool_use.name,
            title,
            kind,
            tool_use.input.clone(),
        );

        if let Some(content) = replay_content {
            stream.update_tool_call_fields(
                &tool_use.id,
                acp::ToolCallUpdateFields::new().content(content),
                None,
            );
        }

        if let Some(output) = output.clone() {
            // For replay, we use a dummy cancellation receiver since the tool already completed
            let (_cancellation_tx, cancellation_rx) = watch::channel(false);
            let tool_event_stream = ToolCallEventStream::new(
                tool_use.id.clone(),
                stream.clone(),
                Some(self.project.read(cx).fs().clone()),
                cancellation_rx,
                self.sandbox_grants.clone(),
                Some(cx.weak_entity()),
            );
            tool.replay(tool_use.input.clone(), output, tool_event_stream, cx)
                .log_err();
        }

        stream.update_tool_call_fields(
            &tool_use.id,
            acp::ToolCallUpdateFields::new()
                .status(status)
                .raw_output(output),
            None,
        );
    }

    /// A canceled tool result carries only the model-facing `TOOL_CANCELED_MESSAGE`
    /// sentinel (inserted exactly when a tool had no real result). It's never
    /// meaningful to the user, so we detect it to skip replaying the tool call.
    fn is_canceled_tool_result(tool_result: &LanguageModelToolResult) -> bool {
        tool_result.is_error
            && matches!(
                tool_result.content.as_slice(),
                [LanguageModelToolResultContent::Text(text)]
                    if text.as_ref() == TOOL_CANCELED_MESSAGE
            )
    }

    fn tool_result_content_for_replay(
        tool_result: &LanguageModelToolResult,
    ) -> Option<Vec<acp::ToolCallContent>> {
        let has_image = tool_result
            .content
            .iter()
            .any(|part| matches!(part, LanguageModelToolResultContent::Image(_)));
        if !has_image && tool_result.output.is_some() {
            return None;
        }

        let content = tool_result
            .content
            .iter()
            .filter_map(|part| match part {
                LanguageModelToolResultContent::Text(text) => {
                    if text.is_empty() {
                        None
                    } else {
                        Some(acp::ToolCallContent::Content(acp::Content::new(
                            acp::ContentBlock::Text(acp::TextContent::new(text.to_string())),
                        )))
                    }
                }
                LanguageModelToolResultContent::Image(image) => Some(
                    acp::ToolCallContent::Content(acp::Content::new(acp::ContentBlock::Image(
                        acp::ImageContent::new(image.source.clone(), "image/png"),
                    ))),
                ),
            })
            .collect::<Vec<_>>();

        if content.is_empty() {
            None
        } else {
            Some(content)
        }
    }

    pub fn from_db(
        id: acp::SessionId,
        db_thread: DbThread,
        project: Entity<Project>,
        project_context: Entity<ProjectContext>,
        context_server_registry: Entity<ContextServerRegistry>,
        templates: Arc<Templates>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = AgentSettings::get_global(cx);
        let profile_id = db_thread
            .profile
            .unwrap_or_else(|| settings.default_profile.clone());

        let saved_selection = db_thread.model.map(|model| SelectedModel {
            provider: model.provider.into(),
            model: model.model.into(),
        });

        let resolved_saved_model = LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            saved_selection
                .as_ref()
                .and_then(|selection| registry.select_model(selection, cx))
                .map(|configured| configured.model)
        });

        let model = match (resolved_saved_model, saved_selection) {
            (Some(model), _) => ThreadModel::Ready(model),
            (None, Some(selection)) => ThreadModel::Unresolved(selection),
            (None, None) => Self::resolve_profile_model(&profile_id, cx)
                .or_else(|| {
                    LanguageModelRegistry::global(cx).update(cx, |registry, _cx| {
                        registry.default_model().map(|model| model.model)
                    })
                })
                .map_or(ThreadModel::Unset, ThreadModel::Ready),
        };

        let (prompt_capabilities_tx, prompt_capabilities_rx) = watch::channel(
            Self::prompt_capabilities(model.as_model().map(|model| model.as_ref())),
        );

        let action_log = cx.new(|_| ActionLog::new(project.clone()));

        Self {
            id,
            prompt_id: PromptId::new(),
            title: if db_thread.title.is_empty() {
                None
            } else {
                Some(db_thread.title.clone())
            },
            pending_title_generation: None,
            title_generation_failed: false,
            pending_summary_generation: None,
            summary: db_thread.detailed_summary,
            messages: db_thread.messages,
            user_store: project.read(cx).user_store(),
            running_turn: None,
            end_turn_at_next_boundary: false,
            pending_message: None,
            tools: BTreeMap::default(),
            request_token_usage: db_thread.request_token_usage.clone(),
            cumulative_token_usage: db_thread.cumulative_token_usage,
            current_request_token_usage: TokenUsage::default(),
            pending_compaction_telemetry: None,
            initial_project_snapshot: Task::ready(db_thread.initial_project_snapshot).shared(),
            context_server_registry,
            profile_id,
            project_context,
            templates,
            model,
            summarization_model: None,
            thinking_enabled: db_thread.thinking_enabled,
            thinking_effort: db_thread.thinking_effort,
            speed: db_thread.speed,
            project,
            action_log,
            updated_at: db_thread.updated_at,
            prompt_capabilities_tx,
            prompt_capabilities_rx,
            subagent_context: db_thread.subagent_context,
            draft_prompt: db_thread.draft_prompt,
            ui_scroll_position: db_thread.ui_scroll_position.map(|sp| gpui::ListOffset {
                item_ix: sp.item_ix,
                offset_in_item: gpui::px(sp.offset_in_item),
            }),
            running_subagents: Vec::new(),
            inherits_parent_model_settings: true,
            sandboxed_terminal_temp_dir: db_thread.sandboxed_terminal_temp_dir,
            sandbox_grants: Rc::new(RefCell::new(ThreadSandboxGrants::from_db(
                &db_thread.sandbox_grants,
            ))),
        }
    }

    /// The sandbox grants configured for this thread, using unverified Git path
    /// candidates. Use [`Self::refresh_verified_sandbox_status`] for UI or other
    /// surfaces that need to match terminal enforcement.
    pub fn sandbox_status(&self, cx: &App) -> Option<(ThreadSandbox, ThreadSandbox)> {
        if !self.sandboxing_available(cx) {
            return None;
        }
        let persistent = AgentSettings::get_global(cx).sandbox_permissions.clone();
        let git_dirs = crate::sandboxing::sandbox_git_dirs(self.project.read(cx), cx);
        let grants = self.sandbox_grants.borrow();
        let settings = crate::sandboxing::settings_thread_sandbox(&persistent)
            .with_git(persistent.allow_git_access, git_dirs.clone());
        let thread = grants
            .thread_sandbox()
            .with_git(grants.git_access_granted(), git_dirs);
        Some((settings, thread))
    }

    pub fn refresh_verified_sandbox_status(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(SandboxStatusKey, SandboxStatusRefresh)> {
        if !self.sandboxing_available(cx) {
            return None;
        }

        let persistent = AgentSettings::get_global(cx).sandbox_permissions.clone();
        let settings_sandbox = crate::sandboxing::settings_thread_sandbox(&persistent);
        let grants = self.sandbox_grants.borrow();
        let thread_sandbox = grants.thread_sandbox();
        let thread_allow_git_access = grants.git_access_granted();
        drop(grants);

        let (sandbox_path_candidates, fs) = {
            let project = self.project.read(cx);
            (
                SandboxGitPathCandidates::from_project(project, cx),
                project.fs().clone(),
            )
        };
        let baseline_writable_paths = sandbox_path_candidates.writable_paths.clone();
        let git_paths = sandbox_path_candidates.git_paths.clone();
        let repository_paths = sandbox_path_candidates.cache_key_repositories();

        let key = SandboxStatusKey {
            settings_sandbox: settings_sandbox.clone(),
            thread_sandbox: thread_sandbox.clone(),
            baseline_writable_paths: baseline_writable_paths.clone(),
            git_paths: git_paths.clone(),
            repository_paths,
            settings_allow_git_access: persistent.allow_git_access,
            thread_allow_git_access,
        };

        if settings_sandbox.is_unsandboxed() || thread_sandbox.is_unsandboxed() {
            return Some((
                key,
                SandboxStatusRefresh::Ready(VerifiedSandboxStatus {
                    settings_sandbox,
                    thread_sandbox,
                    baseline_writable_paths,
                }),
            ));
        }

        let git_access_requested = persistent.allow_git_access || thread_allow_git_access;
        if !git_access_requested {
            return Some((
                key,
                SandboxStatusRefresh::Ready(VerifiedSandboxStatus {
                    settings_sandbox: settings_sandbox.with_git(false, git_paths.clone()),
                    thread_sandbox: thread_sandbox.with_git(false, git_paths),
                    baseline_writable_paths,
                }),
            ));
        }

        let task = cx.spawn(async move |_this, _cx| {
            let sandbox_paths = sandbox_git_paths(sandbox_path_candidates, fs.as_ref(), true).await;
            VerifiedSandboxStatus {
                settings_sandbox: settings_sandbox.with_git(
                    persistent.allow_git_access && sandbox_paths.allow_git_access,
                    sandbox_paths.git_dirs.clone(),
                ),
                thread_sandbox: thread_sandbox.with_git(
                    thread_allow_git_access && sandbox_paths.allow_git_access,
                    sandbox_paths.git_dirs,
                ),
                baseline_writable_paths,
            }
        });

        Some((key, SandboxStatusRefresh::Pending(task)))
    }

    /// Whether agent terminal commands are sandboxed for this thread's project,
    /// so the UI can decide whether to surface the sandbox status at all.
    pub fn sandboxing_enabled(&self, cx: &App) -> bool {
        sandboxing_enabled_for_project(self.project.read(cx), cx)
    }

    /// Whether sandboxing is *applicable* for this thread's project (feature on,
    /// local project, supported platform), regardless of whether it's been
    /// turned off in settings. The UI shows the sandbox indicator whenever this
    /// is true, drawing it struck-out when sandboxing is disabled.
    pub fn sandboxing_available(&self, cx: &App) -> bool {
        sandboxing_available_for_project(self.project.read(cx), cx)
    }

    /// The directory subtrees the sandbox always grants write access to for this
    /// thread's project (its worktree roots), derived from the same source the
    /// terminal tool uses when it actually builds the sandbox.
    pub fn sandbox_baseline_writable_paths(&self, cx: &App) -> Vec<PathBuf> {
        crate::sandboxing::sandbox_worktree_writable_paths(self.project.read(cx), cx)
    }

    pub fn to_db(&self, cx: &App) -> Task<DbThread> {
        let initial_project_snapshot = self.initial_project_snapshot.clone();
        let mut thread = DbThread {
            title: self.title().unwrap_or_default(),
            messages: self.messages.clone(),
            updated_at: self.updated_at,
            detailed_summary: self.summary.clone(),
            initial_project_snapshot: None,
            cumulative_token_usage: self.cumulative_token_usage,
            request_token_usage: self.request_token_usage.clone(),
            model: (&self.model).into(),
            profile: Some(self.profile_id.clone()),
            subagent_context: self.subagent_context.clone(),
            speed: self.speed,
            thinking_enabled: self.thinking_enabled,
            thinking_effort: self.thinking_effort.clone(),
            draft_prompt: self.draft_prompt.clone(),
            ui_scroll_position: self.ui_scroll_position.map(|lo| {
                crate::db::SerializedScrollPosition {
                    item_ix: lo.item_ix,
                    offset_in_item: lo.offset_in_item.as_f32(),
                }
            }),
            sandboxed_terminal_temp_dir: self.sandboxed_terminal_temp_dir.clone(),
            sandbox_grants: self.sandbox_grants.borrow().to_db(),
        };

        cx.background_spawn(async move {
            let initial_project_snapshot = initial_project_snapshot.await;
            thread.initial_project_snapshot = initial_project_snapshot;
            thread
        })
    }

    /// Create a snapshot of the current project state including git information and unsaved buffers.
    fn project_snapshot(
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Task<Arc<ProjectSnapshot>> {
        let task = project::telemetry_snapshot::TelemetrySnapshot::new(&project, cx);
        cx.spawn(async move |_, _| {
            let snapshot = task.await;

            Arc::new(ProjectSnapshot {
                worktree_snapshots: snapshot.worktree_snapshots,
                timestamp: Utc::now(),
            })
        })
    }

    pub fn project_context(&self) -> &Entity<ProjectContext> {
        &self.project_context
    }

    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }

    pub fn action_log(&self) -> &Entity<ActionLog> {
        &self.action_log
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty() && self.title.is_none()
    }

    pub fn draft_prompt(&self) -> Option<&[acp::ContentBlock]> {
        self.draft_prompt.as_deref()
    }

    pub fn set_draft_prompt(&mut self, prompt: Option<Vec<acp::ContentBlock>>) {
        self.draft_prompt = prompt;
    }

    pub fn ui_scroll_position(&self) -> Option<gpui::ListOffset> {
        self.ui_scroll_position
    }

    pub fn set_ui_scroll_position(&mut self, position: Option<gpui::ListOffset>) {
        self.ui_scroll_position = position;
    }

    pub fn model(&self) -> Option<&Arc<dyn LanguageModel>> {
        self.model.as_model()
    }

    pub(crate) fn ensure_model(
        &mut self,
        default_model: Option<&Arc<dyn LanguageModel>>,
        cx: &mut Context<Self>,
    ) {
        let resolved = match &self.model {
            ThreadModel::Ready(_) => return,
            ThreadModel::Unresolved(selection) => {
                LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
                    registry
                        .select_model(selection, cx)
                        .map(|configured| configured.model)
                })
            }
            ThreadModel::Unset => default_model.cloned(),
        };

        if let Some(model) = resolved {
            self.set_model(model, cx);
        }
    }

    pub fn set_model(&mut self, model: Arc<dyn LanguageModel>, cx: &mut Context<Self>) {
        let old_usage = self.latest_token_usage();
        self.model = ThreadModel::Ready(model.clone());
        let new_caps = Self::prompt_capabilities(self.model.as_model().map(|model| model.as_ref()));
        let new_usage = self.latest_token_usage();
        if old_usage != new_usage {
            cx.emit(TokenUsageUpdated(new_usage));
        }
        self.prompt_capabilities_tx.send(new_caps).log_err();

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    if thread.inherits_parent_model_settings {
                        thread.set_model(model.clone(), cx);
                    }
                })
                .ok();
        }

        cx.notify()
    }

    pub fn summarization_model(&self) -> Option<&Arc<dyn LanguageModel>> {
        self.summarization_model.as_ref()
    }

    pub fn set_summarization_model(
        &mut self,
        model: Option<Arc<dyn LanguageModel>>,
        cx: &mut Context<Self>,
    ) {
        self.summarization_model = model.clone();

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    thread.set_summarization_model(model.clone(), cx)
                })
                .ok();
        }
        cx.notify()
    }

    pub fn thinking_enabled(&self) -> bool {
        self.thinking_enabled
    }

    pub fn set_thinking_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.thinking_enabled = enabled;

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    if thread.inherits_parent_model_settings {
                        thread.set_thinking_enabled(enabled, cx);
                    }
                })
                .ok();
        }
        cx.notify();
    }

    pub fn thinking_effort(&self) -> Option<&String> {
        self.thinking_effort.as_ref()
    }

    pub fn set_thinking_effort(&mut self, effort: Option<String>, cx: &mut Context<Self>) {
        self.thinking_effort = effort.clone();

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    if thread.inherits_parent_model_settings {
                        thread.set_thinking_effort(effort.clone(), cx)
                    }
                })
                .ok();
        }
        cx.notify();
    }

    pub fn speed(&self) -> Option<Speed> {
        self.speed
    }

    pub fn set_speed(&mut self, speed: Speed, cx: &mut Context<Self>) {
        self.speed = Some(speed);

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| {
                    if thread.inherits_parent_model_settings {
                        thread.set_speed(speed, cx);
                    }
                })
                .ok();
        }
        cx.notify();
    }

    pub fn last_message(&self) -> Option<&Message> {
        self.messages.last().map(std::ops::Deref::deref)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn last_received_or_pending_message(&self) -> Option<Arc<Message>> {
        if let Some(message) = self.pending_message.clone() {
            Some(Arc::new(Message::Agent(message)))
        } else {
            self.messages.last().cloned()
        }
    }

    pub fn add_default_tools(
        &mut self,
        environment: Rc<dyn ThreadEnvironment>,
        cx: &mut Context<Self>,
    ) {
        // Only update the agent location for the root thread, not for subagents.
        let update_agent_location = self.parent_thread_id().is_none();

        let language_registry = self.project.read(cx).languages().clone();
        self.add_tool(CopyPathTool::new(self.project.clone()));
        self.add_tool(CreateDirectoryTool::new(self.project.clone()));
        self.add_tool(DeletePathTool::new(
            self.project.clone(),
            self.action_log.clone(),
        ));
        self.add_tool(EditFileTool::new(
            self.project.clone(),
            cx.weak_entity(),
            self.action_log.clone(),
            language_registry.clone(),
        ));
        self.add_tool(WriteFileTool::new(
            self.project.clone(),
            cx.weak_entity(),
            self.action_log.clone(),
            language_registry,
        ));
        self.add_tool(FetchTool::new(self.project.read(cx).client().http_client()));
        self.add_tool(FindPathTool::new(self.project.clone()));
        self.add_tool(GrepTool::new(self.project.clone()));
        self.add_tool(ListDirectoryTool::new(self.project.clone()));
        self.add_tool(MovePathTool::new(self.project.clone()));
        self.add_tool(ReadFileTool::new(
            self.project.clone(),
            self.action_log.clone(),
            update_agent_location,
        ));
        // Register terminal tool variants; `enabled_tools` exposes the one
        // matching the current sandbox state to the model as `terminal`.
        self.add_tool(TerminalTool::new(self.project.clone(), environment.clone()));
        self.add_tool(SandboxedTerminalTool::new(
            self.project.clone(),
            environment.clone(),
        ));
        self.add_tool(WebSearchTool);

        self.add_tool(DiagnosticsTool::new(self.project.clone()));

        let code_action_store: CodeActionStore = cx.new(|_cx| None);
        self.add_tool(FindReferencesTool::new(self.project.clone()));
        self.add_tool(GetCodeActionsTool::new(
            self.project.clone(),
            code_action_store.clone(),
        ));
        self.add_tool(ApplyCodeActionTool::new(
            self.project.clone(),
            code_action_store,
        ));
        self.add_tool(GoToDefinitionTool::new(self.project.clone()));
        self.add_tool(RenameTool::new(self.project.clone()));

        if self.depth() < MAX_SUBAGENT_DEPTH {
            self.add_tool(SpawnAgentTool::new(environment.clone()));
        }

        // Sibling-thread tools are exposed at every depth: a subagent should
        // still be able to kick off independent sibling work on behalf of the
        // user, even when it can no longer nest further subagents. Visibility
        // to the model is gated by `CreateThreadToolFeatureFlag` in
        // `Thread::enabled_tools`.
        self.add_tool(CreateThreadTool::new(environment.clone()));
        self.add_tool(ListAgentsAndModelsTool::new(environment));
    }

    pub fn add_tool<T: AgentTool>(&mut self, tool: T) {
        debug_assert!(
            !self.tools.contains_key(T::NAME),
            "Duplicate tool name: {}",
            T::NAME,
        );
        self.tools.insert(T::NAME.into(), tool.erase());
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn remove_tool(&mut self, name: &str) -> bool {
        self.tools.remove(name).is_some()
    }

    pub fn profile(&self) -> &AgentProfileId {
        &self.profile_id
    }

    pub fn set_profile(&mut self, profile_id: AgentProfileId, cx: &mut Context<Self>) {
        if self.profile_id == profile_id {
            return;
        }

        self.profile_id = profile_id.clone();

        // Swap to the profile's preferred model when available.
        if let Some(model) = Self::resolve_profile_model(&self.profile_id, cx) {
            self.set_model(model, cx);
        }

        for subagent in &self.running_subagents {
            subagent
                .update(cx, |thread, cx| thread.set_profile(profile_id.clone(), cx))
                .ok();
        }
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) -> Task<()> {
        for subagent in self.running_subagents.drain(..) {
            if let Some(subagent) = subagent.upgrade() {
                subagent.update(cx, |thread, cx| thread.cancel(cx)).detach();
            }
        }

        let Some(running_turn) = self.running_turn.take() else {
            self.flush_pending_message(cx);
            return Task::ready(());
        };

        let turn_task = running_turn.cancel();

        cx.spawn(async move |this, cx| {
            turn_task.await;
            this.update(cx, |this, cx| {
                this.flush_pending_message(cx);
            })
            .ok();
        })
    }

    pub fn set_end_turn_at_next_boundary(&mut self, end_at_boundary: bool) {
        self.end_turn_at_next_boundary = end_at_boundary;
    }

    pub fn end_turn_at_next_boundary(&self) -> bool {
        self.end_turn_at_next_boundary
    }

    fn accumulate_token_usage(&mut self, update: language_model::TokenUsage) {
        let previous_accounted_usage = self.current_request_token_usage;
        let current_accounted_usage = TokenUsage {
            input_tokens: previous_accounted_usage
                .input_tokens
                .max(update.input_tokens),
            output_tokens: previous_accounted_usage
                .output_tokens
                .max(update.output_tokens),
            cache_creation_input_tokens: previous_accounted_usage
                .cache_creation_input_tokens
                .max(update.cache_creation_input_tokens),
            cache_read_input_tokens: previous_accounted_usage
                .cache_read_input_tokens
                .max(update.cache_read_input_tokens),
        };
        self.current_request_token_usage = current_accounted_usage;
        self.cumulative_token_usage = self.cumulative_token_usage
            + TokenUsage {
                input_tokens: current_accounted_usage
                    .input_tokens
                    .saturating_sub(previous_accounted_usage.input_tokens),
                output_tokens: current_accounted_usage
                    .output_tokens
                    .saturating_sub(previous_accounted_usage.output_tokens),
                cache_creation_input_tokens: current_accounted_usage
                    .cache_creation_input_tokens
                    .saturating_sub(previous_accounted_usage.cache_creation_input_tokens),
                cache_read_input_tokens: current_accounted_usage
                    .cache_read_input_tokens
                    .saturating_sub(previous_accounted_usage.cache_read_input_tokens),
            };
    }

    fn update_token_usage(&mut self, update: language_model::TokenUsage, cx: &mut Context<Self>) {
        self.accumulate_token_usage(update);

        let Some(last_user_message) = self.last_user_message() else {
            return;
        };

        self.request_token_usage
            .insert(last_user_message.id.clone(), update);
        cx.emit(TokenUsageUpdated(self.latest_token_usage()));
        cx.notify();
    }

    pub fn truncate(
        &mut self,
        client_user_message_id: ClientUserMessageId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.cancel(cx).detach();
        // Clear pending message since cancel will try to flush it asynchronously,
        // and we don't want that content to be added after we truncate
        self.pending_message.take();
        let Some(position) = self.messages.iter().position(|msg| {
            matches!(&**msg, Message::User(UserMessage { id, .. }) if id == &client_user_message_id)
        }) else {
            return Err(anyhow!("Message not found"));
        };

        for message in self.messages.drain(position..) {
            match &*message {
                Message::User(message) => {
                    self.request_token_usage.remove(&message.id);
                }
                Message::Agent(_) | Message::Resume | Message::Compaction(_) => {}
            }
        }
        self.clear_summary();
        cx.notify();
        Ok(())
    }

    pub fn latest_request_token_usage(&self) -> Option<language_model::TokenUsage> {
        let last_user_message = self.last_user_message()?;
        let tokens = self.request_token_usage.get(&last_user_message.id)?;
        Some(*tokens)
    }

    pub fn cumulative_token_usage(&self) -> language_model::TokenUsage {
        self.cumulative_token_usage
    }

    pub fn latest_token_usage(&self) -> Option<acp_thread::TokenUsage> {
        let usage = self.latest_request_token_usage()?;
        let model = self.model()?;
        let input_tokens = total_input_tokens(usage);

        Some(acp_thread::TokenUsage {
            max_tokens: model.max_token_count(),
            max_output_tokens: model.max_output_tokens(),
            used_tokens: usage.total_tokens(),
            input_tokens,
            output_tokens: usage.output_tokens,
        })
    }

    /// Get the total input token count as of the message before the given message.
    ///
    /// Returns `None` if:
    /// - `target_id` is the first message (no previous message)
    /// - The previous message hasn't received a response yet (no usage data)
    /// - `target_id` is not found in the messages
    pub fn tokens_before_message(&self, target_id: &ClientUserMessageId) -> Option<u64> {
        let mut previous_user_message_id: Option<&ClientUserMessageId> = None;

        for message in &self.messages {
            if let Message::User(user_msg) = &**message {
                if &user_msg.id == target_id {
                    let prev_id = previous_user_message_id?;
                    let usage = self.request_token_usage.get(prev_id)?;
                    return Some(total_input_tokens(*usage));
                }
                previous_user_message_id = Some(&user_msg.id);
            }
        }
        None
    }

    /// Look up the active profile and resolve its preferred model if one is configured.
    fn resolve_profile_model(
        profile_id: &AgentProfileId,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn LanguageModel>> {
        let selection = AgentSettings::get_global(cx)
            .profiles
            .get(profile_id)?
            .default_model
            .clone()?;
        Self::resolve_model_from_selection(&selection, cx)
    }

    /// Translate a stored model selection into the configured model from the registry.
    fn resolve_model_from_selection(
        selection: &LanguageModelSelection,
        cx: &mut Context<Self>,
    ) -> Option<Arc<dyn LanguageModel>> {
        let selected = SelectedModel {
            provider: LanguageModelProviderId::from(selection.provider.0.clone()),
            model: LanguageModelId::from(selection.model.clone()),
        };
        LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            registry
                .select_model(&selected, cx)
                .map(|configured| configured.model)
        })
    }

    pub fn resume(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>> {
        self.messages.push(Arc::new(Message::Resume));
        cx.notify();

        log::debug!("Total messages in thread: {}", self.messages.len());
        self.run_turn(cx)
    }

    /// Sending a message results in the model streaming a response, which could include tool calls.
    /// After calling tools, the model will stops and waits for any outstanding tool calls to be completed and their results sent.
    /// The returned channel will report all the occurrences in which the model stops before erroring or ending its turn.
    pub fn send<T>(
        &mut self,
        id: ClientUserMessageId,
        content: impl IntoIterator<Item = T>,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>>
    where
        T: Into<UserMessageContent>,
    {
        let content = content.into_iter().map(Into::into).collect::<Arc<_>>();
        log::debug!("Thread::send content: {:?}", content);

        self.messages
            .push(Arc::new(Message::User(UserMessage { id, content })));
        cx.notify();

        self.send_existing(cx)
    }

    pub fn send_existing(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>> {
        let model = self
            .model()
            .ok_or_else(|| anyhow!(NoModelConfiguredError))?;

        log::info!("Thread::send called with model: {}", model.name().0);
        self.advance_prompt_id();

        log::debug!("Total messages in thread: {}", self.messages.len());
        self.run_turn(cx)
    }

    /// Force a manual context compaction using the summary strategy,
    /// regardless of the current token usage or context window size.
    pub fn compact(
        &mut self,
        id: ClientUserMessageId,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>> {
        let model = self
            .model()
            .cloned()
            .ok_or_else(|| anyhow!(NoModelConfiguredError))?;

        // Flush any pending message and cancel an in-flight turn before we
        // start, mirroring `run_turn` so a stray completion can't race with the
        // compaction we're about to perform.
        self.flush_pending_message(cx);
        self.cancel(cx).detach();

        let compaction = self.forced_compaction_target_ix().map(|request_end_ix| {
            self.advance_prompt_id();
            let request = self.build_compaction_request(request_end_ix, &model, cx);
            self.current_request_token_usage = TokenUsage::default();
            (model, request)
        });

        if compaction.is_some() {
            self.pending_compaction_telemetry = self.build_compaction_telemetry("manual", cx);
        }

        self.clear_summary();
        cx.notify();

        let (events_tx, events_rx) = mpsc::unbounded::<Result<ThreadEvent>>();
        let event_stream = ThreadEventStream(events_tx);
        let (cancellation_tx, mut cancellation_rx) = watch::channel(false);
        let task = cx.spawn({
            let event_stream = event_stream.clone();
            async move |this, cx| {
                let result = if let Some((model, request)) = compaction {
                    Self::stream_compaction(
                        &this,
                        &event_stream,
                        cancellation_rx.clone(),
                        model,
                        request,
                        CompactionInsertion::Manual { marker_id: id },
                        cx,
                    )
                    .await
                } else {
                    Ok(ControlFlow::Continue(()))
                };

                // If we were cancelled, `cancel()` already took `running_turn`
                // (possibly for a new turn), so leave it alone.
                if *cancellation_rx.borrow() {
                    this.update(cx, |this, _| {
                        this.emit_compaction_telemetry_outcome("canceled", None)
                    })
                    .log_err();
                    return;
                }

                match result {
                    // On success, the telemetry event is deferred until the next
                    // completion reports usage (see `handle_completion_event`),
                    // so we leave `pending_compaction_telemetry` in place here.
                    Ok(_) => event_stream.send_stop(acp::StopReason::EndTurn),
                    Err(error) => {
                        log::error!("Manual compaction failed: {:?}", error);
                        this.update(cx, |this, _| {
                            this.emit_compaction_telemetry_outcome(
                                "failed",
                                Some(error.to_string()),
                            )
                        })
                        .log_err();
                        event_stream.send_error(error);
                    }
                }

                _ = this.update(cx, |this, _| this.running_turn.take());
            }
        });
        self.running_turn = Some(RunningTurn::new(
            event_stream,
            BTreeMap::default(),
            cancellation_tx,
            task,
        ));

        Ok(events_rx)
    }

    pub fn push_acp_user_block(
        &mut self,
        id: ClientUserMessageId,
        blocks: impl IntoIterator<Item = acp::ContentBlock>,
        path_style: PathStyle,
        cx: &mut Context<Self>,
    ) {
        let content = blocks
            .into_iter()
            .map(|block| UserMessageContent::from_content_block(block, path_style))
            .collect::<Arc<_>>();
        self.messages
            .push(Arc::new(Message::User(UserMessage { id, content })));
        cx.notify();
    }

    pub fn push_acp_agent_block(&mut self, block: acp::ContentBlock, cx: &mut Context<Self>) {
        let text = match block {
            acp::ContentBlock::Text(text_content) => text_content.text,
            acp::ContentBlock::Image(_) => "[image]".to_string(),
            acp::ContentBlock::Audio(_) => "[audio]".to_string(),
            acp::ContentBlock::ResourceLink(resource_link) => resource_link.uri,
            acp::ContentBlock::Resource(resource) => match resource.resource {
                acp::EmbeddedResourceResource::TextResourceContents(resource) => resource.uri,
                acp::EmbeddedResourceResource::BlobResourceContents(resource) => resource.uri,
                _ => "[resource]".to_string(),
            },
            _ => "[unknown]".to_string(),
        };

        self.messages.push(Arc::new(Message::Agent(AgentMessage {
            content: vec![AgentMessageContent::Text(text)],
            ..Default::default()
        })));
        cx.notify();
    }

    fn run_turn(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>> {
        // Flush the old pending message synchronously before cancelling,
        // to avoid a race where the detached cancel task might flush the NEW
        // turn's pending message instead of the old one.
        self.flush_pending_message(cx);
        self.cancel(cx).detach();

        let (events_tx, events_rx) = mpsc::unbounded::<Result<ThreadEvent>>();
        let event_stream = ThreadEventStream(events_tx);
        let message_ix = self.messages.len().saturating_sub(1);
        self.clear_summary();
        let tools = self.enabled_tools(cx);
        let (cancellation_tx, mut cancellation_rx) = watch::channel(false);
        let task = cx.spawn({
            let event_stream = event_stream.clone();
            async move |this, cx| {
                log::debug!("Starting agent turn execution");

                let turn_result =
                    Self::run_turn_internal(&this, &event_stream, cancellation_rx.clone(), cx)
                        .await;

                // Check if we were cancelled - if so, cancel() already took running_turn
                // and we shouldn't touch it (it might be a NEW turn now)
                let was_cancelled = *cancellation_rx.borrow();
                if was_cancelled {
                    log::debug!("Turn was cancelled, skipping cleanup");
                    return;
                }

                _ = this.update(cx, |this, cx| this.flush_pending_message(cx));

                match turn_result {
                    Ok(()) => {
                        log::debug!("Turn execution completed");
                        event_stream.send_stop(acp::StopReason::EndTurn);
                    }
                    Err(error) => {
                        log::error!("Turn execution failed: {:?}", error);
                        match error.downcast::<CompletionError>() {
                            Ok(CompletionError::Refusal) => {
                                event_stream.send_stop(acp::StopReason::Refusal);
                                _ = this.update(cx, |this, _| this.messages.truncate(message_ix));
                            }
                            Ok(CompletionError::MaxTokens) => {
                                event_stream.send_stop(acp::StopReason::MaxTokens);
                            }
                            Ok(CompletionError::Other(error)) | Err(error) => {
                                event_stream.send_error(error);
                            }
                        }
                    }
                }

                _ = this.update(cx, |this, _| this.running_turn.take());
            }
        });
        self.running_turn = Some(RunningTurn::new(event_stream, tools, cancellation_tx, task));
        Ok(events_rx)
    }

    async fn run_turn_internal(
        this: &WeakEntity<Self>,
        event_stream: &ThreadEventStream,
        mut cancellation_rx: watch::Receiver<bool>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let mut attempt = 0;
        let mut intent = CompletionIntent::UserPrompt;
        // Set when a refusal fallback occurs so subsequent iterations use the fallback model.
        let mut refusal_fallback_model: Option<Arc<dyn LanguageModel>> = None;
        loop {
            match Self::perform_compaction_if_needed(
                this,
                event_stream,
                cancellation_rx.clone(),
                cx,
            )
            .await
            {
                // On success the telemetry event is deferred until the
                // completion below reports usage, so we can record an
                // accurate post-compaction context size (see
                // `handle_completion_event`).
                Ok(ControlFlow::Continue(())) => {}
                Ok(ControlFlow::Break(())) => {
                    this.update(cx, |this, _| {
                        this.emit_compaction_telemetry_outcome("canceled", None)
                    })?;
                    return Ok(());
                }
                Err(error) => {
                    log::error!("Compaction failed: {}", error);
                    let error_message = error.to_string();
                    match error.downcast::<LanguageModelCompletionError>() {
                        Ok(error) => {
                            attempt += 1;
                            match Self::retry_completion_error(
                                this,
                                event_stream,
                                &mut cancellation_rx,
                                error,
                                attempt,
                                cx,
                            )
                            .await
                            {
                                Ok(ControlFlow::Break(())) => {
                                    this.update(cx, |this, _| {
                                        this.emit_compaction_telemetry_outcome("canceled", None)
                                    })?;
                                    return Ok(());
                                }
                                Ok(ControlFlow::Continue(())) => {
                                    this.update(cx, |this, _| {
                                        if let Some(telemetry) =
                                            this.pending_compaction_telemetry.as_mut()
                                        {
                                            telemetry.retries += 1;
                                        }
                                    })?;
                                    continue;
                                }
                                Err(retry_error) => {
                                    this.update(cx, |this, _| {
                                        this.emit_compaction_telemetry_outcome(
                                            "failed",
                                            Some(error_message),
                                        )
                                    })?;
                                    return Err(retry_error);
                                }
                            }
                        }
                        Err(error) => {
                            this.update(cx, |this, _| {
                                this.emit_compaction_telemetry_outcome(
                                    "failed",
                                    Some(error_message),
                                )
                            })?;
                            return Err(error);
                        }
                    }
                }
            }

            // Re-read the model and refresh tools on each iteration so that
            // mid-turn changes (e.g. the user switches model, toggles tools,
            // or changes profile) take effect between tool-call rounds.
            // If a refusal fallback is active, use that model instead.
            let (model, request) = this.update(cx, |this, cx| {
                let model = refusal_fallback_model
                    .clone()
                    .or_else(|| this.model().cloned())
                    .ok_or_else(|| anyhow!(NoModelConfiguredError))?;
                this.refresh_turn_tools(cx);
                let request = this.build_completion_request(intent, cx)?;
                this.current_request_token_usage = TokenUsage::default();
                anyhow::Ok((model, request))
            })??;

            telemetry::event!(
                "Agent Thread Completion",
                thread_id = this.read_with(cx, |this, _| this.id.to_string())?,
                parent_thread_id = this.read_with(cx, |this, _| this
                    .parent_thread_id()
                    .map(|id| id.to_string()))?,
                prompt_id = this.read_with(cx, |this, _| this.prompt_id.to_string())?,
                model = model.telemetry_id(),
                model_provider = model.provider_id().to_string(),
                attempt
            );

            log::debug!("Calling model.stream_completion, attempt {}", attempt);

            let (mut events, mut error) = match model.stream_completion(request, cx).await {
                Ok(events) => (events.fuse(), None),
                Err(err) => (stream::empty().boxed().fuse(), Some(err)),
            };
            let mut tool_results: FuturesUnordered<Task<LanguageModelToolResult>> =
                FuturesUnordered::new();
            let mut early_tool_results: Vec<LanguageModelToolResult> = Vec::new();
            let mut cancelled = false;
            let mut had_refusal = false;
            loop {
                // Race between getting the first event, tool completion, and cancellation.
                let first_event = futures::select! {
                    event = events.next().fuse() => event,
                    tool_result = futures::StreamExt::select_next_some(&mut tool_results) => {
                        let is_error = tool_result.is_error;
                        let is_still_streaming = this
                            .read_with(cx, |this, _cx| {
                                this.running_turn
                                    .as_ref()
                                    .and_then(|turn| turn.streaming_tool_inputs.get(&tool_result.tool_use_id))
                                    .map_or(false, |inputs| !inputs.has_received_final())
                            })
                            .unwrap_or(false);

                        early_tool_results.push(tool_result);

                        // Only break if the tool errored and we are still
                        // streaming the input of the tool. If the tool errored
                        // but we are no longer streaming its input (i.e. there
                        // are parallel tool calls) we want to continue
                        // processing those tool inputs.
                        if is_error && is_still_streaming {
                            break;
                        }
                        continue;
                    }
                    _ = cancellation_rx.changed().fuse() => {
                        if *cancellation_rx.borrow() {
                            cancelled = true;
                            break;
                        }
                        continue;
                    }
                };
                let Some(first_event) = first_event else {
                    break;
                };

                // Collect all immediately available events to process as a batch
                let mut batch = vec![first_event];
                while let Some(event) = events.next().now_or_never().flatten() {
                    batch.push(event);
                }

                // Process the batch in a single update
                let batch_result = this.update(cx, |this, cx| {
                    let mut batch_tool_results = Vec::new();
                    let mut batch_error = None;

                    for event in batch {
                        log::trace!("Received completion event: {:?}", event);
                        match event {
                            Ok(event) => {
                                match this.handle_completion_event(
                                    event,
                                    event_stream,
                                    cancellation_rx.clone(),
                                    cx,
                                ) {
                                    Ok(Some(task)) => batch_tool_results.push(task),
                                    Ok(None) => {}
                                    Err(err) => {
                                        batch_error = Some(err);
                                        break;
                                    }
                                }
                            }
                            Err(err) => {
                                batch_error = Some(err.into());
                                break;
                            }
                        }
                    }

                    cx.notify();
                    (batch_tool_results, batch_error)
                })?;

                tool_results.extend(batch_result.0);
                if let Some(err) = batch_result.1 {
                    let is_refusal = err
                        .downcast_ref::<CompletionError>()
                        .is_some_and(|e| matches!(e, CompletionError::Refusal));
                    if is_refusal {
                        log::info!("Model refused request; checking for fallback model");
                        had_refusal = true;
                        break;
                    }
                    error = Some(err.downcast()?);
                    break;
                }
            }

            // Drop the stream to release the rate limit permit before tool execution.
            // The stream holds a semaphore guard that limits concurrent requests.
            // Without this, the permit would be held during potentially long-running
            // tool execution, which could cause deadlocks when tools spawn subagents
            // that need their own permits.
            drop(events);

            // Drop streaming tool input senders that never received their final input.
            // This prevents deadlock when the LLM stream ends (e.g. because of an error)
            // before sending a tool use with `is_input_complete: true`.
            this.update(cx, |this, _cx| {
                if let Some(running_turn) = this.running_turn.as_mut() {
                    if running_turn.streaming_tool_inputs.is_empty() {
                        return;
                    }
                    log::warn!("Dropping partial tool inputs because the stream ended");
                    running_turn.streaming_tool_inputs.drain();
                }
            })?;

            if had_refusal {
                let maybe_fallback = this.update(cx, |this, cx| -> Option<Arc<dyn LanguageModel>> {
                    let current_model = refusal_fallback_model.as_ref().or(this.model())?;
                    let fallback_id = match current_model.refusal_fallback_model_id() {
                        Some(id) => id,
                        None => {
                            log::info!(
                                "Refusal fallback: no fallback configured for model {} (provider {})",
                                current_model.id().0,
                                current_model.provider_id()
                            );
                            return None;
                        }
                    };
                    let provider_id = current_model.provider_id();
                    let found = LanguageModelRegistry::global(cx)
                        .read(cx)
                        .available_models(cx)
                        .find(|m| {
                            m.provider_id() == provider_id && m.id().0.as_ref() == fallback_id
                        });
                    if found.is_none() {
                        log::info!(
                            "Refusal fallback: fallback model {}/{} not found in available models",
                            provider_id,
                            fallback_id
                        );
                    }
                    found
                })?;

                if let Some(fallback) = maybe_fallback {
                    log::info!("Refusal fallback: retrying with {}", fallback.id().0);
                    let fallback_name = fallback.name().0.clone();
                    this.update(cx, |this, cx| {
                        this.pending_message = None;
                        this.set_model(fallback.clone(), cx);
                    })?;
                    event_stream.send_retry(acp_thread::RetryStatus {
                        last_error: "Safety filter triggered".into(),
                        attempt: 1,
                        max_attempts: 1,
                        started_at: Instant::now(),
                        duration: Duration::MAX,
                        meta: Some(acp_thread::meta_with_refusal_fallback(&fallback_name)),
                    });
                    refusal_fallback_model = Some(fallback);
                    continue;
                }
                log::info!("Request refused with no fallback model available");
                return Err(CompletionError::Refusal.into());
            }

            let end_turn = tool_results.is_empty() && early_tool_results.is_empty();

            for tool_result in early_tool_results {
                Self::process_tool_result(this, event_stream, cx, tool_result)?;
            }
            while let Some(tool_result) = tool_results.next().await {
                Self::process_tool_result(this, event_stream, cx, tool_result)?;
            }

            this.update(cx, |this, cx| {
                this.flush_pending_message(cx);
                if this.title.is_none() {
                    this.generate_title(cx);
                }
            })?;

            if cancelled {
                log::debug!("Turn cancelled by user, exiting");
                return Ok(());
            }

            if let Some(error) = error {
                attempt += 1;
                match Self::retry_completion_error(
                    this,
                    event_stream,
                    &mut cancellation_rx,
                    error,
                    attempt,
                    cx,
                )
                .await?
                {
                    ControlFlow::Break(_) => return Ok(()),
                    ControlFlow::Continue(_) => {}
                }
                this.update(cx, |this, _cx| {
                    if let Some(Message::Agent(message)) = this.last_message() {
                        if message.tool_results.is_empty() {
                            intent = CompletionIntent::UserPrompt;
                            this.messages.push(Arc::new(Message::Resume));
                        }
                    }
                })?;
            } else if end_turn {
                return Ok(());
            } else {
                let end_at_boundary =
                    this.update(cx, |this, _| this.end_turn_at_next_boundary())?;
                if end_at_boundary {
                    log::debug!("Steering message queued, ending turn at message boundary");
                    return Ok(());
                }
                intent = CompletionIntent::ToolResults;
                attempt = 0;
            }
        }
    }

    /// Computes the retry status for a failed completion, notifies listeners,
    /// and waits out the backoff delay (or returns early if the turn is
    /// cancelled while waiting). Returns an error if the completion is not
    /// retryable or retries are exhausted.
    async fn retry_completion_error(
        this: &WeakEntity<Self>,
        event_stream: &ThreadEventStream,
        cancellation_rx: &mut watch::Receiver<bool>,
        error: LanguageModelCompletionError,
        attempt: u8,
        cx: &mut AsyncApp,
    ) -> Result<ControlFlow<()>> {
        let retry = this.update(cx, |this, cx| {
            let user_store = this.user_store.read(cx);
            this.handle_completion_error(error, attempt, user_store.plan())
        })??;
        let timer = cx.background_executor().timer(retry.duration);
        event_stream.send_retry(retry);
        futures::select! {
            _ = timer.fuse() => {}
            _ = cancellation_rx.changed().fuse() => {
                if *cancellation_rx.borrow() {
                    log::debug!("Turn cancelled during retry delay, exiting");
                    return Ok(ControlFlow::Break(()));
                }
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    async fn perform_compaction_if_needed(
        this: &WeakEntity<Self>,
        event_stream: &ThreadEventStream,
        cancellation_rx: watch::Receiver<bool>,
        cx: &mut AsyncApp,
    ) -> Result<ControlFlow<()>> {
        let Some((model, request, insertion_ix)) = this.update(cx, |this, cx| {
            let insertion_ix = this.compaction_message_target_ix(cx)?;
            let model = this.model().cloned()?;
            let request = this.build_compaction_request(insertion_ix, &model, cx);
            this.current_request_token_usage = TokenUsage::default();
            // Preserve telemetry across retries so the retry count keeps
            // accumulating rather than resetting on each attempt.
            if this.pending_compaction_telemetry.is_none() {
                this.pending_compaction_telemetry = this.build_compaction_telemetry("auto", cx);
            }
            Some((model, request, insertion_ix))
        })?
        else {
            return Ok(ControlFlow::Continue(()));
        };

        Self::stream_compaction(
            this,
            event_stream,
            cancellation_rx,
            model,
            request,
            CompactionInsertion::Auto { insertion_ix },
            cx,
        )
        .await
    }

    async fn stream_compaction(
        this: &WeakEntity<Self>,
        event_stream: &ThreadEventStream,
        mut cancellation_rx: watch::Receiver<bool>,
        model: Arc<dyn LanguageModel>,
        request: LanguageModelRequest,
        insertion: CompactionInsertion,
        cx: &mut AsyncApp,
    ) -> Result<ControlFlow<()>> {
        log::debug!("Running compaction");
        let compaction_id = acp_thread::ContextCompactionId(Uuid::new_v4().to_string().into());
        event_stream.send_context_compaction(
            compaction_id.clone(),
            acp_thread::ContextCompactionStatus::InProgress,
        );
        let stream = futures::select! {
            result = model.stream_completion(request, cx).fuse() => result,
            _ = cancellation_rx.changed().fuse() => {
                if *cancellation_rx.borrow() {
                    log::debug!("Compaction cancelled before request started");
                    return Ok(ControlFlow::Break(()));
                }
                return Ok(ControlFlow::Continue(()));
            }
        };
        let mut stream = stream?;

        let mut summary = String::new();
        loop {
            let event = futures::select! {
                event = stream.next().fuse() => event,
                _ = cancellation_rx.changed().fuse() => {
                    if *cancellation_rx.borrow() {
                        log::debug!("Compaction cancelled while summarizing");
                        return Ok(ControlFlow::Break(()));
                    }
                    continue;
                }
            };

            let Some(event) = event else {
                break;
            };

            match event? {
                LanguageModelCompletionEvent::Text(text) => {
                    summary.push_str(&text);
                    event_stream.send_context_compaction_update(compaction_id.clone(), &text);
                }
                LanguageModelCompletionEvent::UsageUpdate(usage) => {
                    this.update(cx, |this, _cx| {
                        this.accumulate_token_usage(usage);
                    })?;
                }
                LanguageModelCompletionEvent::Stop(_)
                | LanguageModelCompletionEvent::Started
                | LanguageModelCompletionEvent::Queued { .. }
                | LanguageModelCompletionEvent::Thinking { .. }
                | LanguageModelCompletionEvent::RedactedThinking { .. }
                | LanguageModelCompletionEvent::ReasoningDetails(_)
                | LanguageModelCompletionEvent::ToolUse(_)
                | LanguageModelCompletionEvent::ToolUseJsonParseError { .. }
                | LanguageModelCompletionEvent::StartMessage { .. }
                | LanguageModelCompletionEvent::Compaction(_) => {}
            }
        }

        if *cancellation_rx.borrow() {
            log::debug!("Compaction cancelled after summarizing");
            return Ok(ControlFlow::Break(()));
        }

        let summary = summary.trim().to_string();
        if summary.is_empty() {
            log::warn!("Compaction produced an empty summary");
            return Err(anyhow::anyhow!("Compaction produced an empty summary"));
        }

        log::debug!("Compaction succeeded:\n{summary}");
        event_stream.update_context_compaction_status(
            compaction_id,
            acp_thread::ContextCompactionStatus::Completed,
        );

        this.update(cx, |this, cx| {
            let compaction = Arc::new(Message::Compaction(CompactionInfo::Summary(summary.into())));
            match insertion {
                CompactionInsertion::Auto { insertion_ix } => {
                    if insertion_ix <= this.messages.len() {
                        this.messages.insert(insertion_ix, compaction);
                    } else {
                        this.messages.push(compaction);
                    }
                }
                CompactionInsertion::Manual { marker_id } => {
                    this.messages.push(Arc::new(Message::User(UserMessage {
                        id: marker_id,
                        content: Arc::from([]),
                    })));
                    this.messages.push(compaction);
                }
            }
            cx.notify();
        })?;

        Ok(ControlFlow::Continue(()))
    }

    fn process_tool_result(
        this: &WeakEntity<Thread>,
        event_stream: &ThreadEventStream,
        cx: &mut AsyncApp,
        tool_result: LanguageModelToolResult,
    ) -> Result<(), anyhow::Error> {
        log::debug!("Tool finished {:?}", tool_result);

        event_stream.update_tool_call_fields(
            &tool_result.tool_use_id,
            acp::ToolCallUpdateFields::new()
                .status(if tool_result.is_error {
                    acp::ToolCallStatus::Failed
                } else {
                    acp::ToolCallStatus::Completed
                })
                .raw_output(tool_result.output.clone()),
            None,
        );
        this.update(cx, |this, _cx| {
            this.pending_message()
                .tool_results
                .insert(tool_result.tool_use_id.clone(), tool_result)
        })?;
        Ok(())
    }

    fn handle_completion_error(
        &mut self,
        error: LanguageModelCompletionError,
        attempt: u8,
        plan: Option<Plan>,
    ) -> Result<acp_thread::RetryStatus> {
        let Some(model) = self.model() else {
            return Err(anyhow!(error));
        };

        let auto_retry = if model.provider_id() == MAV_CLOUD_PROVIDER_ID {
            plan.is_some()
        } else {
            true
        };

        if !auto_retry {
            return Err(anyhow!(error));
        }

        let Some(strategy) = Self::retry_strategy_for(&error) else {
            return Err(anyhow!(error));
        };

        let max_attempts = match &strategy {
            RetryStrategy::ExponentialBackoff { max_attempts, .. } => *max_attempts,
            RetryStrategy::Fixed { max_attempts, .. } => *max_attempts,
        };

        if attempt > max_attempts {
            return Err(anyhow!(error));
        }

        let delay = match &strategy {
            RetryStrategy::ExponentialBackoff { initial_delay, .. } => {
                let delay_secs = initial_delay.as_secs() * 2u64.pow((attempt - 1) as u32);
                Duration::from_secs(delay_secs)
            }
            RetryStrategy::Fixed { delay, .. } => *delay,
        };
        log::debug!("Retry attempt {attempt} with delay {delay:?}");

        Ok(acp_thread::RetryStatus {
            last_error: error.to_string().into(),
            attempt: attempt as usize,
            max_attempts: max_attempts as usize,
            started_at: Instant::now(),
            duration: delay,
            meta: None,
        })
    }

    /// A helper method that's called on every streamed completion event.
    /// Returns an optional tool result task, which the main agentic loop will
    /// send back to the model when it resolves.
    fn handle_completion_event(
        &mut self,
        event: LanguageModelCompletionEvent,
        event_stream: &ThreadEventStream,
        cancellation_rx: watch::Receiver<bool>,
        cx: &mut Context<Self>,
    ) -> Result<Option<Task<LanguageModelToolResult>>> {
        log::trace!("Handling streamed completion event: {:?}", event);
        use LanguageModelCompletionEvent::*;

        match event {
            StartMessage { .. } => {
                self.flush_pending_message(cx);
                self.pending_message = Some(AgentMessage::default());
            }
            Text(new_text) => self.handle_text_event(new_text, event_stream),
            Thinking { text, signature } => {
                self.handle_thinking_event(text, signature, event_stream)
            }
            RedactedThinking { data } => self.handle_redacted_thinking_event(data),
            ReasoningDetails(details) => {
                let last_message = self.pending_message();
                // Store the last non-empty reasoning_details (overwrites earlier ones)
                // This ensures we keep the encrypted reasoning with signatures, not the early text reasoning
                if let serde_json::Value::Array(arr) = &details {
                    if !arr.is_empty() {
                        last_message.reasoning_details = Some(Arc::new(details));
                    }
                } else {
                    last_message.reasoning_details = Some(Arc::new(details));
                }
            }
            ToolUse(tool_use) => {
                return Ok(self.handle_tool_use_event(tool_use, event_stream, cancellation_rx, cx));
            }
            ToolUseJsonParseError {
                id,
                tool_name,
                raw_input,
                json_parse_error,
            } => {
                return Ok(self.handle_tool_use_json_parse_error_event(
                    id,
                    tool_name,
                    raw_input,
                    json_parse_error,
                    event_stream,
                    cancellation_rx,
                    cx,
                ));
            }
            UsageUpdate(usage) => {
                telemetry::event!(
                    "Agent Thread Completion Usage Updated",
                    thread_id = self.id.to_string(),
                    parent_thread_id = self.parent_thread_id().map(|id| id.to_string()),
                    prompt_id = self.prompt_id.to_string(),
                    model = self.model().map(|m| m.telemetry_id()),
                    model_provider = self.model().map(|m| m.provider_id().to_string()),
                    input_tokens = usage.input_tokens,
                    output_tokens = usage.output_tokens,
                    cache_creation_input_tokens = usage.cache_creation_input_tokens,
                    cache_read_input_tokens = usage.cache_read_input_tokens,
                );
                // A successful compaction defers its telemetry until the first
                // completion that follows it, so `tokens_after` reflects the
                // real post-compaction context size.
                if let Some(telemetry) = self.pending_compaction_telemetry.take() {
                    telemetry.emit("succeeded", None, Some(total_input_tokens(usage)));
                }
                self.update_token_usage(usage, cx);
            }
            Stop(StopReason::Refusal) => return Err(CompletionError::Refusal.into()),
            Stop(StopReason::MaxTokens) => return Err(CompletionError::MaxTokens.into()),
            Stop(StopReason::ToolUse | StopReason::EndTurn) => {}
            Started | Queued { .. } | Compaction(_) => {}
        }

        Ok(None)
    }

    fn handle_text_event(&mut self, new_text: String, event_stream: &ThreadEventStream) {
        event_stream.send_text(&new_text);

        let last_message = self.pending_message();
        if let Some(AgentMessageContent::Text(text)) = last_message.content.last_mut() {
            text.push_str(&new_text);
        } else {
            last_message
                .content
                .push(AgentMessageContent::Text(new_text));
        }
    }

    fn handle_thinking_event(
        &mut self,
        new_text: String,
        new_signature: Option<String>,
        event_stream: &ThreadEventStream,
    ) {
        event_stream.send_thinking(&new_text);

        let last_message = self.pending_message();
        if let Some(AgentMessageContent::Thinking { text, signature }) =
            last_message.content.last_mut()
        {
            text.push_str(&new_text);
            *signature = new_signature.or(signature.take());
        } else {
            last_message.content.push(AgentMessageContent::Thinking {
                text: new_text,
                signature: new_signature,
            });
        }
    }

    fn handle_redacted_thinking_event(&mut self, data: String) {
        let last_message = self.pending_message();
        last_message
            .content
            .push(AgentMessageContent::RedactedThinking(data));
    }

    fn handle_tool_use_event(
        &mut self,
        tool_use: LanguageModelToolUse,
        event_stream: &ThreadEventStream,
        cancellation_rx: watch::Receiver<bool>,
        cx: &mut Context<Self>,
    ) -> Option<Task<LanguageModelToolResult>> {
        cx.notify();

        let tool = self.tool(tool_use.name.as_ref());
        let mut title = SharedString::from(&tool_use.name);
        let mut kind = acp::ToolKind::Other;
        if let Some(tool) = tool.as_ref() {
            title = tool.initial_title(tool_use.input.clone(), cx);
            kind = tool.kind();
        }

        self.send_or_update_tool_use(&tool_use, title, kind, event_stream);

        let Some(tool) = tool else {
            let content = format!("No tool named {} exists", tool_use.name);
            return Some(Task::ready(LanguageModelToolResult {
                content: vec![LanguageModelToolResultContent::Text(Arc::from(content))],
                tool_use_id: tool_use.id,
                tool_name: tool_use.name,
                is_error: true,
                output: None,
            }));
        };

        if !tool_use.is_input_complete {
            if tool.supports_input_streaming() {
                let running_turn = self.running_turn.as_mut()?;
                if let Some(sender) = running_turn.streaming_tool_inputs.get_mut(&tool_use.id) {
                    sender.send_partial(tool_use.input);
                    return None;
                }

                let (mut sender, tool_input) = ToolInputSender::channel();
                sender.send_partial(tool_use.input);
                running_turn
                    .streaming_tool_inputs
                    .insert(tool_use.id.clone(), sender);

                let tool = tool.clone();
                log::debug!("Running streaming tool {}", tool_use.name);
                return Some(self.run_tool(
                    tool,
                    tool_input,
                    tool_use.id,
                    tool_use.name,
                    event_stream,
                    cancellation_rx,
                    cx,
                ));
            } else {
                return None;
            }
        }

        if let Some(mut sender) = self
            .running_turn
            .as_mut()?
            .streaming_tool_inputs
            .remove(&tool_use.id)
        {
            sender.send_full(tool_use.input);
            return None;
        }

        log::debug!("Running tool {}", tool_use.name);
        let tool_input = ToolInput::ready(tool_use.input);
        Some(self.run_tool(
            tool,
            tool_input,
            tool_use.id,
            tool_use.name,
            event_stream,
            cancellation_rx,
            cx,
        ))
    }

    fn run_tool(
        &self,
        tool: Arc<dyn AnyAgentTool>,
        tool_input: ToolInput<serde_json::Value>,
        tool_use_id: LanguageModelToolUseId,
        tool_name: Arc<str>,
        event_stream: &ThreadEventStream,
        cancellation_rx: watch::Receiver<bool>,
        cx: &mut Context<Self>,
    ) -> Task<LanguageModelToolResult> {
        let fs = self.project.read(cx).fs().clone();
        let tool_event_stream = ToolCallEventStream::new(
            tool_use_id.clone(),
            event_stream.clone(),
            Some(fs),
            cancellation_rx,
            self.sandbox_grants.clone(),
            Some(cx.weak_entity()),
        );
        tool_event_stream.update_fields(
            acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::InProgress),
        );
        let supports_images = self.model().is_some_and(|model| model.supports_images());
        let tool_result = tool.run(tool_input, tool_event_stream, cx);
        cx.foreground_executor().spawn(async move {
            let (is_error, output) = match tool_result.await {
                Ok(mut output) => {
                    let contains_image = output
                        .llm_output
                        .iter()
                        .any(|part| matches!(part, LanguageModelToolResultContent::Image(_)));
                    if contains_image && !supports_images {
                        // Replace each image part with an inline placeholder so
                        // any accompanying text is still presented to the model.
                        // If there's nothing else in the output, surface an error
                        // to match the pre-multi-part behavior for image-only
                        // tool results.
                        let placeholder = LanguageModelToolResultContent::Text(Arc::from(
                            "[Tool responded with an image, but this model doesn't support images]",
                        ));
                        let has_non_image = output
                            .llm_output
                            .iter()
                            .any(|part| !matches!(part, LanguageModelToolResultContent::Image(_)));
                        if has_non_image {
                            output.llm_output = output
                                .llm_output
                                .into_iter()
                                .map(|part| match part {
                                    LanguageModelToolResultContent::Image(_) => placeholder.clone(),
                                    other => other,
                                })
                                .collect();
                            (false, output)
                        } else {
                            let output = anyhow::anyhow!(
                                "Attempted to read an image, but this model doesn't support it.",
                            )
                            .into();
                            (true, output)
                        }
                    } else {
                        (false, output)
                    }
                }
                Err(output) => (true, output),
            };

            LanguageModelToolResult {
                tool_use_id,
                tool_name,
                is_error,
                content: output.llm_output,
                output: Some(output.raw_output),
            }
        })
    }

    fn handle_tool_use_json_parse_error_event(
        &mut self,
        tool_use_id: LanguageModelToolUseId,
        tool_name: Arc<str>,
        raw_input: Arc<str>,
        json_parse_error: String,
        event_stream: &ThreadEventStream,
        cancellation_rx: watch::Receiver<bool>,
        cx: &mut Context<Self>,
    ) -> Option<Task<LanguageModelToolResult>> {
        let tool_use = LanguageModelToolUse {
            id: tool_use_id,
            name: tool_name,
            raw_input: raw_input.to_string(),
            input: serde_json::json!({}),
            is_input_complete: true,
            thought_signature: None,
        };
        self.send_or_update_tool_use(
            &tool_use,
            SharedString::from(&tool_use.name),
            acp::ToolKind::Other,
            event_stream,
        );

        let tool = self.tool(tool_use.name.as_ref());

        let Some(tool) = tool else {
            let content = format!("No tool named {} exists", tool_use.name);
            return Some(Task::ready(LanguageModelToolResult {
                content: vec![LanguageModelToolResultContent::Text(Arc::from(content))],
                tool_use_id: tool_use.id,
                tool_name: tool_use.name,
                is_error: true,
                output: None,
            }));
        };

        let error_message = format!("Error parsing input JSON: {json_parse_error}");

        if tool.supports_input_streaming()
            && let Some(mut sender) = self
                .running_turn
                .as_mut()?
                .streaming_tool_inputs
                .remove(&tool_use.id)
        {
            sender.send_invalid_json(error_message);
            return None;
        }

        log::debug!("Running tool {}. Received invalid JSON", tool_use.name);
        let tool_input = ToolInput::invalid_json(error_message);
        Some(self.run_tool(
            tool,
            tool_input,
            tool_use.id,
            tool_use.name,
            event_stream,
            cancellation_rx,
            cx,
        ))
    }

    fn send_or_update_tool_use(
        &mut self,
        tool_use: &LanguageModelToolUse,
        title: SharedString,
        kind: acp::ToolKind,
        event_stream: &ThreadEventStream,
    ) {
        // Ensure the last message ends in the current tool use
        let last_message = self.pending_message();

        let has_tool_use = last_message.content.iter_mut().rev().any(|content| {
            if let AgentMessageContent::ToolUse(last_tool_use) = content {
                if last_tool_use.id == tool_use.id {
                    *last_tool_use = tool_use.clone();
                    return true;
                }
            }
            false
        });

        if !has_tool_use {
            event_stream.send_tool_call(
                &tool_use.id,
                &tool_use.name,
                title,
                kind,
                tool_use.input.clone(),
            );
            last_message
                .content
                .push(AgentMessageContent::ToolUse(tool_use.clone()));
        } else {
            event_stream.update_tool_call_fields(
                &tool_use.id,
                acp::ToolCallUpdateFields::new()
                    .title(title.as_str())
                    .kind(kind)
                    .raw_input(tool_use.input.clone()),
                None,
            );
        }
    }

    pub fn title(&self) -> Option<SharedString> {
        self.title.clone()
    }

    pub fn is_generating_summary(&self) -> bool {
        self.pending_summary_generation.is_some()
    }

    pub fn is_generating_title(&self) -> bool {
        self.pending_title_generation.is_some()
    }

    pub fn has_failed_title_generation(&self) -> bool {
        self.title_generation_failed
    }

    pub fn can_generate_title(&self) -> bool {
        self.pending_title_generation.is_none() && self.summarization_model.is_some()
    }

    pub fn summary(&mut self, cx: &mut Context<Self>) -> Shared<Task<Option<SharedString>>> {
        if let Some(summary) = self.summary.as_ref() {
            return Task::ready(Some(summary.clone())).shared();
        }
        if let Some(task) = self.pending_summary_generation.clone() {
            return task;
        }
        let Some(model) = self.summarization_model.clone() else {
            log::error!("No summarization model available");
            return Task::ready(None).shared();
        };
        let mut request = LanguageModelRequest {
            intent: Some(CompletionIntent::ThreadContextSummarization),
            temperature: AgentSettings::temperature_for_model(&model, cx),
            ..Default::default()
        };

        self.extend_request_history_until(&mut request.messages, self.messages.len());

        request.messages.push(LanguageModelRequestMessage {
            role: Role::User,
            content: vec![SUMMARIZE_THREAD_DETAILED_PROMPT.into()],
            cache: false,
            reasoning_details: None,
        });

        let task = cx
            .spawn(async move |this, cx| {
                let mut summary = String::new();
                let mut messages = model.stream_completion(request, cx).await.log_err()?;
                while let Some(event) = messages.next().await {
                    let event = event.log_err()?;
                    let text = match event {
                        LanguageModelCompletionEvent::Text(text) => text,
                        _ => continue,
                    };

                    let mut lines = text.lines();
                    summary.extend(lines.next());
                }

                log::debug!("Setting summary: {}", summary);
                let summary = SharedString::from(summary);

                this.update(cx, |this, cx| {
                    this.summary = Some(summary.clone());
                    this.pending_summary_generation = None;
                    cx.notify()
                })
                .ok()?;

                Some(summary)
            })
            .shared();
        self.pending_summary_generation = Some(task.clone());
        task
    }

    pub fn generate_title(&mut self, cx: &mut Context<Self>) {
        if !self.can_generate_title() {
            return;
        }
        let Some(model) = self.summarization_model.clone() else {
            return;
        };
        self.spawn_title_generation(model, None, cx);
    }

    pub fn regenerate_title(&mut self, cx: &mut Context<Self>) -> bool {
        self.regenerate_title_with_callback(cx, |_title, _cx| {})
    }

    pub fn regenerate_title_with_callback(
        &mut self,
        cx: &mut Context<Self>,
        on_generated_title: impl FnOnce(SharedString, &mut Context<Self>) + 'static,
    ) -> bool {
        if self.pending_title_generation.is_some() {
            return false;
        }

        let Some(model) = self.summarization_model.clone() else {
            return false;
        };

        self.spawn_title_generation(model, Some(Box::new(on_generated_title)), cx);

        true
    }

    fn spawn_title_generation(
        &mut self,
        model: Arc<dyn LanguageModel>,
        on_generated_title: Option<Box<dyn FnOnce(SharedString, &mut Context<Self>)>>,
        cx: &mut Context<Self>,
    ) {
        self.title_generation_failed = false;
        log::debug!("Generating title with model: {:?}", model.name());

        let temperature = AgentSettings::temperature_for_model(&model, cx);
        let request = build_thread_title_request(&self.messages, temperature);

        let title_generation = cx.spawn(async move |_this, cx| {
            stream_thread_title(model, request, cx)
                .await
                .context("failed to generate thread title")
                .map(SharedString::from)
                .log_err()
        });

        self.pending_title_generation = Some(cx.spawn(async move |this, cx| {
            let title = title_generation.await;
            _ = this.update(cx, |this, cx| {
                this.pending_title_generation = None;
                if let Some(title) = title {
                    this.set_title(title.clone(), cx);
                    if let Some(on_generated_title) = on_generated_title {
                        on_generated_title(title, cx);
                    }
                } else {
                    this.title_generation_failed = true;
                    cx.emit(TitleUpdated);
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub fn set_title(&mut self, title: SharedString, cx: &mut Context<Self>) {
        self.pending_title_generation = None;
        self.title_generation_failed = false;
        if Some(&title) != self.title.as_ref() {
            self.title = Some(title);
            cx.emit(TitleUpdated);
            cx.notify();
        }
    }

    fn clear_summary(&mut self) {
        self.summary = None;
        self.pending_summary_generation = None;
    }

    fn last_user_message(&self) -> Option<&UserMessage> {
        self.messages
            .iter()
            .rev()
            .find_map(|message| match &**message {
                Message::User(user_message) => Some(user_message),
                Message::Agent(_) | Message::Resume | Message::Compaction(_) => None,
            })
    }

    fn pending_message(&mut self) -> &mut AgentMessage {
        self.pending_message.get_or_insert_default()
    }

    fn flush_pending_message(&mut self, cx: &mut Context<Self>) {
        let Some(mut message) = self.pending_message.take() else {
            return;
        };

        if message.content.is_empty() {
            return;
        }

        for content in &message.content {
            let AgentMessageContent::ToolUse(tool_use) = content else {
                continue;
            };

            if !message.tool_results.contains_key(&tool_use.id) {
                message.tool_results.insert(
                    tool_use.id.clone(),
                    LanguageModelToolResult {
                        tool_use_id: tool_use.id.clone(),
                        tool_name: tool_use.name.clone(),
                        is_error: true,
                        content: vec![LanguageModelToolResultContent::Text(
                            TOOL_CANCELED_MESSAGE.into(),
                        )],
                        output: None,
                    },
                );
            }
        }

        self.messages.push(Arc::new(Message::Agent(message)));
        self.updated_at = Utc::now();
        self.clear_summary();
        cx.notify()
    }

    pub(crate) fn build_completion_request(
        &self,
        completion_intent: CompletionIntent,
        cx: &App,
    ) -> Result<LanguageModelRequest> {
        let completion_intent =
            if self.is_subagent() && completion_intent == CompletionIntent::UserPrompt {
                CompletionIntent::Subagent
            } else {
                completion_intent
            };

        let model = self
            .model()
            .ok_or_else(|| anyhow!(NoModelConfiguredError))?;
        let tools = if let Some(turn) = self.running_turn.as_ref() {
            turn.tools
                .iter()
                .filter_map(|(tool_name, tool)| {
                    log::trace!("Including tool: {}", tool_name);
                    Some(LanguageModelRequestTool {
                        name: tool_name.to_string(),
                        description: tool.description().to_string(),
                        input_schema: tool.input_schema(model.tool_input_format()).log_err()?,
                        use_input_streaming: tool.supports_input_streaming(),
                    })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        log::debug!("Building completion request");
        log::debug!("Completion intent: {:?}", completion_intent);

        let available_tools: Vec<_> = self
            .running_turn
            .as_ref()
            .map(|turn| turn.tools.keys().cloned().collect())
            .unwrap_or_default();

        log::debug!("Request includes {} tools", available_tools.len());
        let messages = self.build_request_messages(available_tools, cx);
        log::debug!("Request will include {} messages", messages.len());

        let request = LanguageModelRequest {
            thread_id: Some(self.id.to_string()),
            prompt_id: Some(self.prompt_id.to_string()),
            intent: Some(completion_intent),
            messages,
            tools,
            tool_choice: None,
            stop: Vec::new(),
            temperature: AgentSettings::temperature_for_model(model, cx),
            // Models that can't run with thinking disabled ignore the
            // toggle state, which may be stale from a previously selected
            // model that could.
            thinking_allowed: self.thinking_enabled || !model.supports_disabling_thinking(),
            thinking_effort: self.thinking_effort.clone(),
            speed: self.speed(),
            compact_at_tokens: None,
        };

        log::debug!("Completion request built successfully");
        Ok(request)
    }

    fn enabled_tools(&self, cx: &App) -> BTreeMap<SharedString, Arc<dyn AnyAgentTool>> {
        let Some(model) = self.model() else {
            return BTreeMap::new();
        };
        let Some(profile) = AgentSettings::get_global(cx).profiles.get(&self.profile_id) else {
            return BTreeMap::new();
        };
        fn truncate(tool_name: &SharedString) -> SharedString {
            if tool_name.len() > MAX_TOOL_NAME_LENGTH {
                let mut truncated = tool_name.to_string();
                truncated.truncate(MAX_TOOL_NAME_LENGTH);
                truncated.into()
            } else {
                tool_name.clone()
            }
        }

        // Terminal variants are configured by users under the canonical
        // `terminal` name. Expose the one matching the current sandbox state
        // to the model under that name.
        let use_sandboxed_terminal = sandboxing_enabled_for_project(self.project.read(cx), cx);

        let mut tools = self
            .tools
            .iter()
            .filter_map(|(tool_name, tool)| {
                let terminal_variant = matches!(
                    tool_name.as_ref(),
                    TerminalTool::NAME | SandboxedTerminalTool::NAME
                );
                let profile_tool_name = if terminal_variant {
                    TerminalTool::NAME
                } else {
                    tool_name.as_ref()
                };

                if tool.supports_provider(&model.provider_id())
                    && profile.is_tool_enabled(profile_tool_name)
                {
                    match (tool_name.as_ref(), use_sandboxed_terminal) {
                        (TerminalTool::NAME, false) | (SandboxedTerminalTool::NAME, true) => {
                            Some((SharedString::from(TerminalTool::NAME), tool.clone()))
                        }
                        (TerminalTool::NAME | SandboxedTerminalTool::NAME, _) => None,
                        _ => Some((truncate(tool_name), tool.clone())),
                    }
                } else {
                    None
                }
            })
            .filter(|(tool_name, _)| crate::tools::tool_feature_flag_enabled(tool_name, cx))
            .collect::<BTreeMap<_, _>>();

        let mut context_server_tools = Vec::new();
        let mut seen_tools = tools.keys().cloned().collect::<HashSet<_>>();
        let mut duplicate_tool_names = HashSet::default();
        for (server_id, server_tools) in self.context_server_registry.read(cx).servers() {
            for (tool_name, tool) in server_tools {
                if profile.is_context_server_tool_enabled(&server_id.0, &tool_name) {
                    let tool_name = truncate(tool_name);
                    if !seen_tools.insert(tool_name.clone()) {
                        duplicate_tool_names.insert(tool_name.clone());
                    }
                    context_server_tools.push((server_id.clone(), tool_name, tool.clone()));
                }
            }
        }

        // When there are duplicate tool names, disambiguate by prefixing them
        // with the server ID (converted to snake_case for API compatibility).
        // In the rare case there isn't enough space for the disambiguated tool
        // name, keep only the last tool with this name.
        for (server_id, tool_name, tool) in context_server_tools {
            if duplicate_tool_names.contains(&tool_name) {
                let available = MAX_TOOL_NAME_LENGTH.saturating_sub(tool_name.len());
                if available >= 2 {
                    let mut disambiguated = server_id.0.to_snake_case();
                    disambiguated.truncate(available - 1);
                    disambiguated.push('_');
                    disambiguated.push_str(&tool_name);
                    tools.insert(disambiguated.into(), tool.clone());
                } else {
                    tools.insert(tool_name, tool.clone());
                }
            } else {
                tools.insert(tool_name, tool.clone());
            }
        }

        tools
    }

    fn refresh_turn_tools(&mut self, cx: &App) {
        let tools = self.enabled_tools(cx);
        if let Some(turn) = self.running_turn.as_mut() {
            turn.tools = tools;
        }
    }

    fn tool(&self, name: &str) -> Option<Arc<dyn AnyAgentTool>> {
        self.running_turn.as_ref()?.tools.get(name).cloned()
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.running_turn
            .as_ref()
            .is_some_and(|turn| turn.tools.contains_key(name))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn has_registered_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub(crate) fn register_running_subagent(&mut self, subagent: WeakEntity<Thread>) {
        self.running_subagents.push(subagent);
    }

    pub(crate) fn unregister_running_subagent(
        &mut self,
        subagent_session_id: &acp::SessionId,
        cx: &App,
    ) {
        self.running_subagents.retain(|s| {
            s.upgrade()
                .map_or(false, |s| s.read(cx).id() != subagent_session_id)
        });
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn running_subagent_ids(&self, cx: &App) -> Vec<acp::SessionId> {
        self.running_subagents
            .iter()
            .filter_map(|s| s.upgrade().map(|s| s.read(cx).id().clone()))
            .collect()
    }

    pub fn is_subagent(&self) -> bool {
        self.subagent_context.is_some()
    }

    pub fn parent_thread_id(&self) -> Option<acp::SessionId> {
        self.subagent_context
            .as_ref()
            .map(|c| c.parent_thread_id.clone())
    }

    pub fn depth(&self) -> u8 {
        self.subagent_context.as_ref().map(|c| c.depth).unwrap_or(0)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_subagent_context(&mut self, context: SubagentContext) {
        self.subagent_context = Some(context);
    }

    pub fn is_turn_complete(&self) -> bool {
        self.running_turn.is_none()
    }

    pub fn to_markdown(&self) -> String {
        let mut markdown = messages_to_markdown(&self.messages);

        if let Some(message) = self.pending_message.as_ref() {
            markdown.push_str("\n## Assistant\n\n");
            markdown.push_str(&message.to_markdown());
        }

        markdown
    }

    fn advance_prompt_id(&mut self) {
        self.prompt_id = PromptId::new();
    }

    fn retry_strategy_for(error: &LanguageModelCompletionError) -> Option<RetryStrategy> {
        use LanguageModelCompletionError::*;
        use http_client::StatusCode;

        // General strategy here:
        // - If retrying won't help (e.g. invalid API key or payload too large), return None so we don't retry at all.
        // - If it's a time-based issue (e.g. server overloaded, rate limit exceeded), retry up to 4 times with exponential backoff.
        // - If it's an issue that *might* be fixed by retrying (e.g. internal server error), retry up to 3 times.
        match error {
            HttpResponseError {
                status_code: StatusCode::TOO_MANY_REQUESTS,
                ..
            } => Some(RetryStrategy::ExponentialBackoff {
                initial_delay: BASE_RETRY_DELAY,
                max_attempts: MAX_RETRY_ATTEMPTS,
            }),
            ServerOverloaded { retry_after, .. } | RateLimitExceeded { retry_after, .. } => {
                Some(RetryStrategy::Fixed {
                    delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                    max_attempts: MAX_RETRY_ATTEMPTS,
                })
            }
            UpstreamProviderError {
                status,
                retry_after,
                ..
            } => match *status {
                StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => {
                    Some(RetryStrategy::Fixed {
                        delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                        max_attempts: MAX_RETRY_ATTEMPTS,
                    })
                }
                StatusCode::INTERNAL_SERVER_ERROR => Some(RetryStrategy::Fixed {
                    delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                    // Internal Server Error could be anything, retry up to 3 times.
                    max_attempts: 3,
                }),
                status => {
                    // There is no StatusCode variant for the unofficial HTTP 529 ("The service is overloaded"),
                    // but we frequently get them in practice. See https://http.dev/529
                    if status.as_u16() == 529 {
                        Some(RetryStrategy::Fixed {
                            delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                            max_attempts: MAX_RETRY_ATTEMPTS,
                        })
                    } else {
                        Some(RetryStrategy::Fixed {
                            delay: retry_after.unwrap_or(BASE_RETRY_DELAY),
                            max_attempts: 2,
                        })
                    }
                }
            },
            ApiInternalServerError { .. } => Some(RetryStrategy::Fixed {
                delay: BASE_RETRY_DELAY,
                max_attempts: 3,
            }),
            ApiReadResponseError { .. }
            | HttpSend { .. }
            | DeserializeResponse { .. }
            | BadRequestFormat { .. } => Some(RetryStrategy::Fixed {
                delay: BASE_RETRY_DELAY,
                max_attempts: 3,
            }),
            // Retrying these errors definitely shouldn't help.
            HttpResponseError {
                status_code:
                    StatusCode::PAYLOAD_TOO_LARGE | StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED,
                ..
            }
            | AuthenticationError { .. }
            | PermissionError { .. }
            | NoApiKey { .. }
            | ApiEndpointNotFound { .. }
            | PromptTooLarge { .. } => None,
            // These errors might be transient, so retry them
            SerializeRequest { .. } | BuildRequestBody { .. } | StreamEndedUnexpectedly { .. } => {
                Some(RetryStrategy::Fixed {
                    delay: BASE_RETRY_DELAY,
                    max_attempts: 1,
                })
            }
            // Retry all other 4xx and 5xx errors once.
            HttpResponseError { status_code, .. }
                if status_code.is_client_error() || status_code.is_server_error() =>
            {
                Some(RetryStrategy::Fixed {
                    delay: BASE_RETRY_DELAY,
                    max_attempts: 3,
                })
            }
            // Retrying won't help for Payment Required errors.
            PaymentRequired => None,
            // Retrying won't help until the user consents to data retention
            // or switches models.
            DataRetentionConsentRequired { .. } => None,
            // Conservatively assume that any other errors are non-retryable
            HttpResponseError { .. } | Other(..) => Some(RetryStrategy::Fixed {
                delay: BASE_RETRY_DELAY,
                max_attempts: 2,
            }),
        }
    }
}

pub fn build_thread_title_request(
    messages: &[Arc<Message>],
    temperature: Option<f32>,
) -> LanguageModelRequest {
    let mut request = LanguageModelRequest {
        intent: Some(CompletionIntent::ThreadSummarization),
        temperature,
        ..Default::default()
    };
    extend_request_history_until(messages, &mut request.messages, messages.len());
    request.messages.push(LanguageModelRequestMessage {
        role: Role::User,
        content: vec![SUMMARIZE_THREAD_PROMPT.into()],
        cache: false,
        reasoning_details: None,
    });
    request
}

pub async fn stream_thread_title(
    model: Arc<dyn LanguageModel>,
    request: LanguageModelRequest,
    cx: &AsyncApp,
) -> Result<String> {
    let mut title = String::new();
    let mut events = model.stream_completion(request, cx).await?;
    while let Some(event) = events.next().await {
        let LanguageModelCompletionEvent::Text(text) = event? else {
            continue;
        };
        if let Some(newline_ix) = text.find(|ch| ch == '\n' || ch == '\r') {
            title.push_str(&text[..newline_ix]);
            break;
        }
        title.push_str(&text);
    }
    Ok(title)
}

pub struct TokenUsageUpdated(pub Option<acp_thread::TokenUsage>);

impl EventEmitter<TokenUsageUpdated> for Thread {}

pub struct TitleUpdated;

impl EventEmitter<TitleUpdated> for Thread {}

#[derive(Clone)]
struct ThreadEventStream(mpsc::UnboundedSender<Result<ThreadEvent>>);

impl ThreadEventStream {
    fn send_user_message(&self, message: &UserMessage) {
        self.0
            .unbounded_send(Ok(ThreadEvent::UserMessage(message.clone())))
            .ok();
    }

    fn send_text(&self, text: &str) {
        self.0
            .unbounded_send(Ok(ThreadEvent::AgentText(text.to_string())))
            .ok();
    }

    fn send_thinking(&self, text: &str) {
        self.0
            .unbounded_send(Ok(ThreadEvent::AgentThinking(text.to_string())))
            .ok();
    }

    fn send_tool_call(
        &self,
        id: &LanguageModelToolUseId,
        tool_name: &str,
        title: SharedString,
        kind: acp::ToolKind,
        input: serde_json::Value,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ToolCall(Self::initial_tool_call(
                id,
                tool_name,
                title.to_string(),
                kind,
                input,
            ))))
            .ok();
    }

    fn initial_tool_call(
        id: &LanguageModelToolUseId,
        tool_name: &str,
        title: String,
        kind: acp::ToolKind,
        input: serde_json::Value,
    ) -> acp::ToolCall {
        acp::ToolCall::new(id.to_string(), title)
            .kind(kind)
            .raw_input(input)
            .meta(acp_thread::meta_with_tool_name(tool_name))
    }

    fn update_tool_call_fields(
        &self,
        tool_use_id: &LanguageModelToolUseId,
        fields: acp::ToolCallUpdateFields,
        meta: Option<acp::Meta>,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ToolCallUpdate(
                acp::ToolCallUpdate::new(tool_use_id.to_string(), fields)
                    .meta(meta)
                    .into(),
            )))
            .ok();
    }

    fn resolve_tool_call_authorization(
        &self,
        tool_use_id: &LanguageModelToolUseId,
        outcome: acp_thread::SelectedPermissionOutcome,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ToolCallAuthorizationResolved {
                tool_call_id: acp::ToolCallId::new(tool_use_id.to_string()),
                outcome,
            }))
            .ok();
    }

    fn send_retry(&self, status: acp_thread::RetryStatus) {
        self.0.unbounded_send(Ok(ThreadEvent::Retry(status))).ok();
    }

    fn send_context_compaction(
        &self,
        id: acp_thread::ContextCompactionId,
        status: acp_thread::ContextCompactionStatus,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ContextCompaction(
                acp_thread::ContextCompaction {
                    id,
                    status,
                    summary: None,
                },
            )))
            .ok();
    }

    fn send_context_compaction_update(
        &self,
        id: acp_thread::ContextCompactionId,
        summary_delta: &str,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ContextCompactionUpdate(
                acp_thread::ContextCompactionUpdate {
                    id,
                    summary_delta: summary_delta.to_string(),
                    status: None,
                },
            )))
            .ok();
    }

    fn update_context_compaction_status(
        &self,
        id: acp_thread::ContextCompactionId,
        status: acp_thread::ContextCompactionStatus,
    ) {
        self.0
            .unbounded_send(Ok(ThreadEvent::ContextCompactionUpdate(
                acp_thread::ContextCompactionUpdate {
                    id,
                    summary_delta: String::new(),
                    status: Some(status),
                },
            )))
            .ok();
    }

    fn send_stop(&self, reason: acp::StopReason) {
        self.0.unbounded_send(Ok(ThreadEvent::Stop(reason))).ok();
    }

    fn send_canceled(&self) {
        self.0
            .unbounded_send(Ok(ThreadEvent::Stop(acp::StopReason::Cancelled)))
            .ok();
    }

    fn send_error(&self, error: impl Into<anyhow::Error>) {
        self.0.unbounded_send(Err(error.into())).ok();
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

impl ToolCallEventStream {
    #[cfg(any(test, feature = "test-support"))]
    pub fn test() -> (Self, ToolCallEventStreamReceiver) {
        let (stream, receiver, _cancellation_tx) = Self::test_with_cancellation();
        (stream, receiver)
    }

    /// Like [`Self::test`], but the returned stream shares the provided
    /// thread-scoped sandbox grants. This mirrors how a real [`Thread`] builds a
    /// distinct event stream per tool call while sharing one set of grants, so
    /// tests can exercise sequences of tool calls within the same conversation.
    #[cfg(test)]
    pub(crate) fn test_with_grants(
        sandbox_grants: Rc<RefCell<ThreadSandboxGrants>>,
    ) -> (Self, ToolCallEventStreamReceiver) {
        let (events_tx, events_rx) = mpsc::unbounded::<Result<ThreadEvent>>();
        let (_cancellation_tx, cancellation_rx) = watch::channel(false);

        let stream = ToolCallEventStream::new(
            "test_id".into(),
            ThreadEventStream(events_tx),
            None,
            cancellation_rx,
            sandbox_grants,
            None,
        );

        (stream, ToolCallEventStreamReceiver(events_rx))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_with_cancellation() -> (Self, ToolCallEventStreamReceiver, watch::Sender<bool>) {
        let (events_tx, events_rx) = mpsc::unbounded::<Result<ThreadEvent>>();
        let (cancellation_tx, cancellation_rx) = watch::channel(false);

        let stream = ToolCallEventStream::new(
            "test_id".into(),
            ThreadEventStream(events_tx),
            None,
            cancellation_rx,
            Rc::new(RefCell::new(ThreadSandboxGrants::default())),
            None,
        );

        (
            stream,
            ToolCallEventStreamReceiver(events_rx),
            cancellation_tx,
        )
    }

    /// Signal cancellation for this event stream. Only available in tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn signal_cancellation_with_sender(cancellation_tx: &mut watch::Sender<bool>) {
        cancellation_tx.send(true).ok();
    }

    fn new(
        tool_use_id: LanguageModelToolUseId,
        stream: ThreadEventStream,
        fs: Option<Arc<dyn Fs>>,
        cancellation_rx: watch::Receiver<bool>,
        sandbox_grants: Rc<RefCell<ThreadSandboxGrants>>,
        thread: Option<WeakEntity<Thread>>,
    ) -> Self {
        Self {
            tool_use_id,
            stream,
            fs,
            cancellation_rx,
            sandbox_grants,
            thread,
        }
    }

    /// Whether the owning thread is a subagent, so prompts can say "for this
    /// subagent" instead of "for this thread".
    fn is_subagent(&self, cx: &App) -> bool {
        self.thread
            .as_ref()
            .and_then(|thread| thread.upgrade())
            .is_some_and(|thread| thread.read(cx).is_subagent())
    }

    /// Persist the thread so a freshly recorded "for this thread" sandbox grant
    /// survives a reopen. Saving is driven by the agent's `observe` on the
    /// thread entity, so a no-op `notify` is enough to schedule it.
    fn persist_thread_grants(thread: &Option<WeakEntity<Thread>>, cx: &AsyncApp) {
        let Some(thread) = thread else { return };
        cx.update(|cx| {
            thread.update(cx, |_thread, cx| cx.notify()).ok();
        });
    }

    /// Returns a future that resolves when the user cancels the tool call.
    /// Tools should select on this alongside their main work to detect user cancellation.
    pub fn cancelled_by_user(&self) -> impl std::future::Future<Output = ()> + '_ {
        let mut rx = self.cancellation_rx.clone();
        async move {
            loop {
                if *rx.borrow() {
                    return;
                }
                if rx.changed().await.is_err() {
                    // Sender dropped, will never be cancelled
                    std::future::pending::<()>().await;
                }
            }
        }
    }

    /// Returns true if the user has cancelled this tool call.
    /// This is useful for checking cancellation state after an operation completes,
    /// to determine if the completion was due to user cancellation.
    pub fn was_cancelled_by_user(&self) -> bool {
        *self.cancellation_rx.clone().borrow()
    }

    pub fn tool_use_id(&self) -> &LanguageModelToolUseId {
        &self.tool_use_id
    }

    pub fn update_fields(&self, fields: acp::ToolCallUpdateFields) {
        self.stream
            .update_tool_call_fields(&self.tool_use_id, fields, None);
    }

    pub fn update_fields_with_meta(
        &self,
        fields: acp::ToolCallUpdateFields,
        meta: Option<acp::Meta>,
    ) {
        self.stream
            .update_tool_call_fields(&self.tool_use_id, fields, meta);
    }

    pub fn resolve_authorization(&self, outcome: acp_thread::SelectedPermissionOutcome) {
        self.stream
            .resolve_tool_call_authorization(&self.tool_use_id, outcome);
    }

    pub fn update_diff(&self, diff: Entity<acp_thread::Diff>) {
        self.stream
            .0
            .unbounded_send(Ok(ThreadEvent::ToolCallUpdate(
                acp_thread::ToolCallUpdateDiff {
                    id: acp::ToolCallId::new(self.tool_use_id.to_string()),
                    diff,
                }
                .into(),
            )))
            .ok();
    }

    pub fn subagent_spawned(&self, id: acp::SessionId) {
        self.stream
            .0
            .unbounded_send(Ok(ThreadEvent::SubagentSpawned(id)))
            .ok();
    }

    /// Authorize a third-party tool (e.g., MCP tool from a context server).
    ///
    /// Unlike built-in tools, third-party tools don't support pattern-based permissions.
    /// They only support `default` (allow/deny/confirm) per tool.
    ///
    /// Uses the dropdown authorization flow with two granularities:
    /// - "Always for <display_name> MCP tool" → sets `tools.<tool_id>.default = "allow"` or "deny"
    /// - "Only this time" → allow/deny once
    pub fn authorize_third_party_tool(
        &self,
        title: impl Into<String>,
        tool_id: String,
        display_name: String,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = acp_thread::PermissionOptions::Dropdown(vec![
            acp_thread::PermissionOptionChoice {
                allow: acp::PermissionOption::new(
                    acp::PermissionOptionId::new(format!("always_allow_mcp:{tool_id}")),
                    format!("Always for {display_name} MCP tool"),
                    acp::PermissionOptionKind::AllowAlways,
                ),
                deny: acp::PermissionOption::new(
                    acp::PermissionOptionId::new(format!("always_deny_mcp:{tool_id}")),
                    format!("Always for {display_name} MCP tool"),
                    acp::PermissionOptionKind::RejectAlways,
                ),
                sub_patterns: vec![],
            },
            acp_thread::PermissionOptionChoice {
                allow: acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Only this time",
                    acp::PermissionOptionKind::AllowOnce,
                ),
                deny: acp::PermissionOption::new(
                    acp::PermissionOptionId::new("deny"),
                    "Only this time",
                    acp::PermissionOptionKind::RejectOnce,
                ),
                sub_patterns: vec![],
            },
        ]);

        // MCP tools are gated only by tool id (no per-input pattern
        // matching), so we pass a single empty input value just to satisfy
        // `decide_permission_from_settings`' signature.
        let check_settings: Box<dyn Fn(&App) -> ToolPermissionDecision> =
            Box::new(move |cx: &App| {
                let settings = agent_settings::AgentSettings::get_global(cx);
                decide_permission_from_settings(&tool_id, &[String::new()], settings)
            });

        self.run_authorization_loop(title, options, None, Some(check_settings), cx)
    }

    /// Gate a tool call on user permission, driven by the agent's
    /// tool-permission settings.
    ///
    /// Evaluates the current settings up-front: returns `Ok(())` immediately
    /// if the tool is already allowed, an error if it is denied, and
    /// otherwise prompts the user for a decision. While a prompt is pending,
    /// a subscription to `SettingsStore` watches for changes (for example,
    /// when the user clicks "Always for …" on a sibling tool call and the
    /// new rule becomes globally visible). When settings change, the current
    /// prompt is dismissed and the decision is re-evaluated. This closes the
    /// gap where an "Always for …" decision on one pending tool call would
    /// not propagate to other pending tool calls in the same turn or in
    /// subagent turns.
    ///
    /// For authorizations that must always prompt regardless of settings
    /// (e.g. symlink-escape confirmations, sensitive settings-file edits),
    /// use [`Self::prompt`] instead.
    pub fn authorize(
        &self,
        title: impl Into<String>,
        context: ToolPermissionContext,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = context.build_permission_options();

        let tool_name = context.tool_name.clone();
        let input_values = context.input_values.clone();
        let check_settings: Box<dyn Fn(&App) -> ToolPermissionDecision> =
            Box::new(move |cx: &App| {
                decide_permission_from_settings(
                    &tool_name,
                    &input_values,
                    agent_settings::AgentSettings::get_global(cx),
                )
            });

        self.run_authorization_loop(title, options, Some(context), Some(check_settings), cx)
    }

    /// Like [`Self::authorize`], but always prompts the user without
    /// consulting settings. Use this for authorizations that must be
    /// confirmed even when the user has configured `always_allow` rules —
    /// for example, symlink-escape confirmations or edits that target
    /// sensitive settings files.
    pub fn authorize_always_prompt(
        &self,
        title: impl Into<String>,
        context: ToolPermissionContext,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let title = title.into();
        let options = context.build_permission_options();
        self.run_authorization_loop(title, options, Some(context), None, cx)
    }

    /// Gate a sandbox *escalation* (network access, per-path writes, or full
    /// filesystem write access) on user approval.
    ///
    /// Offers the user three grant lifetimes — "once", "for the rest of this
    /// thread", and "always". Thread grants live in the shared, in-memory
    /// [`ThreadSandboxGrants`]. Always grants are persisted in agent settings
    /// and are also observed while a prompt is pending, matching the
    /// settings-driven authorization flow for regular tools.
    pub(crate) fn authorize_sandbox(
        &self,
        request: SandboxRequest,
        reason: String,
        cx: &mut App,
    ) -> Task<Result<()>> {
        if Self::sandbox_request_covered_by_grants(&request, &self.sandbox_grants, cx) {
            return Task::ready(Ok(()));
        }

        let (network_hosts, network_all_hosts) = match &request.network {
            crate::sandboxing::NetworkRequest::None => (Vec::new(), false),
            crate::sandboxing::NetworkRequest::AnyHost => (Vec::new(), true),
            crate::sandboxing::NetworkRequest::Hosts(hosts) => {
                (hosts.iter().map(|host| host.to_string()).collect(), false)
            }
        };
        let sandbox_authorization_details = acp_thread::SandboxAuthorizationDetails {
            // The command stays in the tool-call title (set by the terminal
            // tool), so the approval card keeps showing it; the details only
            // describe the requested access and the agent's reason.
            command: None,
            network_hosts,
            network_all_hosts,
            allow_git_access: request.allow_git_access,
            allow_fs_write_all: request.allow_fs_write_all,
            unsandboxed: request.unsandboxed,
            write_paths: request.write_paths.clone(),
            reason,
        };
        let allow_thread_label = if self.is_subagent(cx) {
            "Allow for this subagent"
        } else {
            "Allow for this thread"
        };
        let options = acp_thread::PermissionOptions::Flat(vec![
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowOnce.as_id()),
                "Allow once",
                acp::PermissionOptionKind::AllowOnce,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowThread.as_id()),
                allow_thread_label,
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowAlways.as_id()),
                "Allow always",
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::Deny.as_id()),
                "Deny",
                acp::PermissionOptionKind::RejectOnce,
            ),
        ]);

        let fs = self.fs.clone();
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        let sandbox_grants = self.sandbox_grants.clone();
        let thread = self.thread.clone();
        let auto_allow_outcome = match auto_resolve_permission_outcome(&options, true) {
            Ok(outcome) => outcome,
            Err(error) => return Task::ready(Err(error)),
        };
        cx.spawn(async move |cx| {
            let (response_tx, mut response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        tool_call: acp::ToolCallUpdate::new(
                            tool_use_id.to_string(),
                            // Leave the title untouched so the card keeps
                            // showing the command (matching the fallback flow).
                            acp::ToolCallUpdateFields::new(),
                        )
                        .meta(acp_thread::meta_with_sandbox_authorization(
                            sandbox_authorization_details,
                        )),
                        options,
                        response: response_tx,
                        context: None,
                        kind: acp_thread::AuthorizationKind::PermissionGrant,
                    },
                )))
            {
                log::error!("Failed to send sandbox authorization: {error}");
                return Err(anyhow!("Failed to send sandbox authorization: {error}"));
            }

            let (mut settings_tx, mut settings_rx) = watch::channel(());
            let _settings_subscription = cx.update(|cx| {
                cx.observe_global::<SettingsStore>(move |_cx| {
                    settings_tx.send(()).ok();
                })
            });

            loop {
                let settings_changed = async {
                    if settings_rx.changed().await.is_err() {
                        std::future::pending::<()>().await;
                    }
                };
                futures::select_biased! {
                    outcome = (&mut response_rx).fuse() => {
                        let outcome = outcome
                            .map_err(|_| anyhow!("authorization channel closed"))?;
                        return Self::handle_sandbox_permission_outcome(
                            &outcome,
                            &request,
                            sandbox_grants.clone(),
                            thread.clone(),
                            fs.clone(),
                            cx,
                        );
                    }
                    _ = settings_changed.fuse() => {
                        if cx.update(|cx| Self::sandbox_request_covered_by_grants(
                            &request,
                            &sandbox_grants,
                            cx,
                        )) {
                            drop(response_rx);
                            stream.resolve_tool_call_authorization(
                                &tool_use_id,
                                auto_allow_outcome.clone(),
                            );
                            return Ok(());
                        }
                    }
                }
            }
        })
    }

    fn sandbox_request_covered_by_grants(
        request: &SandboxRequest,
        sandbox_grants: &Rc<RefCell<ThreadSandboxGrants>>,
        cx: &App,
    ) -> bool {
        let settings = AgentSettings::get_global(cx);
        sandbox_grants
            .borrow()
            .covers_with_persistent(request, &settings.sandbox_permissions)
    }

    fn handle_sandbox_permission_outcome(
        outcome: &acp_thread::SelectedPermissionOutcome,
        request: &SandboxRequest,
        sandbox_grants: Rc<RefCell<ThreadSandboxGrants>>,
        thread: Option<WeakEntity<Thread>>,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) -> Result<()> {
        debug_assert!(
            outcome.params.is_none(),
            "unexpected params for sandbox permission"
        );

        match acp_thread::SandboxPermission::from_id(outcome.option_id.0.as_ref()) {
            Some(acp_thread::SandboxPermission::AllowOnce) => Ok(()),
            Some(acp_thread::SandboxPermission::AllowThread) => {
                sandbox_grants.borrow_mut().record(request);
                Self::persist_thread_grants(&thread, cx);
                Ok(())
            }
            Some(acp_thread::SandboxPermission::AllowAlways) => {
                Self::persist_sandbox_always_permission(request, fs, cx);
                Ok(())
            }
            Some(acp_thread::SandboxPermission::Deny) => {
                Err(anyhow!("Permission to run tool denied by user"))
            }
            None => {
                let other = outcome.option_id.0.as_ref();
                debug_assert!(false, "unexpected sandbox permission option_id: {other}");
                Err(anyhow!("Permission to run tool denied by user"))
            }
        }
    }

    fn persist_sandbox_always_permission(
        request: &SandboxRequest,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) {
        let Some(fs) = fs else {
            log::error!(
                "Cannot persist \"allow always\" sandbox permission: no filesystem available"
            );
            return;
        };

        let request = request.clone();
        cx.update(|cx| {
            update_settings_file(fs, cx, move |settings, _| {
                let agent = settings.agent.get_or_insert_default();
                match &request.network {
                    crate::sandboxing::NetworkRequest::None => {}
                    crate::sandboxing::NetworkRequest::AnyHost => {
                        agent.allow_sandbox_all_hosts();
                    }
                    crate::sandboxing::NetworkRequest::Hosts(hosts) => {
                        // Rebuild the persisted list with subsumption pruning
                        // so granting `*.github.com` retires a previously
                        // persisted `api.github.com` instead of accumulating
                        // redundant entries. Unparsable hand-edited entries
                        // are preserved untouched.
                        let mut patterns = Vec::new();
                        let mut unparsable = Vec::new();
                        for raw in agent.sandbox_network_hosts() {
                            match http_proxy::HostPattern::parse(raw) {
                                Ok(pattern) => {
                                    crate::sandboxing::insert_host_pattern(&mut patterns, pattern)
                                }
                                Err(_) => unparsable.push(raw.clone()),
                            }
                        }
                        for host in hosts {
                            crate::sandboxing::insert_host_pattern(&mut patterns, host.clone());
                        }
                        let mut host_strings = unparsable;
                        host_strings.extend(patterns.iter().map(|pattern| pattern.to_string()));
                        agent.set_sandbox_network_hosts(host_strings);
                    }
                }
                if request.allow_git_access {
                    agent.allow_sandbox_git_access();
                }
                if request.allow_fs_write_all {
                    agent.allow_sandbox_fs_write_all();
                }
                if request.unsandboxed {
                    agent.allow_sandbox_unsandboxed();
                }
                for path in request.write_paths {
                    agent.add_sandbox_write_path(path);
                }
            });
        });
    }

    /// The sandbox permissions to actually enforce for a command: the union
    /// of this command's `request`, everything granted "for the rest of the
    /// conversation", and persistent "allow always" sandbox grants.
    ///
    /// Callers must apply this to the enforced sandbox policy (rather than
    /// the raw `request`) so standing grants keep working for later commands
    /// that write to a previously approved path without re-requesting it.
    pub(crate) fn effective_sandbox_request(
        &self,
        request: &SandboxRequest,
        persistent: &agent_settings::SandboxPermissions,
    ) -> SandboxRequest {
        self.sandbox_grants
            .borrow()
            .effective_with_persistent(request, persistent)
    }

    /// Whether the user allowed running commands unsandboxed for the rest of
    /// the thread (distinct from the persistent `allow_unsandboxed` setting).
    pub(crate) fn sandbox_fallback_granted_for_thread(&self) -> bool {
        self.sandbox_grants.borrow().fallback_granted_for_thread()
    }

    /// Whether the user approved a model-requested `unsandboxed: true` escape
    /// for the rest of this thread. Like the fallback grant, this makes every
    /// command in the thread run without a sandbox.
    pub(crate) fn unsandboxed_granted_for_thread(&self) -> bool {
        self.sandbox_grants.borrow().unsandboxed_granted()
    }

    /// Ask the user how to proceed when the OS sandbox could not be created
    /// for a command (for example, `bwrap` is missing or user namespaces are
    /// disabled).
    ///
    /// Unlike [`Self::authorize_sandbox`] — which gates a model-requested
    /// *escalation* — this surfaces a *system limitation*: the sandbox failed,
    /// so the prompt explains why (`reason`) and lets the user retry, run the
    /// command unsandboxed (once / for this thread / always), or deny it. The
    /// "for this thread" choice is recorded in the in-memory thread grants and
    /// "always" is persisted as the `allow_unsandboxed` setting. Only the
    /// Bubblewrap sandboxes (Linux directly, Windows via WSL) can fail to
    /// create a sandbox, so this is gated to those platforms.
    ///
    /// `retries` is how many times the user has already pressed Retry for this
    /// command; it's shown on the button so repeated presses visibly advance
    /// ("Retry", then "Retry (attempt 1)", "Retry (attempt 2)", …).
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    pub(crate) fn authorize_sandbox_fallback(
        &self,
        command: Option<String>,
        reason: String,
        retries: usize,
        cx: &mut App,
    ) -> Task<Result<SandboxFallbackDecision>> {
        let details = acp_thread::SandboxFallbackAuthorizationDetails { command, reason };
        let retry_label = if retries == 0 {
            "Retry".to_string()
        } else {
            format!("Retry (attempt {retries})")
        };
        let allow_thread_label = if self.is_subagent(cx) {
            "Run without sandbox for this subagent"
        } else {
            "Run without sandbox for this thread"
        };
        let options = acp_thread::PermissionOptions::Flat(vec![
            // Retry isn't an allow/deny choice; the UI renders it with its own
            // icon and we dispatch on the option id, so the kind here only
            // governs keybindings. Use `RejectAlways` (which has none) so the
            // "allow once" shortcut maps to "Run without sandbox once" rather
            // than to Retry.
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SANDBOX_FALLBACK_RETRY_OPTION_ID),
                retry_label,
                acp::PermissionOptionKind::RejectAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowOnce.as_id()),
                "Run without sandbox once",
                acp::PermissionOptionKind::AllowOnce,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowThread.as_id()),
                allow_thread_label,
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowAlways.as_id()),
                "Always run without sandbox",
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                acp::PermissionOptionId::new(acp_thread::SandboxPermission::Deny.as_id()),
                "Deny",
                acp::PermissionOptionKind::RejectOnce,
            ),
        ]);

        let fs = self.fs.clone();
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        let sandbox_grants = self.sandbox_grants.clone();
        let thread = self.thread.clone();
        cx.spawn(async move |cx| {
            let (response_tx, response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        // Deliberately leave the tool-call title untouched so
                        // the card keeps showing the *command* (not the
                        // failure reason): it's critical the user can see what
                        // they're approving to run unsandboxed. The reason is
                        // surfaced separately by the fallback details / warning.
                        tool_call: acp::ToolCallUpdate::new(
                            tool_use_id.to_string(),
                            acp::ToolCallUpdateFields::new(),
                        )
                        .meta(
                            acp_thread::meta_with_sandbox_fallback_authorization(details),
                        ),
                        options,
                        response: response_tx,
                        context: None,
                        kind: acp_thread::AuthorizationKind::ActionChoice,
                    },
                )))
            {
                log::error!("Failed to send sandbox fallback authorization: {error}");
                return Err(anyhow!(
                    "Failed to send sandbox fallback authorization: {error}"
                ));
            }

            let outcome = response_rx
                .await
                .map_err(|_| anyhow!("authorization channel closed"))?;

            let option_id = outcome.option_id.0.as_ref();
            if option_id == acp_thread::SANDBOX_FALLBACK_RETRY_OPTION_ID {
                return Ok(SandboxFallbackDecision::Retry);
            }
            match acp_thread::SandboxPermission::from_id(option_id) {
                Some(acp_thread::SandboxPermission::AllowOnce) => {
                    Ok(SandboxFallbackDecision::RunUnsandboxed)
                }
                Some(acp_thread::SandboxPermission::AllowThread) => {
                    sandbox_grants.borrow_mut().record_fallback();
                    Self::persist_thread_grants(&thread, cx);
                    Ok(SandboxFallbackDecision::RunUnsandboxed)
                }
                Some(acp_thread::SandboxPermission::AllowAlways) => {
                    sandbox_grants.borrow_mut().record_fallback();
                    Self::persist_thread_grants(&thread, cx);
                    Self::persist_sandbox_unsandboxed_permission(fs, cx);
                    Ok(SandboxFallbackDecision::RunUnsandboxed)
                }
                Some(acp_thread::SandboxPermission::Deny) => Ok(SandboxFallbackDecision::Deny),
                None => {
                    let other = option_id;
                    debug_assert!(false, "unexpected sandbox fallback option_id: {other}");
                    Ok(SandboxFallbackDecision::Deny)
                }
            }
        })
    }

    /// Persist the `allow_unsandboxed` setting. Going forward this turns
    /// sandboxing off for the model-facing surface: later turns expose the
    /// plain `terminal` tool (with no sandbox prompt section) and commands run
    /// without an OS sandbox. On Windows, WSL sandbox setup is skipped.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    fn persist_sandbox_unsandboxed_permission(fs: Option<Arc<dyn Fs>>, cx: &AsyncApp) {
        let Some(fs) = fs else {
            log::error!(
                "Cannot persist \"allow always\" unsandboxed permission: no filesystem available"
            );
            return;
        };
        cx.update(|cx| {
            update_settings_file(fs, cx, move |settings, _| {
                settings
                    .agent
                    .get_or_insert_default()
                    .allow_sandbox_unsandboxed();
            });
        });
    }

    /// Prompts the user to choose between an explicit set of actions and
    /// returns the chosen `option_id`.
    ///
    /// Unlike [`Self::authorize`] / [`Self::authorize_always_prompt`], this
    /// does not interpret the user's choice as a permission grant — callers
    /// are responsible for handling each `option_id` explicitly. Use this
    /// when a tool needs the user to pick between several side-effecting
    /// actions (for example, "Save" vs "Discard" for a dirty buffer).
    pub fn prompt_for_decision(
        &self,
        title: Option<String>,
        message: Option<String>,
        options: Vec<acp::PermissionOption>,
        cx: &mut App,
    ) -> Task<Result<acp::PermissionOptionId>> {
        let options = acp_thread::PermissionOptions::Flat(options);
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        cx.spawn(async move |_cx| {
            let mut fields = acp::ToolCallUpdateFields::new();
            if let Some(title) = title {
                fields = fields.title(title);
            }
            if let Some(message) = message {
                fields = fields.content(vec![acp::ToolCallContent::from(message)]);
            }

            let (response_tx, response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        tool_call: acp::ToolCallUpdate::new(tool_use_id.to_string(), fields),
                        options,
                        response: response_tx,
                        context: None,
                        kind: acp_thread::AuthorizationKind::ActionChoice,
                    },
                )))
            {
                log::error!("Failed to send tool call decision prompt: {error}");
                return Err(anyhow!("Failed to send tool call decision prompt: {error}"));
            }

            let outcome = response_rx
                .await
                .map_err(|_| anyhow!("authorization channel closed"))?;
            Ok(outcome.option_id)
        })
    }

    /// Prompts the user for authorization.
    ///
    /// When `check_settings` is `Some`, this gate is settings-driven: the
    /// settings are evaluated up-front (an Allow or Deny result resolves the
    /// task immediately without prompting), and while a prompt is pending a
    /// `SettingsStore` subscription watches for changes. A subsequent Allow
    /// or Deny dismisses the prompt UI and resolves the task without user
    /// interaction.
    ///
    /// When `check_settings` is `None`, the user is always prompted and
    /// settings changes are ignored. This suits prompts that aren't
    /// settings-driven (e.g. symlink-escape confirmations).
    fn run_authorization_loop(
        &self,
        title: String,
        options: acp_thread::PermissionOptions,
        context: Option<ToolPermissionContext>,
        check_settings: Option<Box<dyn Fn(&App) -> ToolPermissionDecision>>,
        cx: &mut App,
    ) -> Task<Result<()>> {
        // Short-circuit when current settings yield a definitive answer.
        if let Some(check) = check_settings.as_ref() {
            match check(cx) {
                ToolPermissionDecision::Allow => return Task::ready(Ok(())),
                ToolPermissionDecision::Deny(reason) => {
                    return Task::ready(Err(anyhow!(reason)));
                }
                ToolPermissionDecision::Confirm => {}
            }
        }

        let fs = self.fs.clone();
        let stream = self.stream.clone();
        let tool_use_id = self.tool_use_id.clone();
        let auto_resolution_outcomes = if check_settings.is_some() {
            match (
                auto_resolve_permission_outcome(&options, true),
                auto_resolve_permission_outcome(&options, false),
            ) {
                (Ok(allow), Ok(deny)) => Some((allow, deny)),
                (Err(error), _) | (_, Err(error)) => return Task::ready(Err(error)),
            }
        } else {
            None
        };
        cx.spawn(async move |cx| {
            let (response_tx, mut response_rx) = oneshot::channel();
            if let Err(error) = stream
                .0
                .unbounded_send(Ok(ThreadEvent::ToolCallAuthorization(
                    ToolCallAuthorization {
                        tool_call: acp::ToolCallUpdate::new(
                            tool_use_id.to_string(),
                            acp::ToolCallUpdateFields::new().title(title),
                        ),
                        options,
                        response: response_tx,
                        context,
                        kind: acp_thread::AuthorizationKind::PermissionGrant,
                    },
                )))
            {
                log::error!("Failed to send tool call authorization: {error}");
                return Err(anyhow!("Failed to send tool call authorization: {error}"));
            }

            let Some(check_settings) = check_settings else {
                let outcome = response_rx
                    .await
                    .map_err(|_| anyhow!("authorization channel closed"))?;

                return Self::persist_permission_outcome(&outcome, fs, cx);
            };
            let Some((auto_allow_outcome, auto_deny_outcome)) = auto_resolution_outcomes else {
                return Err(anyhow!("missing auto-resolution outcomes"));
            };

            let (mut settings_tx, mut settings_rx) = watch::channel(());
            let _settings_subscription = cx.update(|cx| {
                cx.observe_global::<SettingsStore>(move |_cx| {
                    settings_tx.send(()).ok();
                })
            });

            // Race the user's response against settings changes. On each
            // settings change, re-evaluate `check_settings`: if it now
            // yields a definitive Allow or Deny, resolve the prompt
            // without user interaction. Otherwise keep waiting on the
            // same prompt.
            loop {
                let settings_changed = async {
                    if settings_rx.changed().await.is_err() {
                        std::future::pending::<()>().await;
                    }
                };
                futures::select_biased! {
                    outcome = (&mut response_rx).fuse() => {
                        let outcome = outcome
                            .map_err(|_| anyhow!("authorization channel closed"))?;
                        return Self::persist_permission_outcome(&outcome, fs.clone(), cx);
                    }
                    _ = settings_changed.fuse() => {
                        // On auto-resolve, we dismiss the prompt UI by
                        // resolving the tool call's `WaitingForConfirmation`
                        // status with an internal selected outcome. Dropping
                        // `response_rx` prevents the synthetic response from
                        // being delivered back into this loop.
                        match cx.update(|cx| check_settings(cx)) {
                            ToolPermissionDecision::Allow => {
                                drop(response_rx);
                                stream.resolve_tool_call_authorization(
                                    &tool_use_id,
                                    auto_allow_outcome.clone(),
                                );
                                return Ok(());
                            }
                            ToolPermissionDecision::Deny(reason) => {
                                drop(response_rx);
                                stream.resolve_tool_call_authorization(
                                    &tool_use_id,
                                    auto_deny_outcome.clone(),
                                );
                                return Err(anyhow!(reason));
                            }
                            ToolPermissionDecision::Confirm => continue,
                        }
                    }
                }
            }
        })
    }

    /// Interprets a `SelectedPermissionOutcome` and persists any settings changes.
    /// Returns `true` if the tool call should be allowed, `false` if denied.
    fn persist_permission_outcome(
        outcome: &acp_thread::SelectedPermissionOutcome,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) -> Result<()> {
        let option_id = outcome.option_id.0.as_ref();
        let err = || Err(anyhow!("Permission to run tool denied by user"));

        let always_permission = option_id
            .strip_prefix("always_allow:")
            .map(|tool| (tool, ToolPermissionMode::Allow))
            .or_else(|| {
                option_id
                    .strip_prefix("always_deny:")
                    .map(|tool| (tool, ToolPermissionMode::Deny))
            })
            .or_else(|| {
                option_id
                    .strip_prefix("always_allow_mcp:")
                    .map(|tool| (tool, ToolPermissionMode::Allow))
            })
            .or_else(|| {
                option_id
                    .strip_prefix("always_deny_mcp:")
                    .map(|tool| (tool, ToolPermissionMode::Deny))
            });

        if let Some((tool, mode)) = always_permission {
            let params = outcome.params.as_ref();
            Self::persist_always_permission(tool, mode, params, fs, cx);
            return if mode == ToolPermissionMode::Allow {
                Ok(())
            } else {
                err()
            };
        }

        // Handle simple "allow" / "deny" (once, no persistence)
        if option_id == "allow" || option_id == "deny" {
            debug_assert!(
                outcome.params.is_none(),
                "unexpected params for once-only permission"
            );
            return if option_id == "allow" { Ok(()) } else { err() };
        }

        debug_assert!(false, "unexpected permission option_id: {option_id}");

        err()
    }

    /// Persists an "always allow" or "always deny" permission, using sub_patterns
    /// from params when present.
    fn persist_always_permission(
        tool: &str,
        mode: ToolPermissionMode,
        params: Option<&acp_thread::SelectedPermissionParams>,
        fs: Option<Arc<dyn Fs>>,
        cx: &AsyncApp,
    ) {
        let Some(fs) = fs else {
            return;
        };

        match params {
            Some(acp_thread::SelectedPermissionParams::Terminal {
                patterns: sub_patterns,
            }) => {
                debug_assert!(
                    !sub_patterns.is_empty(),
                    "empty sub_patterns for tool {tool} — callers should pass None instead"
                );
                let tool = tool.to_string();
                let sub_patterns = sub_patterns.clone();
                cx.update(|cx| {
                    update_settings_file(fs, cx, move |settings, _| {
                        let agent = settings.agent.get_or_insert_default();
                        for pattern in sub_patterns {
                            match mode {
                                ToolPermissionMode::Allow => {
                                    agent.add_tool_allow_pattern(&tool, pattern);
                                }
                                ToolPermissionMode::Deny => {
                                    agent.add_tool_deny_pattern(&tool, pattern);
                                }
                                // If there's no matching pattern this will
                                // default to confirm, so falling through is
                                // fine here.
                                ToolPermissionMode::Confirm => (),
                            }
                        }
                    });
                });
            }
            None => {
                let tool = tool.to_string();
                cx.update(|cx| {
                    update_settings_file(fs, cx, move |settings, _| {
                        settings
                            .agent
                            .get_or_insert_default()
                            .set_tool_default_permission(&tool, mode);
                    });
                });
            }
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub struct ToolCallEventStreamReceiver(mpsc::UnboundedReceiver<Result<ThreadEvent>>);

#[cfg(any(test, feature = "test-support"))]
impl ToolCallEventStreamReceiver {
    pub async fn expect_authorization(&mut self) -> ToolCallAuthorization {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallAuthorization(auth))) = event {
            auth
        } else {
            panic!("Expected ToolCallAuthorization but got: {:?}", event);
        }
    }

    pub async fn expect_update_fields(&mut self) -> acp::ToolCallUpdateFields {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(
            update,
        )))) = event
        {
            update.fields
        } else {
            panic!("Expected update fields but got: {:?}", event);
        }
    }

    pub async fn expect_authorization_resolved(
        &mut self,
    ) -> (acp::ToolCallId, acp_thread::SelectedPermissionOutcome) {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallAuthorizationResolved {
            tool_call_id,
            outcome,
        })) = event
        {
            (tool_call_id, outcome)
        } else {
            panic!("Expected authorization resolved but got: {:?}", event);
        }
    }

    pub async fn expect_diff(&mut self) -> Entity<acp_thread::Diff> {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateDiff(
            update,
        )))) = event
        {
            update.diff
        } else {
            panic!("Expected diff but got: {:?}", event);
        }
    }

    pub async fn expect_terminal(&mut self) -> Entity<acp_thread::Terminal> {
        let event = self.0.next().await;
        if let Some(Ok(ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateTerminal(
            update,
        )))) = event
        {
            update.terminal
        } else {
            panic!("Expected terminal but got: {:?}", event);
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl std::ops::Deref for ToolCallEventStreamReceiver {
    type Target = mpsc::UnboundedReceiver<Result<ThreadEvent>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(any(test, feature = "test-support"))]
impl std::ops::DerefMut for ToolCallEventStreamReceiver {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<&str> for UserMessageContent {
    fn from(text: &str) -> Self {
        Self::Text(text.into())
    }
}

impl From<String> for UserMessageContent {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl UserMessageContent {
    pub fn from_content_block(value: acp::ContentBlock, path_style: PathStyle) -> Self {
        match value {
            acp::ContentBlock::Text(text_content) => Self::Text(text_content.text),
            acp::ContentBlock::Image(image_content) => Self::Image(convert_image(image_content)),
            acp::ContentBlock::Audio(_) => {
                // TODO
                Self::Text("[audio]".to_string())
            }
            acp::ContentBlock::ResourceLink(resource_link) => {
                match MentionUri::parse(&resource_link.uri, path_style) {
                    Ok(uri) => Self::Mention {
                        uri,
                        content: SharedString::default(),
                    },
                    Err(err) => {
                        log::error!("Failed to parse mention link: {}", err);
                        Self::Text(format!("[{}]({})", resource_link.name, resource_link.uri))
                    }
                }
            }
            acp::ContentBlock::Resource(resource) => match resource.resource {
                acp::EmbeddedResourceResource::TextResourceContents(resource) => {
                    match MentionUri::parse(&resource.uri, path_style) {
                        Ok(uri) => Self::Mention {
                            uri,
                            content: resource.text.into(),
                        },
                        Err(err) => {
                            log::error!("Failed to parse mention link: {}", err);
                            Self::Text(
                                MarkdownCodeBlock {
                                    tag: &resource.uri,
                                    text: &resource.text,
                                }
                                .to_string(),
                            )
                        }
                    }
                }
                acp::EmbeddedResourceResource::BlobResourceContents(_) => {
                    // TODO
                    Self::Text("[blob]".to_string())
                }
                other => {
                    log::warn!("Unexpected content type: {:?}", other);
                    Self::Text("[unknown]".to_string())
                }
            },
            other => {
                log::warn!("Unexpected content type: {:?}", other);
                Self::Text("[unknown]".to_string())
            }
        }
    }
}

impl From<UserMessageContent> for acp::ContentBlock {
    fn from(content: UserMessageContent) -> Self {
        match content {
            UserMessageContent::Text(text) => text.into(),
            UserMessageContent::Image(image) => {
                acp::ContentBlock::Image(acp::ImageContent::new(image.source, "image/png"))
            }
            UserMessageContent::Mention { uri, content } => acp::ContentBlock::Resource(
                acp::EmbeddedResource::new(acp::EmbeddedResourceResource::TextResourceContents(
                    acp::TextResourceContents::new(content, uri.to_uri().to_string()),
                )),
            ),
        }
    }
}

fn convert_image(image_content: acp::ImageContent) -> LanguageModelImage {
    LanguageModelImage {
        source: image_content.data.into(),
    }
}

#[cfg(test)]
#[path = "thread/test_fixtures.rs"]
mod tests;
