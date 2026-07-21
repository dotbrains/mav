use super::*;

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum SerializedSidebarView {
    #[default]
    ThreadList,
    #[serde(alias = "Archive")]
    History,
}

#[derive(Clone, Copy)]
pub(super) enum NewEntryTarget {
    LastCreatedKind,
    Terminal,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct SerializedSidebar {
    #[serde(default)]
    pub(super) width: Option<f32>,
    #[serde(default)]
    pub(super) active_view: SerializedSidebarView,
}

#[derive(Debug, Default)]
pub(super) enum SidebarView {
    #[default]
    ThreadList,
    Archive(Entity<ThreadsArchiveView>),
}

pub(super) enum ArchiveWorktreeOutcome {
    Success,
    Cancelled,
}

// TODO: The mapping from workspace root paths to git repositories needs a
// unified approach across the codebase: this function, `AgentPanel::classify_worktrees`,
// thread persistence (which PathList is saved to the database), and thread
// querying (which PathList is used to read threads back). All of these need
// to agree on how repos are resolved for a given workspace, especially in
// multi-root and nested-repo configurations.
/// The sidebar re-derives its entire entry list from scratch on every
/// change via `update_entries` → `rebuild_contents`. Avoid adding
/// incremental or inter-event coordination state — if something can
/// be computed from the current world state, compute it in the rebuild.
pub struct Sidebar {
    pub(super) multi_workspace: WeakEntity<MultiWorkspace>,
    pub(super) width: Pixels,
    pub(super) focus_handle: FocusHandle,
    pub(super) filter_editor: Entity<Editor>,
    pub(super) thread_rename_editor: Entity<Editor>,
    pub(super) list_state: ListState,
    pub(super) contents: SidebarContents,
    /// The index of the list item that currently has the keyboard focus
    ///
    /// Note: This is NOT the same as the active item.
    pub(super) selection: Option<usize>,
    /// Tracks which sidebar entry is currently active (highlighted).
    pub(super) active_entry: Option<ActiveEntry>,
    pub(super) hovered_thread_index: Option<usize>,
    pub(super) renaming_thread_id: Option<ThreadId>,
    /// Threads in the database-backed regeneration path need their own loading
    /// state because they do not have a live `agent::Thread` to report it.
    pub(super) regenerating_titles: HashSet<ThreadId>,
    /// start_renaming_thread must seed current title into the title editor
    /// so this prevents that BufferEdited event from being interpreted as user input.
    pub(super) suppress_next_rename_edit: bool,

    /// Updated only in response to explicit user actions (clicking a
    /// thread, confirming in the thread switcher, etc.) — never from
    /// background data changes. Used to sort the thread switcher popup.
    pub(super) thread_last_accessed: HashMap<ThreadId, DateTime<Utc>>,
    pub(super) terminal_last_accessed: HashMap<TerminalId, DateTime<Utc>>,
    pub(super) thread_switcher: Option<Entity<ThreadSwitcher>>,
    pub(super) _thread_switcher_subscriptions: Vec<gpui::Subscription>,
    pub(super) pending_thread_activation: Option<agent_ui::ThreadId>,
    /// Persists live thread statuses across rebuilds so that Running→Completed
    /// transitions can be detected even when the group is collapsed (and
    /// thread entries are not present in the list).
    pub(super) live_thread_statuses: HashMap<acp::SessionId, (AgentThreadStatus, ThreadId)>,
    /// Remembers whether each draft last rendered as empty or with content so
    /// that when a draft that was empty gains content again, we refresh
    /// its interaction time.
    pub(super) draft_kinds: HashMap<ThreadId, DraftKind>,
    pub(super) view: SidebarView,
    pub(super) restoring_tasks: HashMap<agent_ui::ThreadId, Task<()>>,
    pub(super) agent_options_menu_handle: PopoverMenuHandle<ContextMenu>,
    pub(super) recent_projects_popover_handle: PopoverMenuHandle<SidebarRecentProjects>,
    pub(super) sidebar_chrome: Entity<title_bar::SidebarChrome>,
    pub(super) project_header_menu_handles: HashMap<usize, PopoverMenuHandle<ContextMenu>>,
    pub(super) project_header_new_thread_menu_handles:
        HashMap<usize, PopoverMenuHandle<ContextMenu>>,
    pub(super) project_header_menu_ix: Option<usize>,
    pub(super) worktree_default_branches: HashMap<ProjectGroupKey, DefaultBranchCache>,
    pub(super) _subscriptions: Vec<gpui::Subscription>,
    pub(super) _draft_editor_observations: Vec<gpui::Subscription>,
    pub(super) update_task: Option<Task<()>>,
    /// For the thread import banners, if there is just one we show "Import
    /// Threads" but if we are showing both the external agents and other
    /// channels import banners then we change the text to disambiguate the
    /// buttons. This field tracks whether we were using verbose labels so they
    /// can stay stable after dismissing one of the banners.
    pub(super) import_banners_use_verbose_labels: Option<bool>,
    /// Display names of other release channels that have threads available to
    /// import.
    pub(super) cross_channel_import_channels: Vec<SharedString>,
}
