use super::*;

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
pub(crate) const COMPACTION_RETAINED_USER_MESSAGES_BYTE_BUDGET: usize = 80_000;

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
pub(crate) enum RetryStrategy {
    ExponentialBackoff {
        initial_delay: Duration,
        max_attempts: u8,
    },
    Fixed {
        delay: Duration,
        max_attempts: u8,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CompletionError {
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
    pub(crate) fn as_model(&self) -> Option<&Arc<dyn LanguageModel>> {
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
