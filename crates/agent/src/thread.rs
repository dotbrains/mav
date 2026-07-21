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
#[path = "thread/event_stream.rs"]
mod event_stream;
#[path = "thread/lifecycle.rs"]
mod lifecycle;
#[cfg(test)]
#[path = "thread/manual_compaction_tests.rs"]
mod manual_compaction_tests;
#[path = "thread/markdown.rs"]
mod markdown;
mod message;
#[path = "thread/model_settings.rs"]
mod model_settings;
#[path = "thread/persistence.rs"]
mod persistence;
#[path = "thread/replay.rs"]
mod replay;
#[path = "thread/running_turn.rs"]
mod running_turn;
#[cfg(test)]
#[path = "thread/sandbox_authorization_tests.rs"]
mod sandbox_authorization_tests;
#[path = "thread/subagent.rs"]
mod subagent;
#[cfg(test)]
#[path = "thread/subagent_settings_tests.rs"]
mod subagent_settings_tests;
#[path = "thread/title_generation.rs"]
mod title_generation;
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
use event_stream::ThreadEventStream;
pub(crate) use markdown::messages_to_markdown;
pub use message::*;
use running_turn::RunningTurn;
#[cfg(any(test, feature = "test-support"))]
pub use tool_call_event_receiver::ToolCallEventStreamReceiver;
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
