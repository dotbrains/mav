use super::*;

#[derive(Clone)]
pub(crate) enum AgentContextSelection {
    Editor(Vec<(Entity<Buffer>, Range<text::Anchor>)>),
    Terminal(Vec<String>),
}

#[derive(Clone)]
pub(crate) enum AgentContextSource {
    Editor(WeakEntity<Editor>),
    TerminalView(WeakEntity<TerminalView>),
    TerminalPanel,
}

impl AgentContextSource {
    pub(crate) fn read_selection(
        &self,
        workspace: &Workspace,
        include_current_line: bool,
        cx: &mut App,
    ) -> Option<AgentContextSelection> {
        match self {
            Self::Editor(handle) => {
                let editor = handle.upgrade()?;
                let ranges = editor_selection_ranges(&editor, include_current_line, cx);
                (!ranges.is_empty()).then_some(AgentContextSelection::Editor(ranges))
            }
            Self::TerminalView(handle) => {
                let terminal_view = handle.upgrade()?;
                terminal_view_selection(&terminal_view, cx)
                    .map(|text| AgentContextSelection::Terminal(vec![text]))
            }
            Self::TerminalPanel => {
                let panel = workspace.panel::<TerminalPanel>(cx)?;
                let selections = panel.read(cx).terminal_selections(cx);
                (!selections.is_empty()).then_some(AgentContextSelection::Terminal(selections))
            }
        }
    }

    pub(crate) fn from_focused(workspace: &Workspace, window: &Window, cx: &App) -> Option<Self> {
        if let Some(agent_panel) = workspace.panel::<AgentPanel>(cx)
            && agent_panel.focus_handle(cx).contains_focused(window, cx)
        {
            return None;
        }

        if let Some(active_item) = workspace.active_item(cx) {
            if let Some(editor) = active_item.act_as::<Editor>(cx) {
                if editor.focus_handle(cx).is_focused(window) {
                    return Some(Self::Editor(editor.downgrade()));
                }
            } else if let Some(terminal_view) = active_item.act_as::<TerminalView>(cx)
                && terminal_view.focus_handle(cx).is_focused(window)
            {
                return Some(Self::TerminalView(terminal_view.downgrade()));
            }
        }

        if let Some(panel) = workspace.panel::<TerminalPanel>(cx)
            && panel.focus_handle(cx).contains_focused(window, cx)
        {
            return Some(Self::TerminalPanel);
        }

        None
    }

    pub(crate) fn from_active(workspace: &Workspace, cx: &App) -> Option<Self> {
        if let Some(active_item) = workspace.active_item(cx) {
            if let Some(editor) = active_item.act_as::<Editor>(cx) {
                return Some(Self::Editor(editor.downgrade()));
            } else if let Some(terminal_view) = active_item.act_as::<TerminalView>(cx) {
                return Some(Self::TerminalView(terminal_view.downgrade()));
            }
        }
        if terminal_panel_dock_is_open(workspace, cx) {
            return Some(Self::TerminalPanel);
        }
        None
    }

    pub(crate) fn exists(&self, workspace: &Workspace, cx: &App) -> bool {
        match self {
            Self::Editor(handle) => handle.upgrade().is_some(),
            Self::TerminalView(handle) => handle.upgrade().is_some(),
            Self::TerminalPanel => terminal_panel_dock_is_open(workspace, cx),
        }
    }
}

fn terminal_panel_dock_is_open(workspace: &Workspace, cx: &App) -> bool {
    workspace.panel::<TerminalPanel>(cx).is_some_and(|_| false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptContextEntry {
    Mode(PromptContextType),
    Action(PromptContextAction),
}

impl PromptContextEntry {
    pub fn keyword(&self) -> &'static str {
        match self {
            Self::Mode(mode) => mode.keyword(),
            Self::Action(action) => action.keyword(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptContextType {
    File,
    Symbol,
    Fetch,
    Thread,
    Skill,
    Diagnostics,
    BranchDiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptContextAction {
    AddSelections,
}

impl PromptContextAction {
    pub fn keyword(&self) -> &'static str {
        match self {
            Self::AddSelections => "selection",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::AddSelections => "Selection",
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            Self::AddSelections => IconName::Reader,
        }
    }
}

impl TryFrom<&str> for PromptContextType {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "file" => Ok(Self::File),
            "symbol" => Ok(Self::Symbol),
            "fetch" => Ok(Self::Fetch),
            "thread" => Ok(Self::Thread),
            "skill" => Ok(Self::Skill),
            "diagnostics" => Ok(Self::Diagnostics),
            "diff" => Ok(Self::BranchDiff),
            _ => Err(format!("Invalid context picker mode: {}", value)),
        }
    }
}

impl PromptContextType {
    pub fn keyword(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::Fetch => "fetch",
            Self::Thread => "thread",
            Self::Skill => "skill",
            Self::Diagnostics => "diagnostics",
            Self::BranchDiff => "branch diff",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::File => "Files & Directories",
            Self::Symbol => "Symbols",
            Self::Fetch => "Fetch",
            Self::Thread => "Threads",
            Self::Skill => "Skills",
            Self::Diagnostics => "Diagnostics",
            Self::BranchDiff => "Branch Diff",
        }
    }

    pub fn icon(&self) -> IconName {
        match self {
            Self::File => IconName::File,
            Self::Symbol => IconName::Code,
            Self::Fetch => IconName::ToolWeb,
            Self::Thread => IconName::Thread,
            Self::Skill => IconName::Sparkle,
            Self::Diagnostics => IconName::Warning,
            Self::BranchDiff => IconName::GitBranch,
        }
    }
}
