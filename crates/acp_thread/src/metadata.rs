use super::*;

/// Key used in ACP ToolCall meta to store the tool's programmatic name.
/// This is a workaround since ACP's ToolCall doesn't have a dedicated name field.
pub const TOOL_NAME_META_KEY: &str = "tool_name";

/// Helper to extract tool name from ACP meta
pub fn tool_name_from_meta(meta: &Option<acp::Meta>) -> Option<SharedString> {
    meta.as_ref()
        .and_then(|m| m.get(TOOL_NAME_META_KEY))
        .and_then(|v| v.as_str())
        .map(|s| SharedString::from(s.to_owned()))
}

/// Helper to create meta with tool name
pub fn meta_with_tool_name(tool_name: &str) -> acp::Meta {
    acp::Meta::from_iter([(TOOL_NAME_META_KEY.into(), tool_name.into())])
}

/// Key used in ACP `AvailableCommand` meta to record which source produced a
/// slash command, so the completion popup can group commands by category.
pub const COMMAND_CATEGORY_META_KEY: &str = "command_category";

/// The source category of a slash command, used to group commands in the
/// completion popup. Only the native Mav agent annotates its commands; commands
/// from external ACP agents carry no category and are grouped on their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCategory {
    /// Built-in Mav agent commands (e.g. `/compact`).
    Native,
    /// Commands sourced from MCP server prompts.
    Mcp,
}

impl CommandCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Mcp => "mcp",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "native" => Some(Self::Native),
            "mcp" => Some(Self::Mcp),
            _ => None,
        }
    }
}

pub fn meta_with_command_category(category: CommandCategory) -> acp::Meta {
    acp::Meta::from_iter([(COMMAND_CATEGORY_META_KEY.into(), category.as_str().into())])
}

pub fn command_category_from_meta(meta: &Option<acp::Meta>) -> Option<CommandCategory> {
    meta.as_ref()
        .and_then(|m| m.get(COMMAND_CATEGORY_META_KEY))
        .and_then(|v| v.as_str())
        .and_then(CommandCategory::from_str)
}

/// Key used in ACP ToolCall meta to store the session id and message indexes
pub const SUBAGENT_SESSION_INFO_META_KEY: &str = "subagent_session_info";

pub const SANDBOX_AUTHORIZATION_META_KEY: &str = "sandbox_authorization";

/// Stable `PermissionOption` ids for the sandbox-escalation approval prompt.
///
/// These are shared across the option construction (in the agent), the outcome
/// dispatch, and the UI so the distinct grant lifetimes stay in sync. Note
/// that `AllowThread` and `AllowAlways` both use
/// `PermissionOptionKind::AllowAlways`; the id is what distinguishes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxPermission {
    AllowOnce,
    AllowThread,
    AllowAlways,
    Deny,
}

