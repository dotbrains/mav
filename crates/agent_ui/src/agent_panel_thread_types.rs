use gpui::SharedString;
use workspace::PathList;

use crate::{Agent, AgentInitialContent};

pub(crate) struct SourcePanelInitialization {
    pub(crate) agent: Agent,
    pub(crate) initial_content: Option<AgentInitialContent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadTitleRegenerationResult {
    NotOpen,
    Started,
    NoModel,
    AlreadyGenerating,
}

/// Optional parameters for `AgentPanel::create_thread_with_options`. All
/// fields default to the panel's current selection so the agent tool only
/// needs to override what it actually cares about.
#[derive(Default)]
pub struct CreateThreadOptions {
    /// Title to assign to the new thread up front.
    pub title: Option<SharedString>,
    /// Initial content to populate in the thread (optionally auto-submitted).
    pub initial_content: Option<AgentInitialContent>,
    /// Agent to use. Defaults to the panel's selected agent.
    pub agent: Option<Agent>,
    /// Model override, as `provider/model-id`. Only applied when the thread
    /// uses the native Mav agent.
    pub model: Option<String>,
    /// Working directories to attach to the new thread (e.g., the path of a
    /// freshly-created sibling worktree). When `None`, the thread inherits
    /// the project's default path list.
    pub work_dirs: Option<PathList>,
}
