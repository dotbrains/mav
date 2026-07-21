use super::*;

pub(super) fn root_repository_snapshots(
    workspace: &Entity<Workspace>,
    cx: &App,
) -> impl Iterator<Item = project::git_store::RepositorySnapshot> {
    let path_list = workspace_path_list(workspace, cx);
    let project = workspace.read(cx).project().read(cx);
    project.repositories(cx).values().filter_map(move |repo| {
        let snapshot = repo.read(cx).snapshot();
        let is_root = path_list
            .paths()
            .iter()
            .any(|p| p.as_path() == snapshot.work_directory_abs_path.as_ref());
        is_root.then_some(snapshot)
    })
}

pub(super) fn workspace_path_list(workspace: &Entity<Workspace>, cx: &App) -> PathList {
    PathList::new(&workspace.read(cx).root_paths(cx))
}

pub(super) fn linked_worktree_path_lists_for_workspaces(
    workspaces: &[Entity<Workspace>],
    cx: &App,
) -> Vec<PathList> {
    let mut linked_worktree_paths = Vec::new();
    for workspace in workspaces {
        if workspace.read(cx).visible_worktrees(cx).count() != 1 {
            continue;
        }
        for snapshot in root_repository_snapshots(workspace, cx) {
            linked_worktree_paths.extend(
                snapshot.linked_worktrees().iter().map(|linked_worktree| {
                    PathList::new(std::slice::from_ref(&linked_worktree.path))
                }),
            );
        }
    }

    linked_worktree_paths.sort_by(|a, b| a.paths()[0].cmp(&b.paths()[0]));
    linked_worktree_paths
}

pub(super) fn workspace_has_terminal_metadata_except(
    workspace: &Entity<Workspace>,
    except_terminal_id: Option<TerminalId>,
    cx: &App,
) -> bool {
    let Some(store) = TerminalThreadMetadataStore::try_global(cx) else {
        return false;
    };
    let path_list = workspace_path_list(workspace, cx);
    let remote_connection = workspace
        .read(cx)
        .project()
        .read(cx)
        .remote_connection_options(cx);
    store
        .read(cx)
        .entries_for_path(&path_list, remote_connection.as_ref())
        .any(|terminal| except_terminal_id != Some(terminal.terminal_id))
}

#[derive(Clone)]
pub(super) struct WorkspaceMenuWorktreeLabel {
    icon: Option<IconName>,
    primary_name: SharedString,
    secondary_name: Option<SharedString>,
}

impl WorkspaceMenuWorktreeLabel {
    pub(super) fn render(&self) -> impl IntoElement {
        h_flex()
            .min_w_0()
            .gap_0p5()
            .when_some(self.icon, |this, icon| {
                this.child(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted))
            })
            .child(Label::new(self.primary_name.clone()).truncate())
            .when_some(self.secondary_name.clone(), |this, secondary_name| {
                this.child(Label::new("/").alpha(0.5))
                    .child(Label::new(secondary_name).truncate())
            })
    }
}

pub(super) fn workspace_menu_worktree_labels(
    workspace: &Entity<Workspace>,
    cx: &App,
) -> Vec<WorkspaceMenuWorktreeLabel> {
    let root_paths = workspace.read(cx).root_paths(cx);
    let show_folder_name = root_paths.len() > 1;
    let project = workspace.read(cx).project().clone();
    let repository_snapshots: Vec<_> = project
        .read(cx)
        .repositories(cx)
        .values()
        .map(|repo| repo.read(cx).snapshot())
        .collect();

    root_paths
        .into_iter()
        .map(|root_path| {
            let root_path = root_path.as_ref();
            let folder_name = root_path
                .file_name()
                .map(|name| SharedString::from(name.to_string_lossy().to_string()))
                .unwrap_or_default();
            let repository_snapshot = repository_snapshots
                .iter()
                .find(|snapshot| snapshot.work_directory_abs_path.as_ref() == root_path);

            if let Some(snapshot) = repository_snapshot {
                let worktree_name = if snapshot.is_linked_worktree() {
                    snapshot
                        .main_worktree_abs_path()
                        .and_then(|main_worktree_path| {
                            project::linked_worktree_short_name(main_worktree_path, root_path)
                        })
                        .unwrap_or_else(|| folder_name.clone())
                } else {
                    "main".into()
                };

                if show_folder_name {
                    WorkspaceMenuWorktreeLabel {
                        icon: Some(IconName::GitWorktree),
                        primary_name: folder_name,
                        secondary_name: Some(worktree_name),
                    }
                } else {
                    WorkspaceMenuWorktreeLabel {
                        icon: Some(IconName::GitWorktree),
                        primary_name: worktree_name,
                        secondary_name: None,
                    }
                }
            } else {
                WorkspaceMenuWorktreeLabel {
                    icon: None,
                    primary_name: folder_name,
                    secondary_name: None,
                }
            }
        })
        .collect()
}

pub(super) fn apply_worktree_label_mode(
    mut worktrees: Vec<ThreadItemWorktreeInfo>,
    mode: AgentThreadWorktreeLabel,
) -> Vec<ThreadItemWorktreeInfo> {
    match mode {
        AgentThreadWorktreeLabel::Both => {}
        AgentThreadWorktreeLabel::Worktree => {
            for wt in &mut worktrees {
                wt.branch_name = None;
            }
        }
        AgentThreadWorktreeLabel::Branch => {
            for wt in &mut worktrees {
                // Fall back to showing the worktree name when no branch is
                // known; an empty chip would be worse than a mismatched icon.
                if wt.branch_name.is_some() {
                    wt.worktree_name = None;
                }
            }
        }
    }
    worktrees
}

/// Shows a [`RemoteConnectionModal`] on the given workspace and establishes
/// an SSH connection. Suitable for passing to
/// [`MultiWorkspace::find_or_create_workspace`] as the `connect_remote`
/// argument.
pub(super) fn connect_remote(
    modal_workspace: Entity<Workspace>,
    connection_options: RemoteConnectionOptions,
    window: &mut Window,
    cx: &mut Context<MultiWorkspace>,
) -> gpui::Task<anyhow::Result<Option<Entity<remote::RemoteClient>>>> {
    remote_connection::connect_with_modal(&modal_workspace, connection_options, window, cx)
}

// Per-project-group cache of the remote default branch, used to populate the
// "Create New Worktree" submenu without doing git I/O while the menu is open.
pub(super) enum DefaultBranchCache {
    Pending,
    Resolved(Option<RemoteBranchName>),
}

// Mirrors the behavior of the worktree picker's "Create new worktree" entries.
pub(super) fn create_worktree_in_workspace(
    workspace: &Entity<Workspace>,
    branch_target: NewWorktreeBranchTarget,
    window: &mut Window,
    cx: &mut App,
) {
    workspace.update(cx, |workspace, cx| {
        let focused_dock = workspace.focused_dock_position(window, cx);
        git_ui::worktree_service::handle_create_worktree(
            workspace,
            &CreateWorktree {
                worktree_name: None,
                branch_target,
            },
            window,
            focused_dock,
            cx,
        );
    });
}
