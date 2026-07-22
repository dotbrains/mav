mod dev_container_suggest;
pub mod disconnected_overlay;
mod remote_connections;
mod remote_servers;
pub mod sidebar_recent_projects;
mod ssh_config;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};

use fs::Fs;

mod delegate_actions;
mod delegate_confirm;
mod delegate_helpers;
mod delegate_matching;
mod delegate_render_footer;
mod delegate_render_match;
mod delegate_state;
mod init_actions;
mod modal;
mod tests;
#[cfg(target_os = "windows")]
mod wsl_picker;

use remote::RemoteConnectionOptions;
pub use remote_connection::{RemoteConnectionModal, connect, connect_with_modal};
pub use remote_connections::{navigate_to_positions, open_remote_project};

use disconnected_overlay::DisconnectedOverlay;
use fuzzy_nucleo::{StringMatch, StringMatchCandidate, match_strings};
use gpui::{
    Action, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    Subscription, Task, TaskExt, WeakEntity, Window, actions, px,
};

use picker::{
    Picker, PickerDelegate, ScrollBehavior,
    highlighted_match_with_paths::{HighlightedMatch, HighlightedMatchWithPaths},
};
use project::{Worktree, git_store::Repository};
pub use remote_connections::RemoteSettings;
pub use remote_servers::RemoteServerProjects;
use settings::{DefaultOpenBehavior, Settings, WorktreeId};
use ui_input::ErasedEditor;
use workspace::ProjectGroupKey;

use dev_container::{DevContainerContext, find_devcontainer_configs};
use mav_actions::{OpenDevContainer, OpenRecent, OpenRemote};
use ui::{
    ButtonLike, ContextMenu, Divider, HighlightedLabel, KeyBinding, ListItem, ListItemSpacing,
    ListSubHeader, PopoverMenu, PopoverMenuHandle, TintColor, Tooltip, prelude::*,
};
use util::{ResultExt, paths::PathExt};
use workspace::{
    HistoryManager, ModalView, MultiWorkspace, OpenMode, OpenOptions, OpenVisible, PathList,
    RecentWorkspace, SerializedWorkspaceLocation, Workspace, WorkspaceDb, WorkspaceId,
    notifications::DetachAndPromptErr, with_active_or_new_workspace,
};

actions!(
    recent_projects,
    [ToggleActionsMenu, RemoveSelected, AddToWorkspace,]
);

#[derive(Clone, Debug)]
pub struct RecentProjectEntry {
    pub name: SharedString,
    pub full_path: SharedString,
    pub paths: Vec<PathBuf>,
    pub workspace_id: WorkspaceId,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug)]
struct OpenFolderEntry {
    worktree_id: WorktreeId,
    name: SharedString,
    path: PathBuf,
    branch: Option<SharedString>,
    is_active: bool,
    connection_options: Option<RemoteConnectionOptions>,
}

#[derive(Clone, Debug)]
enum ProjectPickerEntry {
    Header(SharedString),
    /// A currently open folder from the active workspace's "Current Folders" section.
    ///
    /// `index` points into `RecentProjectsDelegate::open_folders`, and `positions` stores the
    /// fuzzy-match highlight positions for rendering the folder name.
    OpenFolder {
        index: usize,
        positions: Vec<usize>,
    },
    /// A project group from the current window's "This Window" section.
    ///
    /// These entries come from `RecentProjectsDelegate::window_project_groups`, not from the
    /// recent-project database. Empty queries list every project group known to the current
    /// window; non-empty queries list matching project groups. Confirming one activates or loads
    /// that project group in the current window, while secondary confirm can move local project
    /// groups to a new window when multiple groups are available.
    ProjectGroup(StringMatch),
    /// A workspace from the recent-project database's "Recent Projects" section.
    ///
    /// The match's `candidate_id` indexes into `RecentProjectsDelegate::workspaces`. Confirming
    /// one opens that recent workspace in either the current window or a new window, depending on
    /// whether the picker was invoked for new-window behavior and whether this was a primary or
    /// secondary confirm.
    RecentProject(StringMatch),
}

