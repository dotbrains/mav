use super::{ItemHandle, MultiWorkspace, OpenMode, OpenVisible, Workspace};
use collections::HashMap;
use gpui::{Entity, WindowHandle};

/// Controls whether to reuse an existing workspace whose worktrees contain the
/// given paths, and how broadly to match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum WorkspaceMatching {
    /// Always open a new workspace. No matching against existing worktrees.
    None,
    /// Match paths against existing worktree roots and files within them.
    #[default]
    MatchExact,
    /// Match paths against existing worktrees including subdirectories, and
    /// fall back to any existing window if no worktree matched.
    ///
    /// For example, `mav -a foo/bar` will activate the `bar` workspace if it
    /// exists, otherwise it will open a new window with `foo/bar` as the root.
    MatchSubdirectory,
}

#[derive(Clone)]
pub struct OpenOptions {
    pub visible: Option<OpenVisible>,
    pub focus: Option<bool>,
    pub workspace_matching: WorkspaceMatching,
    /// Whether to add unmatched directories to the existing window's sidebar
    /// rather than opening a new window. Defaults to true, matching the default
    /// `cli_default_open_behavior` setting.
    pub add_dirs_to_sidebar: bool,
    pub wait: bool,
    pub requesting_window: Option<WindowHandle<MultiWorkspace>>,
    pub open_mode: OpenMode,
    pub env: Option<HashMap<String, String>>,
    pub open_in_dev_container: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            visible: None,
            focus: None,
            workspace_matching: WorkspaceMatching::default(),
            add_dirs_to_sidebar: true,
            wait: false,
            requesting_window: None,
            open_mode: OpenMode::default(),
            env: None,
            open_in_dev_container: false,
        }
    }
}

impl OpenOptions {
    pub(super) fn should_reuse_existing_window(&self) -> bool {
        self.workspace_matching != WorkspaceMatching::None && self.open_mode != OpenMode::NewWindow
    }
}

/// The result of opening a workspace via `open_paths`, `Workspace::new_local`,
/// or `Workspace::open_workspace_for_paths`.
pub struct OpenResult {
    pub window: WindowHandle<MultiWorkspace>,
    pub workspace: Entity<Workspace>,
    pub opened_items: Vec<Option<anyhow::Result<Box<dyn ItemHandle>>>>,
}
