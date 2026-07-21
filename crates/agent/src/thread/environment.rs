use super::*;

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
