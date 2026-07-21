#[path = "sidebar/active_thread_info.rs"]
mod active_thread_info;
#[path = "sidebar/archive_view.rs"]
mod archive_view;
#[path = "sidebar/archive_worktree_planning.rs"]
mod archive_worktree_planning;
#[path = "sidebar/archive_worktree_tasks.rs"]
mod archive_worktree_tasks;
#[path = "sidebar/entry_updates.rs"]
mod entry_updates;
#[path = "sidebar/focus_and_filter.rs"]
mod focus_and_filter;
#[path = "sidebar/import_onboarding_banner.rs"]
mod import_onboarding_banner;
#[path = "sidebar/selection_actions.rs"]
mod selection_actions;
#[path = "sidebar/sidebar_chrome_rendering.rs"]
mod sidebar_chrome_rendering;
#[path = "sidebar/sidebar_entries.rs"]
mod sidebar_entries;
#[path = "sidebar/sidebar_runtime.rs"]
mod sidebar_runtime;
#[path = "sidebar/workspace_subscriptions.rs"]
mod workspace_subscriptions;
use active_thread_info::all_thread_infos_for_workspace;
use import_onboarding_banner::render_import_onboarding_banner;
use sidebar_entries::*;
pub use workspace_info_dump::dump_workspace_info;
#[path = "sidebar/draft_removal.rs"]
mod draft_removal;
#[path = "sidebar/new_entry_creation.rs"]
mod new_entry_creation;
#[path = "sidebar/project_header_menu.rs"]
mod project_header_menu;
#[path = "sidebar/project_header_new_thread.rs"]
mod project_header_new_thread;
#[path = "sidebar/project_header_rendering.rs"]
mod project_header_rendering;
#[path = "sidebar/project_thread_navigation.rs"]
mod project_thread_navigation;
#[path = "sidebar/terminal_activation.rs"]
mod terminal_activation;
#[path = "sidebar/terminal_closing.rs"]
mod terminal_closing;
#[path = "sidebar/thread_activation.rs"]
mod thread_activation;
#[path = "sidebar/thread_archive_actions.rs"]
mod thread_archive_actions;
#[path = "sidebar/thread_entry_rendering.rs"]
mod thread_entry_rendering;
mod thread_switcher;
#[path = "sidebar/thread_switcher_handlers.rs"]
mod thread_switcher_handlers;
#[path = "sidebar/thread_title_management.rs"]
mod thread_title_management;
#[path = "sidebar/workspace_info_dump.rs"]
mod workspace_info_dump;
#[path = "sidebar/workspace_menu.rs"]
mod workspace_menu;

use acp_thread::ThreadStatus;
use action_log::DiffStats;
use agent::{MAV_AGENT_ID, ThreadStore};
use agent_client_protocol::schema::v1 as acp;
use agent_settings::{AgentSettings, UserAgentsMd};
use agent_ui::terminal_thread_metadata_store::{
    TerminalThreadMetadata, TerminalThreadMetadataStore, terminal_title_prefix,
};
use agent_ui::thread_metadata_store::{
    ThreadMetadata, ThreadMetadataStore, WorktreePaths, worktree_info_from_thread_paths,
};
use agent_ui::threads_archive_view::{
    ThreadsArchiveView, ThreadsArchiveViewEvent, format_history_entry_timestamp,
    fuzzy_match_positions,
};
use agent_ui::{
    AcpThreadImportOnboarding, AddContextServer, Agent, AgentPanel, AgentPanelEvent,
    AgentThreadItem, AgentThreadSource, ArchiveSelectedThread, ConversationView,
    CrossChannelImportOnboarding, DEFAULT_THREAD_TITLE, ManageProfiles, NewTerminalThread,
    NewThread, RenameSelectedThread, TerminalId, ThreadId, ThreadImportModal,
    ThreadTitleRegenerationResult, ToggleOptionsMenu, channels_with_threads,
    connection_store_for_project, create_agent_thread_in_workspace,
    import_threads_from_other_channels, open_agent_thread_in_workspace,
};
use agent_ui::{MessageEditorEvent, StateChange, thread_worktree_archive};
use chrono::{DateTime, Utc};
use editor::Editor;
use feature_flags::{
    AgentThreadWorktreeLabel, AgentThreadWorktreeLabelFlag, FeatureFlag, FeatureFlagAppExt as _,
};
use gpui::{
    Action as _, AnyElement, App, ClickEvent, Context, DismissEvent, Entity, EntityId, FocusHandle,
    Focusable, KeyContext, ListState, Modifiers, Pixels, Render, SharedString, Task, TaskExt,
    WeakEntity, Window, WindowBackgroundAppearance, WindowHandle, linear_color_stop,
    linear_gradient, list, prelude::*, px,
};
use itertools::Itertools;
use language_model::LanguageModelRegistry;
use menu::{
    Cancel, Confirm, SelectChild, SelectFirst, SelectLast, SelectNext, SelectParent, SelectPrevious,
};
use notifications::status_toast::StatusToast;
use project::{AgentId, AgentRegistryStore, Event as ProjectEvent, WorktreeId};
use recent_projects::sidebar_recent_projects::SidebarRecentProjects;
use remote::{RemoteConnectionOptions, same_remote_connection_identity};
use serde::{Deserialize, Serialize};
use settings::Settings as _;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use theme::ActiveTheme;
use ui::{
    AgentThreadStatus, CommonAnimationExt, ContextMenu, ContextMenuEntry, GradientFade,
    HighlightedLabel, KeyBinding, PopoverMenu, PopoverMenuHandle, ProjectEmptyState, ScrollAxes,
    Scrollbars, Tab, ThreadItem, ThreadItemWorktreeInfo, TintColor, Tooltip, WithScrollbar,
    prelude::*, render_modifiers, right_click_menu,
};
use unicode_segmentation::UnicodeSegmentation as _;
use util::ResultExt as _;
use util::path_list::PathList;
use workspace::{
    CloseWindow, MultiWorkspace, MultiWorkspaceEvent, NextProject, NextThread, Open, OpenMode,
    PreviousProject, PreviousThread, ProjectGroupKey, SaveIntent, Sidebar as WorkspaceSidebar,
    SidebarRenderState, SidebarSettings, SidebarSide, Toast, ToggleSidebar, Workspace,
    notifications::NotificationId, render_sidebar_header_controls_with_state,
};

