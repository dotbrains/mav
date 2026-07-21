use super::*;

#[derive(Clone, Debug)]
pub(super) enum ActiveEntry {
    Thread {
        thread_id: agent_ui::ThreadId,
        /// Stable remote identifier, used for matching when thread_id
        /// differs (e.g. after cross-window activation creates a new
        /// local ThreadId).
        session_id: Option<acp::SessionId>,
        workspace: Entity<Workspace>,
    },
    Terminal {
        terminal_id: TerminalId,
        workspace: Entity<Workspace>,
    },
}

impl ActiveEntry {
    pub(super) fn workspace(&self) -> &Entity<Workspace> {
        match self {
            ActiveEntry::Thread { workspace, .. } | ActiveEntry::Terminal { workspace, .. } => {
                workspace
            }
        }
    }

    pub(super) fn is_active_thread(&self, thread_id: &agent_ui::ThreadId) -> bool {
        matches!(self, ActiveEntry::Thread { thread_id: active_thread_id, .. } if active_thread_id == thread_id)
    }

    pub(super) fn is_active_terminal(&self, terminal_id: TerminalId) -> bool {
        matches!(self, ActiveEntry::Terminal { terminal_id: active_terminal_id, .. } if *active_terminal_id == terminal_id)
    }

    pub(super) fn matches_entry(&self, entry: &ListEntry) -> bool {
        match (self, entry) {
            (
                ActiveEntry::Thread {
                    thread_id,
                    session_id,
                    ..
                },
                ListEntry::Thread(thread),
            ) => {
                *thread_id == thread.metadata.thread_id
                    || session_id
                        .as_ref()
                        .zip(thread.metadata.session_id.as_ref())
                        .is_some_and(|(a, b)| a == b)
            }
            (ActiveEntry::Terminal { terminal_id, .. }, ListEntry::Terminal(terminal)) => {
                *terminal_id == terminal.metadata.terminal_id
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ActiveThreadInfo {
    pub(super) session_id: acp::SessionId,
    pub(super) title: SharedString,
    pub(super) status: AgentThreadStatus,
    pub(super) icon: IconName,
    pub(super) icon_from_external_svg: Option<SharedString>,
    pub(super) is_background: bool,
    pub(super) is_title_generating: bool,
    pub(super) diff_stats: DiffStats,
}

#[derive(Clone)]
pub(super) enum ThreadEntryWorkspace {
    Open(Entity<Workspace>),
    Closed {
        /// The paths this entry uses (may point to linked worktrees).
        folder_paths: PathList,
        /// The project group this entry belongs to.
        project_group_key: ProjectGroupKey,
    },
}

impl ThreadEntryWorkspace {
    pub(super) fn is_remote(&self, cx: &App) -> bool {
        match self {
            ThreadEntryWorkspace::Open(workspace) => {
                !workspace.read(cx).project().read(cx).is_local()
            }
            ThreadEntryWorkspace::Closed {
                project_group_key, ..
            } => project_group_key.host().is_some(),
        }
    }
}

/// If the title begins with a decorative prefix (such as a leading emoji,
/// spinner glyph, or symbol the agent prefixed the title with), splits that
/// prefix off so a single representative glyph can be displayed in place of the
/// entry's icon.
pub(super) fn split_leading_icon_char(
    title: &SharedString,
    highlight_positions: &[usize],
) -> Option<(SharedString, SharedString, Vec<usize>)> {
    let prefix = terminal_title_prefix(title)?;
    let icon_char = pick_icon_glyph(prefix)?;

    let stripped_len = prefix.len();
    let trimmed_title = &title[stripped_len..];
    if trimmed_title.is_empty() {
        return None;
    }

    let adjusted_positions = highlight_positions
        .iter()
        .filter(|&&position| position >= stripped_len)
        .map(|&position| position - stripped_len)
        .collect();

    Some((
        icon_char,
        trimmed_title.to_string().into(),
        adjusted_positions,
    ))
}

/// Picks a single glyph to render as the icon from a detected title prefix.
///
/// We only ever show one glyph, so this makes a best effort to choose a
/// meaningful one by glancing at the leading characters of the prefix:
/// runs of `.` are condensed into a single ellipsis, surrounding ASCII brackets
/// are stripped (so `[!]` yields `!`), and a leading run of the same character
/// is collapsed (so `>>>` yields `>`). The result is the first grapheme cluster
/// of whatever remains, keeping multi-codepoint emoji intact.
fn pick_icon_glyph(prefix: &str) -> Option<SharedString> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return None;
    }

    // Strip a single pair of surrounding ASCII brackets, e.g. `[!]` -> `!`.
    let unwrapped = match prefix.chars().next() {
        Some('[') => prefix.strip_prefix('[').and_then(|s| s.strip_suffix(']')),
        Some('(') => prefix.strip_prefix('(').and_then(|s| s.strip_suffix(')')),
        Some('{') => prefix.strip_prefix('{').and_then(|s| s.strip_suffix('}')),
        Some('<') => prefix.strip_prefix('<').and_then(|s| s.strip_suffix('>')),
        _ => None,
    };
    let prefix = unwrapped
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(prefix);

    // Condense a leading run of dots (`...`) into a single ellipsis.
    if prefix.starts_with("..") {
        return Some("\u{2026}".into());
    }

    // Take the first grapheme cluster so multi-codepoint emoji stay intact.
    let first_grapheme = prefix.graphemes(true).next()?;
    if first_grapheme.trim().is_empty() {
        return None;
    }

    Some(first_grapheme.to_string().into())
}

pub(super) fn draft_display_label_for_thread_metadata(
    metadata: &ThreadMetadata,
    workspace: &ThreadEntryWorkspace,
    cx: &App,
) -> Option<(SharedString, DraftKind)> {
    let workspace = match workspace {
        ThreadEntryWorkspace::Open(workspace) => Some(workspace),
        ThreadEntryWorkspace::Closed { .. } => None,
    };

    if let Some(label) =
        agent_ui::draft_prompt_store::display_label_for_draft(workspace, metadata.thread_id, cx)
    {
        return Some((label, DraftKind::WithContent));
    }

    let placeholder = agent_ui::draft_prompt_store::empty_draft_placeholder_label(
        workspace,
        &metadata.agent_id,
        cx,
    );
    Some((placeholder, DraftKind::Empty))
}

pub(super) fn thread_metadata_would_render_sidebar_row(
    metadata: &ThreadMetadata,
    workspace: &ThreadEntryWorkspace,
    cx: &App,
) -> bool {
    if !metadata.is_draft() {
        return true;
    }

    draft_display_label_for_thread_metadata(metadata, workspace, cx).is_some()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DraftKind {
    WithContent,
    Empty,
}

#[derive(Clone)]
pub(super) struct ThreadEntry {
    pub(super) metadata: ThreadMetadata,
    pub(super) icon: IconName,
    pub(super) icon_from_external_svg: Option<SharedString>,
    pub(super) status: AgentThreadStatus,
    pub(super) workspace: ThreadEntryWorkspace,
    pub(super) is_live: bool,
    pub(super) is_background: bool,
    pub(super) is_title_generating: bool,
    pub(super) draft: Option<DraftKind>,
    pub(super) highlight_positions: Vec<usize>,
    pub(super) worktrees: Vec<ThreadItemWorktreeInfo>,
    pub(super) diff_stats: DiffStats,
}

#[derive(Clone)]
pub(super) struct TerminalEntry {
    pub(super) metadata: TerminalThreadMetadata,
    pub(super) workspace: ThreadEntryWorkspace,
    pub(super) worktrees: Vec<ThreadItemWorktreeInfo>,
    pub(super) has_notification: bool,
    pub(super) highlight_positions: Vec<usize>,
}

impl ThreadEntry {
    /// Updates this thread entry with active thread information.
    ///
    /// The existing [`ThreadEntry`] was likely deserialized from the database
    /// but if we have a correspond thread already loaded we want to apply the
    /// live information.
    pub(super) fn apply_active_info(&mut self, info: &ActiveThreadInfo) {
        self.metadata.title = Some(info.title.clone());
        self.status = info.status;
        self.icon = info.icon;
        self.icon_from_external_svg = info.icon_from_external_svg.clone();
        self.is_live = true;
        self.is_background = info.is_background;
        self.is_title_generating = info.is_title_generating;
        self.diff_stats = info.diff_stats;
    }
}

#[derive(Clone)]
pub(super) enum ListEntry {
    ProjectHeader {
        key: ProjectGroupKey,
        label: SharedString,
        highlight_positions: Vec<usize>,
        has_running_threads: bool,
        waiting_thread_count: usize,
        has_notifications: bool,
        is_active: bool,
        has_threads: bool,
    },
    Thread(Arc<ThreadEntry>),
    Terminal(TerminalEntry),
}

#[derive(Clone)]
pub(super) enum ActivatableEntry {
    Thread {
        metadata: ThreadMetadata,
        workspace: ThreadEntryWorkspace,
    },
    Terminal {
        metadata: TerminalThreadMetadata,
        workspace: ThreadEntryWorkspace,
    },
}

impl ActivatableEntry {
    pub(super) fn from_list_entry(entry: &ListEntry) -> Option<Self> {
        match entry {
            ListEntry::Thread(thread) => Some(Self::Thread {
                metadata: thread.metadata.clone(),
                workspace: thread.workspace.clone(),
            }),
            ListEntry::Terminal(terminal) => Some(Self::Terminal {
                metadata: terminal.metadata.clone(),
                workspace: terminal.workspace.clone(),
            }),
            ListEntry::ProjectHeader { .. } => None,
        }
    }

    pub(super) fn project_location(&self, cx: &App) -> (PathList, ProjectGroupKey) {
        match self {
            Self::Thread {
                workspace: ThreadEntryWorkspace::Open(workspace),
                ..
            }
            | Self::Terminal {
                workspace: ThreadEntryWorkspace::Open(workspace),
                ..
            } => (
                PathList::new(&workspace.read(cx).root_paths(cx)),
                workspace.read(cx).project_group_key(cx),
            ),
            Self::Thread {
                workspace:
                    ThreadEntryWorkspace::Closed {
                        folder_paths,
                        project_group_key,
                    },
                ..
            }
            | Self::Terminal {
                workspace:
                    ThreadEntryWorkspace::Closed {
                        folder_paths,
                        project_group_key,
                    },
                ..
            } => (folder_paths.clone(), project_group_key.clone()),
        }
    }
}

#[cfg(test)]
impl ListEntry {
    fn session_id(&self) -> Option<&acp::SessionId> {
        match self {
            ListEntry::Thread(thread_entry) => thread_entry.metadata.session_id.as_ref(),
            ListEntry::Terminal(_) | ListEntry::ProjectHeader { .. } => None,
        }
    }

    fn reachable_workspaces<'a>(
        &'a self,
        multi_workspace: &'a workspace::MultiWorkspace,
        cx: &'a App,
    ) -> Vec<Entity<Workspace>> {
        match self {
            ListEntry::Thread(thread) => match &thread.workspace {
                ThreadEntryWorkspace::Open(ws) => vec![ws.clone()],
                ThreadEntryWorkspace::Closed { .. } => Vec::new(),
            },
            ListEntry::Terminal(terminal) => match &terminal.workspace {
                ThreadEntryWorkspace::Open(workspace) => vec![workspace.clone()],
                ThreadEntryWorkspace::Closed { .. } => Vec::new(),
            },
            ListEntry::ProjectHeader { key, .. } => multi_workspace
                .workspaces_for_project_group(key, cx)
                .unwrap_or_default(),
        }
    }
}

impl From<ThreadEntry> for ListEntry {
    fn from(thread: ThreadEntry) -> Self {
        ListEntry::Thread(Arc::new(thread))
    }
}

impl From<TerminalEntry> for ListEntry {
    fn from(terminal: TerminalEntry) -> Self {
        ListEntry::Terminal(terminal)
    }
}

#[derive(Default)]
pub(super) struct SidebarContents {
    pub(super) entries: Vec<ListEntry>,
    pub(super) notified_threads: HashSet<agent_ui::ThreadId>,
    pub(super) notified_terminals: HashSet<TerminalId>,
    pub(super) project_header_indices: Vec<usize>,
    pub(super) has_open_projects: bool,
}

/// Identity-and-layout key for a [`ListEntry`] used to preserve measured list items
/// across rebuilds. Equal shapes must render to the same height; add any new
/// height-affecting state here.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum EntryShape {
    ProjectHeader {
        key: ProjectGroupKey,
        // Toggles the "No threads yet" empty-state row when not collapsed.
        has_threads: bool,
        // Determines whether the "No threads yet" row is rendered (only shown when
        // `!is_collapsed && !has_threads`).
        is_collapsed: bool,
    },
    Thread(ThreadId),
    Terminal(TerminalId),
}

impl SidebarContents {
    pub(super) fn is_thread_notified(&self, thread_id: &agent_ui::ThreadId) -> bool {
        self.notified_threads.contains(thread_id)
    }

    pub(super) fn is_terminal_notified(&self, terminal_id: TerminalId) -> bool {
        self.notified_terminals.contains(&terminal_id)
    }
}
