#[path = "sidebar/action_shims.rs"]
mod action_shims;
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
#[path = "sidebar/group_state.rs"]
mod group_state;
#[path = "sidebar/import_onboarding_banner.rs"]
mod import_onboarding_banner;
#[path = "sidebar/selection_actions.rs"]
mod selection_actions;
#[path = "sidebar/sidebar_chrome_rendering.rs"]
mod sidebar_chrome_rendering;
#[path = "sidebar/sidebar_constructor.rs"]
mod sidebar_constructor;
#[path = "sidebar/sidebar_entries.rs"]
mod sidebar_entries;
#[path = "sidebar/sidebar_rebuild_helpers.rs"]
mod sidebar_rebuild_helpers;
#[path = "sidebar/sidebar_runtime.rs"]
mod sidebar_runtime;
#[path = "sidebar/sidebar_state.rs"]
mod sidebar_state;
#[path = "sidebar/workspace_subscriptions.rs"]
mod workspace_subscriptions;
use active_thread_info::all_thread_infos_for_workspace;
use import_onboarding_banner::render_import_onboarding_banner;
use sidebar_entries::*;
pub use sidebar_state::Sidebar;
use sidebar_state::*;
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
    PreviousProject, PreviousThread, ProjectGroup, ProjectGroupKey, SaveIntent,
    Sidebar as WorkspaceSidebar, SidebarRenderState, SidebarSettings, SidebarSide, Toast,
    ToggleSidebar, Workspace, notifications::NotificationId,
    render_sidebar_header_controls_with_state,
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

impl Sidebar {
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

        let groups = mw.project_groups(cx);
        let live_notified_terminal_ids =
            sidebar_rebuild_helpers::live_notified_terminal_ids(&workspaces, cx);
        let path_detail_map = sidebar_rebuild_helpers::path_detail_map(&groups);
        let branch_by_path = sidebar_rebuild_helpers::branch_by_path(&workspaces, cx);

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
            let terminals = sidebar_rebuild_helpers::terminal_entries_for_group(
                group_key,
                group_workspaces,
                &linked_worktree_path_lists,
                &branch_by_path,
                &live_notified_terminal_ids,
                &mut seen_terminal_ids,
                cx,
            );
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
            let group_host = group_key.host();

            let is_active = active_workspace
                .as_ref()
                .is_some_and(|active| group_workspaces.contains(active));

            let sidebar_rebuild_helpers::GroupThreadEntries {
                threads,
                has_running_threads,
                waiting_thread_count,
            } = sidebar_rebuild_helpers::thread_entries_for_group(
                group_key,
                group_workspaces,
                &linked_worktree_path_lists,
                should_load_threads,
                is_active,
                active_workspace.as_ref(),
                self.active_entry.as_ref(),
                self.pending_thread_activation,
                old_statuses,
                &mut new_live_statuses,
                &mut notified_threads,
                &mut seen_thread_ids,
                &branch_by_path,
                agent_server_store.as_ref(),
                cx,
            );

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
}