use git_ui::worktree_service::{RemoteBranchName, worktree_create_targets};
use mav_actions::agent::OpenSettings;
use mav_actions::assistant::{ManageSkills, OpenGlobalAgentsMdRules, OpenProjectAgentsMdRules};
use mav_actions::editor::{MoveDown, MoveUp};
use mav_actions::{CreateWorktree, NewWorktreeBranchTarget, OpenRecent};

use mav_actions::sidebar::{FocusSidebarFilter, ToggleThreadSwitcher};

use workspace_menu::*;

use crate::thread_switcher::{
    ThreadSwitcher, ThreadSwitcherEntry, ThreadSwitcherEvent, ThreadSwitcherSelection,
    ThreadSwitcherTerminalEntry, ThreadSwitcherThreadEntry,
};

#[cfg(test)]
mod sidebar_tests;

gpui::actions!(
    sidebar,
    [
        /// Creates a new thread in the currently selected or active project group.
        NewThreadInGroup,
        /// Toggles between the thread list and the thread history.
        ToggleThreadHistory,
    ]
);

gpui::actions!(
    dev,
    [
        /// Dumps multi-workspace state (projects, worktrees, active threads) into a new buffer.
        DumpWorkspaceInfo,
    ]
);

const DEFAULT_WIDTH: Pixels = px(300.0);
const MIN_WIDTH: Pixels = px(200.0);
const MAX_WIDTH: Pixels = px(800.0);

#[derive(Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum SerializedSidebarView {
    #[default]
    ThreadList,
    #[serde(alias = "Archive")]
    History,
}

#[derive(Clone, Copy)]
enum NewEntryTarget {
    LastCreatedKind,
    Terminal,
}

#[derive(Default, Serialize, Deserialize)]
struct SerializedSidebar {
    #[serde(default)]
    width: Option<f32>,
    #[serde(default)]
    active_view: SerializedSidebarView,
}

#[derive(Debug, Default)]
enum SidebarView {
    #[default]
    ThreadList,
    Archive(Entity<ThreadsArchiveView>),
}

enum ArchiveWorktreeOutcome {
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
    multi_workspace: WeakEntity<MultiWorkspace>,
    width: Pixels,
    focus_handle: FocusHandle,
    filter_editor: Entity<Editor>,
    thread_rename_editor: Entity<Editor>,
    list_state: ListState,
    contents: SidebarContents,
    /// The index of the list item that currently has the keyboard focus
    ///
    /// Note: This is NOT the same as the active item.
    selection: Option<usize>,
    /// Tracks which sidebar entry is currently active (highlighted).
    active_entry: Option<ActiveEntry>,
    hovered_thread_index: Option<usize>,
    renaming_thread_id: Option<ThreadId>,
    /// Threads in the database-backed regeneration path need their own loading
    /// state because they do not have a live `agent::Thread` to report it.
    regenerating_titles: HashSet<ThreadId>,
    /// start_renaming_thread must seed current title into the title editor
    /// so this prevents that BufferEdited event from being interpreted as user input.
    suppress_next_rename_edit: bool,

    /// Updated only in response to explicit user actions (clicking a
    /// thread, confirming in the thread switcher, etc.) — never from
    /// background data changes. Used to sort the thread switcher popup.
    thread_last_accessed: HashMap<ThreadId, DateTime<Utc>>,
    terminal_last_accessed: HashMap<TerminalId, DateTime<Utc>>,
    thread_switcher: Option<Entity<ThreadSwitcher>>,
    _thread_switcher_subscriptions: Vec<gpui::Subscription>,
    pending_thread_activation: Option<agent_ui::ThreadId>,
    /// Persists live thread statuses across rebuilds so that Running→Completed
    /// transitions can be detected even when the group is collapsed (and
    /// thread entries are not present in the list).
    live_thread_statuses: HashMap<acp::SessionId, (AgentThreadStatus, ThreadId)>,
    /// Remembers whether each draft last rendered as empty or with content so
    /// that when a draft that was empty gains content again, we refresh
    /// its interaction time.
    draft_kinds: HashMap<ThreadId, DraftKind>,
    view: SidebarView,
    restoring_tasks: HashMap<agent_ui::ThreadId, Task<()>>,
    agent_options_menu_handle: PopoverMenuHandle<ContextMenu>,
    recent_projects_popover_handle: PopoverMenuHandle<SidebarRecentProjects>,
    sidebar_chrome: Entity<title_bar::SidebarChrome>,
    project_header_menu_handles: HashMap<usize, PopoverMenuHandle<ContextMenu>>,
    project_header_new_thread_menu_handles: HashMap<usize, PopoverMenuHandle<ContextMenu>>,
    project_header_menu_ix: Option<usize>,
    worktree_default_branches: HashMap<ProjectGroupKey, DefaultBranchCache>,
    _subscriptions: Vec<gpui::Subscription>,
    _draft_editor_observations: Vec<gpui::Subscription>,
    update_task: Option<Task<()>>,
    /// For the thread import banners, if there is just one we show "Import
    /// Threads" but if we are showing both the external agents and other
    /// channels import banners then we change the text to disambiguate the
    /// buttons. This field tracks whether we were using verbose labels so they
    /// can stay stable after dismissing one of the banners.
    import_banners_use_verbose_labels: Option<bool>,
    /// Display names of other release channels that have threads available to
    /// import.
    cross_channel_import_channels: Vec<SharedString>,
}