fn is_selectable_entry(entry: &ProjectPickerEntry) -> bool {
    matches!(
        entry,
        ProjectPickerEntry::OpenFolder { .. }
            | ProjectPickerEntry::ProjectGroup(_)
            | ProjectPickerEntry::RecentProject(_)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectPickerStyle {
    Modal,
    Popover,
}

pub async fn get_recent_projects(
    current_workspace_id: Option<WorkspaceId>,
    limit: Option<usize>,
    fs: Arc<dyn fs::Fs>,
    db: &WorkspaceDb,
) -> Vec<RecentProjectEntry> {
    let workspaces = db
        .recent_project_workspaces(fs.as_ref())
        .await
        .unwrap_or_default();

    let filtered: Vec<_> = workspaces
        .into_iter()
        .filter(|workspace| Some(workspace.workspace_id) != current_workspace_id)
        .filter(|workspace| matches!(workspace.location, SerializedWorkspaceLocation::Local))
        .collect();

    let mut all_paths: Vec<PathBuf> = filtered
        .iter()
        .flat_map(|workspace| workspace.identity_paths.paths().iter().cloned())
        .collect();
    all_paths.sort_unstable();
    all_paths.dedup();
    let path_details =
        util::disambiguate::compute_disambiguation_details(&all_paths, |path, detail| {
            project::path_suffix(path, detail)
        });
    let path_detail_map: std::collections::HashMap<PathBuf, usize> =
        all_paths.into_iter().zip(path_details).collect();

    let entries: Vec<RecentProjectEntry> = filtered
        .into_iter()
        .map(|workspace| {
            let paths: Vec<PathBuf> = workspace.paths.paths().to_vec();
            let ordered_paths: Vec<&PathBuf> = workspace.identity_paths.ordered_paths().collect();

            let name = ordered_paths
                .iter()
                .map(|p| {
                    let detail = path_detail_map.get(*p).copied().unwrap_or(0);
                    project::path_suffix(p, detail)
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ");

            let full_path = ordered_paths
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("\n");

            RecentProjectEntry {
                name: SharedString::from(name),
                full_path: SharedString::from(full_path),
                paths,
                workspace_id: workspace.workspace_id,
                timestamp: workspace.timestamp,
            }
        })
        .collect();

    match limit {
        Some(n) => entries.into_iter().take(n).collect(),
        None => entries,
    }
}

pub async fn delete_recent_project(workspace_id: WorkspaceId, db: &WorkspaceDb) {
    let _ = db.delete_workspace_by_id(workspace_id).await;
}

fn get_open_folders(workspace: &Workspace, cx: &App) -> Vec<OpenFolderEntry> {
    let project = workspace.project().read(cx);
    let connection_options = project.remote_connection_options(cx);
    let visible_worktrees: Vec<_> = project.visible_worktrees(cx).collect();

    if visible_worktrees.len() <= 1 {
        return Vec::new();
    }

    let active_worktree_id = if let Some(repo) = project.active_repository(cx) {
        let repo = repo.read(cx);
        let repo_path = &repo.work_directory_abs_path;
        project.visible_worktrees(cx).find_map(|worktree| {
            let worktree_path = worktree.read(cx).abs_path();
            (worktree_path == *repo_path || worktree_path.starts_with(repo_path.as_ref()))
                .then(|| worktree.read(cx).id())
        })
    } else {
        project
            .visible_worktrees(cx)
            .next()
            .map(|wt| wt.read(cx).id())
    };

    let mut all_paths: Vec<PathBuf> = visible_worktrees
        .iter()
        .map(|wt| wt.read(cx).abs_path().to_path_buf())
        .collect();
    all_paths.sort_unstable();
    all_paths.dedup();
    let path_details =
        util::disambiguate::compute_disambiguation_details(&all_paths, |path, detail| {
            project::path_suffix(path, detail)
        });
    let path_detail_map: std::collections::HashMap<PathBuf, usize> =
        all_paths.into_iter().zip(path_details).collect();

    let git_store = project.git_store().read(cx);
    let repositories: Vec<_> = git_store.repositories().values().cloned().collect();

    let mut entries: Vec<OpenFolderEntry> = visible_worktrees
        .into_iter()
        .map(|worktree| {
            let worktree_ref = worktree.read(cx);
            let worktree_id = worktree_ref.id();
            let path = worktree_ref.abs_path().to_path_buf();
            let detail = path_detail_map.get(&path).copied().unwrap_or(0);
            let name = SharedString::from(project::path_suffix(&path, detail));
            let branch = get_branch_for_worktree(worktree_ref, &repositories, cx);
            let is_active = active_worktree_id == Some(worktree_id);
            OpenFolderEntry {
                worktree_id,
                name,
                path,
                branch,
                is_active,
                connection_options: connection_options.clone(),
            }
        })
        .collect();

    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

fn get_branch_for_worktree(
    worktree: &Worktree,
    repositories: &[Entity<Repository>],
    cx: &App,
) -> Option<SharedString> {
    let worktree_abs_path = worktree.abs_path();
    repositories
        .iter()
        .filter(|repo| {
            let repo_path = &repo.read(cx).work_directory_abs_path;
            *repo_path == worktree_abs_path || worktree_abs_path.starts_with(repo_path.as_ref())
        })
        .max_by_key(|repo| repo.read(cx).work_directory_abs_path.as_os_str().len())
        .and_then(|repo| {
            repo.read(cx)
                .branch
                .as_ref()
                .map(|branch| SharedString::from(branch.name().to_string()))
        })
}

pub(crate) fn default_open_in_new_window(cx: &App) -> bool {
    matches!(
        workspace::WorkspaceSettings::get_global(cx).default_open_behavior,
        DefaultOpenBehavior::NewWindow
    )
}

impl PickerDelegate for RecentProjectsDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "recent projects"
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Search projects…".into()
    }

    fn render_editor(
        &self,
        editor: &Arc<dyn ErasedEditor>,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Div {
        h_flex()
            .flex_none()
            .h_9()
            .px_2p5()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(editor.render(window, cx))
    }

    fn match_count(&self) -> usize {
        self.filtered_entries.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn can_select(&self, ix: usize, _window: &mut Window, _cx: &mut Context<Picker<Self>>) -> bool {
        matches!(
            self.filtered_entries.get(ix),
            Some(
                ProjectPickerEntry::OpenFolder { .. }
                    | ProjectPickerEntry::ProjectGroup(_)
                    | ProjectPickerEntry::RecentProject(_)
            )
        )
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> gpui::Task<()> {
        self.update_delegate_matches(query, window, cx)
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.confirm_delegate(secondary, window, cx)
    }

    fn dismissed(&mut self, _window: &mut Window, _: &mut Context<Picker<Self>>) {}

    fn no_matches_text(&self, _window: &mut Window, _cx: &mut App) -> Option<SharedString> {
        let text = if self.workspaces.is_empty() && self.open_folders.is_empty() {
            "Recently opened projects will show up here".into()
        } else {
            "No matches".into()
        };
        Some(text)
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        self.render_delegate_match(ix, selected, window, cx)
    }

    fn render_footer(
        &self,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        self.render_delegate_footer(window, cx)
    }
}