impl SandboxPermission {
    pub fn as_id(self) -> &'static str {
        match self {
            Self::AllowOnce => "allow",
            Self::AllowThread => "allow_thread",
            Self::AllowAlways => "allow_always",
            Self::Deny => "deny",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "allow" => Some(Self::AllowOnce),
            "allow_thread" => Some(Self::AllowThread),
            "allow_always" => Some(Self::AllowAlways),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct SandboxAuthorizationDetails {
    #[serde(default)]
    pub command: Option<String>,
    /// Specific hosts the command requested network access to, in canonical
    /// form (`github.com`, `*.npmjs.org`). Empty when no specific hosts were
    /// requested (see `network_all_hosts`).
    #[serde(default)]
    pub network_hosts: Vec<String>,
    /// Whether the command requested access to any host ("arbitrary network
    /// access"). The `network` alias deserializes the field this replaced —
    /// a plain bool meaning "network access" — so details persisted by older
    /// builds still render the network request.
    #[serde(default, alias = "network")]
    pub network_all_hosts: bool,
    /// Whether the command requested access to protected `.git` directories.
    #[serde(default)]
    pub allow_git_access: bool,
    #[serde(default)]
    pub allow_fs_write_all: bool,
    #[serde(default)]
    pub unsandboxed: bool,
    #[serde(default)]
    pub write_paths: Vec<PathBuf>,
    /// The agent-provided justification for requesting these permissions,
    /// shown to the user (attributed to the agent) in the approval prompt.
    #[serde(default)]
    pub reason: String,
}

pub fn meta_with_sandbox_authorization(details: SandboxAuthorizationDetails) -> acp::Meta {
    acp::Meta::from_iter([(
        SANDBOX_AUTHORIZATION_META_KEY.into(),
        serde_json::to_value(details).unwrap_or_default(),
    )])
}

pub fn sandbox_authorization_details_from_meta(
    meta: &Option<acp::Meta>,
) -> Option<SandboxAuthorizationDetails> {
    meta.as_ref()
        .and_then(|m| m.get(SANDBOX_AUTHORIZATION_META_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

pub const SANDBOX_FALLBACK_AUTHORIZATION_META_KEY: &str = "sandbox_fallback_authorization";

/// Stable `PermissionOption` id for the "Retry" choice in the sandbox
/// *fallback* prompt (shown when the OS sandbox can't be created on this
/// system). The remaining choices reuse the [`SandboxPermission`] ids.
pub const SANDBOX_FALLBACK_RETRY_OPTION_ID: &str = "retry";

/// Details shown when the OS sandbox could not be created for a command and
/// the user is asked whether to run it without a sandbox. Distinct from
/// [`SandboxAuthorizationDetails`] (a model-requested *escalation*): here the
/// sandbox itself failed, so the prompt explains why and offers a retry.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct SandboxFallbackAuthorizationDetails {
    #[serde(default)]
    pub command: Option<String>,
    /// Human-readable reason the OS sandbox could not be created (for example,
    /// "bwrap not found on PATH"), shown to the user so they can decide
    /// whether to run the command without a sandbox.
    #[serde(default)]
    pub reason: String,
}

pub fn meta_with_sandbox_fallback_authorization(
    details: SandboxFallbackAuthorizationDetails,
) -> acp::Meta {
    acp::Meta::from_iter([(
        SANDBOX_FALLBACK_AUTHORIZATION_META_KEY.into(),
        serde_json::to_value(details).unwrap_or_default(),
    )])
}

pub fn sandbox_fallback_authorization_details_from_meta(
    meta: &Option<acp::Meta>,
) -> Option<SandboxFallbackAuthorizationDetails> {
    meta.as_ref()
        .and_then(|m| m.get(SANDBOX_FALLBACK_AUTHORIZATION_META_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Meta key recording why the OS sandbox was not applied to a terminal tool
/// call, even though sandboxing was active for the thread. The value is a
/// serialized [`SandboxNotAppliedReason`]. Surfaced as a warning in the UI and
/// used to explain the situation to both the user and the agent.
pub const SANDBOX_NOT_APPLIED_META_KEY: &str = "sandbox_not_applied";

pub fn meta_with_sandbox_not_applied(reason: &SandboxNotAppliedReason) -> acp::Meta {
    acp::Meta::from_iter([(
        SANDBOX_NOT_APPLIED_META_KEY.into(),
        serde_json::to_value(reason).unwrap_or_default(),
    )])
}

pub fn sandbox_not_applied_from_meta(meta: &Option<acp::Meta>) -> Option<SandboxNotAppliedReason> {
    meta.as_ref()
        .and_then(|m| m.get(SANDBOX_NOT_APPLIED_META_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubagentSessionInfo {
    /// The session id of the subagent sessiont that was spawned
    pub session_id: acp::SessionId,
    /// The index of the message of the start of the "turn" run by this tool call
    pub message_start_index: usize,
    /// The index of the output of the message that the subagent has returned
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_end_index: Option<usize>,
}

/// Helper to extract subagent session id from ACP meta
pub fn subagent_session_info_from_meta(meta: &Option<acp::Meta>) -> Option<SubagentSessionInfo> {
    meta.as_ref()
        .and_then(|m| m.get(SUBAGENT_SESSION_INFO_META_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}
