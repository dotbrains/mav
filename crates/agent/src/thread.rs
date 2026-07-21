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
use event_stream::ThreadEventStream;
pub(crate) use markdown::messages_to_markdown;
pub use message::*;
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