impl Sidebar {
    pub fn new(
        multi_workspace: Entity<MultiWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus_in(&focus_handle, window, Self::focus_in)
            .detach();

        AgentThreadWorktreeLabelFlag::watch(cx);

        let filter_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Search threads…", window, cx);
            editor
        });
        let thread_rename_editor = cx.new(|cx| Editor::single_line(window, cx));
        let sidebar_chrome = cx.new(|cx| {
            let workspace = multi_workspace.read(cx).workspace().clone();
            title_bar::SidebarChrome::new(
                "sidebar-title-bar-controls",
                workspace,
                Some(multi_workspace.downgrade()),
                window,
                cx,
            )
        });

        cx.subscribe_in(
            &multi_workspace,
            window,
            |this, _multi_workspace, event: &MultiWorkspaceEvent, window, cx| match event {
                MultiWorkspaceEvent::ActiveWorkspaceChanged { .. } => {
                    let workspace = _multi_workspace.read(cx).workspace().clone();
                    this.sidebar_chrome = cx.new(|cx| {
                        title_bar::SidebarChrome::new(
                            "sidebar-title-bar-controls",
                            workspace,
                            Some(_multi_workspace.downgrade()),
                            window,
                            cx,
                        )
                    });
                    this.sync_active_entry_from_active_workspace(cx);
                    this.replace_archived_panel_thread(window, cx);
                    this.schedule_update_entries(false, cx);
                }
                MultiWorkspaceEvent::WorkspaceAdded(workspace) => {
                    this.subscribe_to_workspace(workspace, window, cx);
                    this.schedule_update_entries(false, cx);
                }
                MultiWorkspaceEvent::WorkspaceRemoved(_)
                | MultiWorkspaceEvent::ProjectGroupsChanged => {
                    this.schedule_update_entries(false, cx);
                }
            },
        )
        .detach();

        cx.subscribe(&filter_editor, |this: &mut Self, _, event, cx| {
            if let editor::EditorEvent::BufferEdited = event {
                let query = this.filter_editor.read(cx).text(cx);
                if !query.is_empty() {
                    this.selection.take();
                }
                this.schedule_update_entries(!query.is_empty(), cx);
            }
        })
        .detach();

        cx.subscribe_in(
            &thread_rename_editor,
            window,
            |this, title_editor, event, window, cx| {
                this.handle_thread_rename_editor_event(title_editor, event, window, cx);
            },
        )
        .detach();

        cx.observe(&ThreadMetadataStore::global(cx), |this, _store, cx| {
            this.schedule_update_entries(false, cx);
        })
        .detach();

        cx.observe(
            &TerminalThreadMetadataStore::global(cx),
            |this, _store, cx| {
                this.schedule_update_entries(false, cx);
            },
        )
        .detach();

        let channels_with_threads = channels_with_threads(cx);
        cx.spawn(async move |this, cx| {
            let channels = channels_with_threads.await;
            this.update(cx, |this, cx| {
                this.cross_channel_import_channels = channels;
                cx.notify();
            })
            .ok();
        })
        .detach();

        let deferred_multi_workspace = multi_workspace.downgrade();
        cx.defer_in(window, move |this, window, cx| {
            if let Some(multi_workspace) = deferred_multi_workspace.upgrade() {
                let workspaces: Vec<_> = multi_workspace.read(cx).workspaces().cloned().collect();
                for workspace in &workspaces {
                    this.subscribe_to_workspace(workspace, window, cx);
                }
            }
            this.schedule_update_entries(false, cx);
        });

        Self {
            multi_workspace: multi_workspace.downgrade(),
            width: DEFAULT_WIDTH,
            focus_handle,
            filter_editor,
            thread_rename_editor,
            list_state: ListState::new(0, gpui::ListAlignment::Top, px(1000.)),
            contents: SidebarContents::default(),
            selection: None,
            active_entry: None,
            hovered_thread_index: None,
            renaming_thread_id: None,
            regenerating_titles: HashSet::new(),
            suppress_next_rename_edit: false,

            thread_last_accessed: HashMap::new(),
            terminal_last_accessed: HashMap::new(),
            thread_switcher: None,
            _thread_switcher_subscriptions: Vec::new(),
            pending_thread_activation: None,
            live_thread_statuses: HashMap::new(),
            draft_kinds: HashMap::new(),
            view: SidebarView::default(),
            restoring_tasks: HashMap::new(),
            agent_options_menu_handle: PopoverMenuHandle::default(),
            recent_projects_popover_handle: PopoverMenuHandle::default(),
            sidebar_chrome,
            project_header_menu_handles: HashMap::new(),
            project_header_new_thread_menu_handles: HashMap::new(),
            project_header_menu_ix: None,
            worktree_default_branches: HashMap::new(),
            _subscriptions: Vec::new(),
            _draft_editor_observations: Vec::new(),
            update_task: None,
            import_banners_use_verbose_labels: None,
            cross_channel_import_channels: Vec::new(),
        }
    }

    fn serialize(&mut self, cx: &mut Context<Self>) {
        cx.emit(workspace::SidebarEvent::SerializeNeeded);
    }

    fn is_group_collapsed(&self, key: &ProjectGroupKey, cx: &App) -> bool {
        self.multi_workspace
            .upgrade()
            .and_then(|mw| {
                mw.read(cx)
                    .group_state_by_key(key)
                    .map(|state| !state.expanded)
            })
            .unwrap_or(false)
    }

    fn set_group_expanded(&self, key: &ProjectGroupKey, expanded: bool, cx: &mut Context<Self>) {
        if let Some(mw) = self.multi_workspace.upgrade() {
            mw.update(cx, |mw, cx| {
                if let Some(state) = mw.group_state_by_key_mut(key) {
                    state.expanded = expanded;
                }
                mw.serialize(cx);
            });
        }
    }

    /// Rebuilds the sidebar contents from current workspace and thread state.
    ///
    /// Iterates [`MultiWorkspace::project_group_keys`] to determine project
    /// groups, then populates thread entries from the metadata store and
    /// merges live thread info from active agent panels.
    ///
    /// Aim for a single forward pass over workspaces and threads plus an
    /// O(T log T) sort. Avoid adding extra scans over the data.
    ///
    /// Properties:
    ///
    /// - Should always show every workspace in the multiworkspace
    ///     - If you have no threads, and two workspaces for the worktree and the main workspace, make sure at least one is shown
    /// - Should always show every thread, associated with each workspace in the multiworkspace
    /// - After every build_contents, our "active" state should exactly match the current workspace's, current agent panel's current thread.
    fn rebuild_contents(&mut self, cx: &App) {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };
        let mw = multi_workspace.read(cx);
        let workspaces: Vec<_> = mw.workspaces().cloned().collect();
        let active_workspace = Some(mw.workspace().clone());

        let agent_server_store = workspaces
            .first()
            .map(|ws| ws.read(cx).project().read(cx).agent_server_store().clone());

        let query = "";

        let previous = mem::take(&mut self.contents);

        let old_statuses = &self.live_thread_statuses;

        let mut entries = Vec::new();
        let mut notified_threads = previous.notified_threads;
        let mut notified_terminals: HashSet<TerminalId> = HashSet::new();
        let mut new_live_statuses: HashMap<acp::SessionId, (AgentThreadStatus, ThreadId)> =
            HashMap::new();
        let mut current_session_ids: HashSet<acp::SessionId> = HashSet::new();
        let mut current_thread_ids: HashSet<agent_ui::ThreadId> = HashSet::new();
        let mut current_terminal_ids: HashSet<TerminalId> = HashSet::new();
        let mut project_header_indices: Vec<usize> = Vec::new();
        let mut seen_thread_ids: HashSet<agent_ui::ThreadId> = HashSet::new();
        let mut seen_terminal_ids: HashSet<TerminalId> = HashSet::new();

        let has_open_projects = workspaces
            .iter()
            .any(|ws| !workspace_path_list(ws, cx).paths().is_empty());

        let resolve_agent_icon = |agent_id: &AgentId| -> (IconName, Option<SharedString>) {
            let agent = Agent::from(agent_id.clone());
            let icon = match agent {
                Agent::NativeAgent => IconName::MavAgent,
                Agent::Custom { .. } => IconName::Terminal,

                _ => IconName::MavAgent,
            };
            let icon_from_external_svg = agent_server_store
                .as_ref()
                .and_then(|store| store.read(cx).agent_icon(&agent_id));
            (icon, icon_from_external_svg)
        };

        let groups = mw.project_groups(cx);
        let mut live_notified_terminal_ids: HashSet<TerminalId> = HashSet::new();
        for workspace in &workspaces {
            if let Some(agent_panel) = workspace.read(cx).panel::<AgentPanel>(cx) {
                live_notified_terminal_ids.extend(
                    agent_panel
                        .read(cx)
                        .terminals(cx)
                        .into_iter()
                        .filter_map(|terminal| terminal.has_notification.then_some(terminal.id)),
                );
            }
        }

        let mut all_paths: Vec<PathBuf> = groups
            .iter()
            .flat_map(|group| group.key.path_list().paths().iter().cloned())
            .collect();
        all_paths.sort_unstable();
        all_paths.dedup();
        let path_details =
            util::disambiguate::compute_disambiguation_details(&all_paths, |path, detail| {
                project::path_suffix(path, detail)
            });
        let path_detail_map: HashMap<PathBuf, usize> =
            all_paths.into_iter().zip(path_details).collect();

        let mut branch_by_path: HashMap<PathBuf, SharedString> = HashMap::new();
        for ws in &workspaces {
            let project = ws.read(cx).project().read(cx);
            for repo in project.repositories(cx).values() {
                let snapshot = repo.read(cx).snapshot();
                if let Some(branch) = &snapshot.branch {
                    branch_by_path.insert(
                        snapshot.work_directory_abs_path.to_path_buf(),
                        SharedString::from(Arc::<str>::from(branch.name())),
                    );
                }
                for linked_wt in snapshot.linked_worktrees() {
                    if let Some(branch) = linked_wt.branch_name() {
                        branch_by_path.insert(
                            linked_wt.path.clone(),
                            SharedString::from(Arc::<str>::from(branch)),
                        );
                    }
                }
            }
        }

        for group in &groups {
            let group_key = &group.key;
            let group_workspaces = &group.workspaces;

            let workspace_by_path_list: HashMap<PathList, &Entity<Workspace>> = group_workspaces
                .iter()
                .map(|ws| (workspace_path_list(ws, cx), ws))
                .collect();
            let resolve_workspace = |folder_paths: &PathList| -> ThreadEntryWorkspace {
                workspace_by_path_list
                    .get(folder_paths)
                    .map(|ws| ThreadEntryWorkspace::Open((*ws).clone()))
                    .unwrap_or_else(|| ThreadEntryWorkspace::Closed {
                        folder_paths: folder_paths.clone(),
                        project_group_key: group_key.clone(),
                    })
            };
            let linked_worktree_path_lists =
                linked_worktree_path_lists_for_workspaces(group_workspaces, cx);
            let make_terminal_entry =
                |metadata: TerminalThreadMetadata, workspace: ThreadEntryWorkspace| {
                    let worktrees =
                        worktree_info_from_thread_paths(&metadata.worktree_paths, &branch_by_path);
                    let has_notification =
                        live_notified_terminal_ids.contains(&metadata.terminal_id);
                    TerminalEntry {
                        metadata,
                        workspace,
                        worktrees,
                        has_notification,
                        highlight_positions: Vec::new(),
                    }
                };

            let mut terminals = Vec::new();
            let terminal_store = TerminalThreadMetadataStore::global(cx);
            let group_host = group_key.host();
            let mut push_terminal_metadata =
                |metadata: TerminalThreadMetadata, workspace: ThreadEntryWorkspace| {
                    if !seen_terminal_ids.insert(metadata.terminal_id) {
                        return;
                    }
                    terminals.push(make_terminal_entry(metadata, workspace));
                };
            for row in terminal_store
                .read(cx)
                .entries_for_main_worktree_path(group_key.path_list(), group_host.as_ref())
                .cloned()
            {
                let workspace = resolve_workspace(row.folder_paths());
                push_terminal_metadata(row, workspace);
            }
            for row in terminal_store
                .read(cx)
                .entries_for_path(group_key.path_list(), group_host.as_ref())
                .cloned()
            {
                let workspace = resolve_workspace(row.folder_paths());
                push_terminal_metadata(row, workspace);
            }
            for ws in group_workspaces {
                let ws_paths = workspace_path_list(ws, cx);
                if ws_paths.paths().is_empty() {
                    continue;
                }
                for row in terminal_store
                    .read(cx)
                    .entries_for_path(&ws_paths, group_host.as_ref())
                    .cloned()
                {
                    push_terminal_metadata(row, ThreadEntryWorkspace::Open(ws.clone()));
                }
            }
            for worktree_path_list in &linked_worktree_path_lists {
                for row in terminal_store
                    .read(cx)
                    .entries_for_path(worktree_path_list, group_host.as_ref())
                    .cloned()
                {
                    push_terminal_metadata(
                        row,
                        ThreadEntryWorkspace::Closed {
                            folder_paths: worktree_path_list.clone(),
                            project_group_key: group_key.clone(),
                        },
                    );
                }
            }
            current_terminal_ids.extend(
                terminals
                    .iter()
                    .map(|terminal| terminal.metadata.terminal_id),
            );
            notified_terminals.extend(terminals.iter().filter_map(|terminal| {
                terminal
                    .has_notification
                    .then_some(terminal.metadata.terminal_id)
            }));
            if group_key.path_list().paths().is_empty() {
                continue;
            }

            let label = group_key.display_name(&path_detail_map);

            let is_collapsed = self.is_group_collapsed(group_key, cx);
            let should_load_threads = !is_collapsed || !query.is_empty();

            let is_active = active_workspace
                .as_ref()
                .is_some_and(|active| group_workspaces.contains(active));

            // Collect live thread infos from all workspaces in this group.
            let live_infos = group_workspaces
                .iter()
                .flat_map(|ws| all_thread_infos_for_workspace(ws, cx));

            let mut threads: Vec<Arc<ThreadEntry>> = Vec::new();
            let mut has_running_threads = false;
            let mut waiting_thread_count: usize = 0;
            let group_host = group_key.host();

            if should_load_threads {
                let thread_store = ThreadMetadataStore::global(cx);

                let make_thread_entry =
                    |row: ThreadMetadata, workspace: ThreadEntryWorkspace| -> Arc<ThreadEntry> {
                        let (icon, icon_from_external_svg) = resolve_agent_icon(&row.agent_id);
                        let worktrees =
                            worktree_info_from_thread_paths(&row.worktree_paths, &branch_by_path);
                        Arc::new(ThreadEntry {
                            metadata: row,
                            icon,
                            icon_from_external_svg,
                            status: AgentThreadStatus::default(),
                            workspace,
                            is_live: false,
                            is_background: false,
                            is_title_generating: false,
                            draft: None,
                            highlight_positions: Vec::new(),
                            worktrees,
                            diff_stats: DiffStats::default(),
                        })
                    };

                // Main code path: one query per group via main_worktree_paths.
                // The main_worktree_paths column is set on all new threads and
                // points to the group's canonical paths regardless of which
                // linked worktree the thread was opened in.
                for row in thread_store
                    .read(cx)
                    .entries_for_main_worktree_path(group_key.path_list(), group_host.as_ref())
                    .cloned()
                {
                    if row.is_draft() {
                        continue;
                    }
                    if !seen_thread_ids.insert(row.thread_id) {
                        continue;
                    }
                    let workspace = resolve_workspace(row.folder_paths());
                    threads.push(make_thread_entry(row, workspace));
                }

                // Legacy threads did not have `main_worktree_paths` populated, so they
                // must be queried by their `folder_paths`.

                // Load any legacy threads for the main worktrees of this project group.
                for row in thread_store
                    .read(cx)
                    .entries_for_path(group_key.path_list(), group_host.as_ref())
                    .cloned()
                {
                    if row.is_draft() {
                        continue;
                    }
                    if !seen_thread_ids.insert(row.thread_id) {
                        continue;
                    }
                    let workspace = resolve_workspace(row.folder_paths());
                    threads.push(make_thread_entry(row, workspace));
                }

                // Also surface any thread whose `folder_paths` equals
                // one of this group's open workspaces' root paths.
                // The three lookups above can all miss when the
                // thread's stored `main_worktree_paths` disagree with
                // the group key (for example, a stale row whose main
                // paths equal its folder paths for a linked-worktree
                // workspace). The thread will be rewritten into the
                // correct shape the next time `handle_conversation_event`
                // fires, but until then the sidebar should still show
                // it under the group whose workspace it actually
                // belongs to.
                for ws in group_workspaces {
                    let ws_paths = workspace_path_list(ws, cx);
                    if ws_paths.paths().is_empty() {
                        continue;
                    }
                    for row in thread_store
                        .read(cx)
                        .entries_for_path(&ws_paths, group_host.as_ref())
                        .cloned()
                    {
                        if row.is_draft() {
                            continue;
                        }
                        if !seen_thread_ids.insert(row.thread_id) {
                            continue;
                        }
                        threads.push(make_thread_entry(
                            row,
                            ThreadEntryWorkspace::Open(ws.clone()),
                        ));
                    }
                }

                // Load any legacy threads for any single linked worktree of this project group.
                for worktree_path_list in &linked_worktree_path_lists {
                    for row in thread_store
                        .read(cx)
                        .entries_for_path(worktree_path_list, group_host.as_ref())
                        .cloned()
                    {
                        if row.is_draft() {
                            continue;
                        }
                        if !seen_thread_ids.insert(row.thread_id) {
                            continue;
                        }
                        threads.push(make_thread_entry(
                            row,
                            ThreadEntryWorkspace::Closed {
                                folder_paths: worktree_path_list.clone(),
                                project_group_key: group_key.clone(),
                            },
                        ));
                    }
                }

                for thread in &mut threads {
                    if thread.draft.is_none() {
                        continue;
                    }
                    if let Some((label, kind)) = draft_display_label_for_thread_metadata(
                        &thread.metadata,
                        &thread.workspace,
                        cx,
                    ) {
                        let thread = Arc::make_mut(thread);
                        thread.metadata.title = Some(label);
                        thread.draft = Some(kind);
                    }
                }
                threads.retain(|thread| thread.draft.is_none() || thread.metadata.title.is_some());

                // Keep empty drafts only while their thread is active; preserve
                // drafts with content because they hold user-typed state.
                let pending_activation = self.pending_thread_activation;
                let active_panel_thread_id = active_workspace
                    .as_ref()
                    .and_then(|ws| ws.read(cx).panel::<AgentPanel>(cx))
                    .and_then(|panel| panel.read(cx).active_thread_id(cx));
                threads.retain(|thread| {
                    if thread.draft != Some(DraftKind::Empty) {
                        return true;
                    }
                    if pending_activation.is_some() {
                        return false;
                    }
                    Some(thread.metadata.thread_id) == active_panel_thread_id
                });

                // Build a lookup from live_infos and compute running/waiting
                // counts in a single pass.
                let mut live_info_by_session: HashMap<acp::SessionId, ActiveThreadInfo> =
                    HashMap::new();
                for info in live_infos {
                    if info.status == AgentThreadStatus::Running {
                        has_running_threads = true;
                    }
                    if info.status == AgentThreadStatus::WaitingForConfirmation {
                        waiting_thread_count += 1;
                    }
                    live_info_by_session.insert(info.session_id.clone(), info);
                }

                // Merge live info into threads and update notification state
                // in a single pass.
                for thread in &mut threads {
                    if let Some(session_id) = thread.metadata.session_id.clone() {
                        if let Some(info) = live_info_by_session.get(&session_id) {
                            let status = info.status;
                            let thread_id = thread.metadata.thread_id;
                            Arc::make_mut(thread).apply_active_info(info);
                            new_live_statuses.insert(session_id, (status, thread_id));
                        }
                    }

                    let session_id = &thread.metadata.session_id;
                    let is_active_thread = self.active_entry.as_ref().is_some_and(|entry| {
                        entry.is_active_thread(&thread.metadata.thread_id)
                            && active_workspace
                                .as_ref()
                                .is_some_and(|active| active == entry.workspace())
                    });

                    if thread.status == AgentThreadStatus::Completed
                        && !is_active_thread
                        && session_id
                            .as_ref()
                            .and_then(|sid| old_statuses.get(sid))
                            .is_some_and(|(s, _)| *s == AgentThreadStatus::Running)
                    {
                        notified_threads.insert(thread.metadata.thread_id);
                    }

                    if is_active_thread && !thread.is_background {
                        notified_threads.remove(&thread.metadata.thread_id);
                    }
                }

                threads.sort_by(|a, b| {
                    let a_time = Self::thread_display_time(&a.metadata);
                    let b_time = Self::thread_display_time(&b.metadata);
                    b_time.cmp(&a_time)
                });
            } else {
                for info in live_infos {
                    if info.status == AgentThreadStatus::Running {
                        has_running_threads = true;
                    }
                    if info.status == AgentThreadStatus::WaitingForConfirmation {
                        waiting_thread_count += 1;
                    }
                    // Resolve the thread_id for this session so we can
                    // track its status and detect transitions even while
                    // the group is collapsed.
                    let thread_id = old_statuses
                        .get(&info.session_id)
                        .map(|(_, tid)| *tid)
                        .or_else(|| {
                            ThreadMetadataStore::global(cx)
                                .read(cx)
                                .entry_by_session(&info.session_id)
                                .map(|m| m.thread_id)
                        });

                    if let Some(thread_id) = thread_id {
                        let old_status = old_statuses.get(&info.session_id).map(|(s, _)| *s);
                        new_live_statuses.insert(info.session_id.clone(), (info.status, thread_id));
                        if info.status == AgentThreadStatus::Completed
                            && old_status == Some(AgentThreadStatus::Running)
                        {
                            notified_threads.insert(thread_id);
                        }
                    }
                }

                if is_active
                    && let Some(ActiveEntry::Thread { thread_id, .. }) = self.active_entry.as_ref()
                {
                    notified_threads.remove(thread_id);
                }
            }

            let has_visible_rows = !threads.is_empty() || !terminals.is_empty();
            let has_stored_thread_rows = !should_load_threads && !has_visible_rows && {
                let store = ThreadMetadataStore::global(cx).read(cx);
                store
                    .entries_for_main_worktree_path(group_key.path_list(), group_host.as_ref())
                    .any(|metadata| {
                        let workspace = resolve_workspace(metadata.folder_paths());
                        thread_metadata_would_render_sidebar_row(metadata, &workspace, cx)
                    })
                    || store
                        .entries_for_path(group_key.path_list(), group_host.as_ref())
                        .any(|metadata| {
                            let workspace = resolve_workspace(metadata.folder_paths());
                            thread_metadata_would_render_sidebar_row(metadata, &workspace, cx)
                        })
            };
            let has_threads = has_visible_rows || has_stored_thread_rows;

            if !query.is_empty() {
                let workspace_highlight_positions =
                    fuzzy_match_positions(&query, &label).unwrap_or_default();
                let workspace_matched = !workspace_highlight_positions.is_empty();

                let mut matched_threads: Vec<Arc<ThreadEntry>> = Vec::new();
                for mut thread in threads {
                    let mut worktree_matched = false;
                    {
                        let thread = Arc::make_mut(&mut thread);
                        let title = thread.metadata.display_title();
                        if let Some(positions) = fuzzy_match_positions(&query, title.as_ref()) {
                            thread.highlight_positions = positions;
                        }
                        for worktree in &mut thread.worktrees {
                            let Some(name) = worktree.worktree_name.as_ref() else {
                                continue;
                            };
                            if let Some(positions) = fuzzy_match_positions(&query, name) {
                                worktree.highlight_positions = positions;
                                worktree_matched = true;
                            }
                        }
                    }
                    if workspace_matched
                        || !thread.highlight_positions.is_empty()
                        || worktree_matched
                    {
                        matched_threads.push(thread);
                    }
                }

                let mut matched_terminals: Vec<TerminalEntry> = Vec::new();
                for mut terminal in terminals {
                    let mut terminal_matched = false;
                    let terminal_title = terminal.metadata.display_title();
                    if let Some(positions) = fuzzy_match_positions(&query, terminal_title.as_ref())
                    {
                        terminal.highlight_positions = positions;
                        terminal_matched = true;
                    }
                    let mut worktree_matched = false;
                    for worktree in &mut terminal.worktrees {
                        let Some(name) = worktree.worktree_name.as_ref() else {
                            continue;
                        };
                        if let Some(positions) = fuzzy_match_positions(&query, name) {
                            worktree.highlight_positions = positions;
                            worktree_matched = true;
                        }
                    }
                    if workspace_matched || terminal_matched || worktree_matched {
                        matched_terminals.push(terminal);
                    }
                }

                if matched_threads.is_empty() && matched_terminals.is_empty() && !workspace_matched
                {
                    continue;
                }

                // Check for notifications: threads that completed while not active.
                let has_thread_notifications = matched_threads
                    .iter()
                    .any(|t| notified_threads.contains(&t.metadata.thread_id));
                let has_terminal_notifications = matched_terminals
                    .iter()
                    .any(|t| notified_terminals.contains(&t.metadata.terminal_id));

                project_header_indices.push(entries.len());
                entries.push(ListEntry::ProjectHeader {
                    key: group_key.clone(),
                    label,
                    highlight_positions: workspace_highlight_positions,
                    has_running_threads,
                    waiting_thread_count,
                    has_notifications: has_thread_notifications || has_terminal_notifications,
                    is_active,
                    has_threads,
                });

                Self::push_entries_by_display_time(
                    &mut entries,
                    matched_terminals,
                    matched_threads,
                    &mut current_session_ids,
                    &mut current_thread_ids,
                );
            } else {
                let has_terminal_notifications = terminals
                    .iter()
                    .any(|t| notified_terminals.contains(&t.metadata.terminal_id));

                // When collapsed, threads aren't loaded into `threads`, so we
                // query the store for thread IDs to check notifications and
                // to prevent the retain below from purging them.
                let has_thread_notifications = if threads.is_empty() && !notified_threads.is_empty()
                {
                    let thread_store = ThreadMetadataStore::global(cx);
                    let store = thread_store.read(cx);
                    let group_thread_ids = store
                        .entries_for_main_worktree_path(group_key.path_list(), group_host.as_ref())
                        .chain(store.entries_for_path(group_key.path_list(), group_host.as_ref()))
                        .map(|m| m.thread_id)
                        .collect::<HashSet<_>>();
                    current_thread_ids.extend(group_thread_ids.iter());
                    group_thread_ids
                        .iter()
                        .any(|id| notified_threads.contains(id))
                } else {
                    threads
                        .iter()
                        .any(|t| notified_threads.contains(&t.metadata.thread_id))
                };

                project_header_indices.push(entries.len());
                entries.push(ListEntry::ProjectHeader {
                    key: group_key.clone(),
                    label,
                    highlight_positions: Vec::new(),
                    has_running_threads,
                    waiting_thread_count,
                    has_notifications: has_thread_notifications || has_terminal_notifications,
                    is_active,
                    has_threads,
                });

                if is_collapsed {
                    continue;
                }

                Self::push_entries_by_display_time(
                    &mut entries,
                    terminals,
                    threads,
                    &mut current_session_ids,
                    &mut current_thread_ids,
                );
            }
        }

        notified_threads.retain(|id| current_thread_ids.contains(id));

        self.thread_last_accessed
            .retain(|id, _| current_thread_ids.contains(id));
        self.terminal_last_accessed
            .retain(|id, _| current_terminal_ids.contains(id));

        self.live_thread_statuses = new_live_statuses;

        self.contents = SidebarContents {
            entries,
            notified_threads,
            notified_terminals,
            project_header_indices,
            has_open_projects,
        };
    }

    fn rename_selected_thread(
        &mut self,
        _: &RenameSelectedThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else {
            return;
        };
        let Some(ListEntry::Thread(thread)) = self.contents.entries.get(ix) else {
            return;
        };
        let thread_id = thread.metadata.thread_id;
        let title = thread.metadata.display_title();
        self.start_renaming_thread(ix, thread_id, title, window, cx);
    }

    fn render_recent_projects_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let multi_workspace = self.multi_workspace.upgrade();

        let workspace = multi_workspace
            .as_ref()
            .map(|mw| mw.read(cx).workspace().downgrade());

        let focus_handle = workspace
            .as_ref()
            .and_then(|ws| ws.upgrade())
            .map(|w| w.read(cx).focus_handle(cx))
            .unwrap_or_else(|| cx.focus_handle());

        let window_project_groups: Vec<ProjectGroupKey> = multi_workspace
            .as_ref()
            .map(|mw| mw.read(cx).project_group_keys())
            .unwrap_or_default();

        let popover_handle = self.recent_projects_popover_handle.clone();

        PopoverMenu::new("sidebar-recent-projects-menu")
            .with_handle(popover_handle)
            .menu(move |window, cx| {
                workspace.as_ref().map(|ws| {
                    SidebarRecentProjects::popover(
                        ws.clone(),
                        window_project_groups.clone(),
                        focus_handle.clone(),
                        window,
                        cx,
                    )
                })
            })
            .trigger_with_tooltip(
                IconButton::new("open-project", IconName::FolderAdd)
                    .icon_size(IconSize::Small)
                    .selected_style(ButtonStyle::Tinted(TintColor::Accent)),
                |_window, cx| Tooltip::for_action("Add Project", &OpenRecent::default(), cx),
            )
            .offset(gpui::Point {
                x: px(-2.0),
                y: px(-2.0),
            })
            .anchor(gpui::Anchor::BottomRight)
    }

    fn new_thread_in_group(
        &mut self,
        _: &NewThreadInGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = self.selected_group_key() {
            self.set_group_expanded(&key, true, cx);
            self.selection = None;
            if let Some(workspace) = self.workspace_for_group(&key, cx) {
                self.create_new_entry(&workspace, window, cx);
            } else {
                self.open_workspace_and_create_entry(
                    &key,
                    NewEntryTarget::LastCreatedKind,
                    window,
                    cx,
                );
            }
        } else if let Some(workspace) = self.active_workspace(cx) {
            self.create_new_entry(&workspace, window, cx);
        }
    }

    fn new_terminal_thread(
        &mut self,
        _: &NewTerminalThread,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();

        if let Some(key) = self.selected_group_key() {
            self.set_group_expanded(&key, true, cx);
            self.selection = None;
            if let Some(workspace) = self.workspace_for_group(&key, cx) {
                self.create_new_terminal(&workspace, window, cx);
            } else {
                self.open_workspace_and_create_entry(&key, NewEntryTarget::Terminal, window, cx);
            }
        } else if let Some(workspace) = self.active_workspace(cx) {
            self.create_new_terminal(&workspace, window, cx);
        }
    }
}
