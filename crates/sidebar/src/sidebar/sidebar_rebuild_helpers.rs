use super::*;

pub(super) fn resolve_agent_icon(
    agent_server_store: Option<&Entity<project::AgentServerStore>>,
    agent_id: &AgentId,
    cx: &App,
) -> (IconName, Option<SharedString>) {
    let agent = Agent::from(agent_id.clone());
    let icon = match agent {
        Agent::NativeAgent => IconName::MavAgent,
        Agent::Custom { .. } => IconName::Terminal,
        _ => IconName::MavAgent,
    };
    let icon_from_external_svg =
        agent_server_store.and_then(|store| store.read(cx).agent_icon(agent_id));
    (icon, icon_from_external_svg)
}

pub(super) fn live_notified_terminal_ids(
    workspaces: &[Entity<Workspace>],
    cx: &App,
) -> HashSet<TerminalId> {
    let mut ids = HashSet::new();
    for workspace in workspaces {
        if let Some(agent_panel) = workspace.read(cx).panel::<AgentPanel>(cx) {
            ids.extend(
                agent_panel
                    .read(cx)
                    .terminals(cx)
                    .into_iter()
                    .filter_map(|terminal| terminal.has_notification.then_some(terminal.id)),
            );
        }
    }
    ids
}

pub(super) fn path_detail_map(groups: &[ProjectGroup]) -> HashMap<PathBuf, usize> {
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
    all_paths.into_iter().zip(path_details).collect()
}

pub(super) fn branch_by_path(
    workspaces: &[Entity<Workspace>],
    cx: &App,
) -> HashMap<PathBuf, SharedString> {
    let mut branch_by_path = HashMap::new();
    for ws in workspaces {
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
    branch_by_path
}

pub(super) fn terminal_entries_for_group(
    group_key: &ProjectGroupKey,
    group_workspaces: &[Entity<Workspace>],
    linked_worktree_path_lists: &[PathList],
    branch_by_path: &HashMap<PathBuf, SharedString>,
    live_notified_terminal_ids: &HashSet<TerminalId>,
    seen_terminal_ids: &mut HashSet<TerminalId>,
    cx: &App,
) -> Vec<TerminalEntry> {
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
    let make_terminal_entry = |metadata: TerminalThreadMetadata,
                               workspace: ThreadEntryWorkspace| {
        let worktrees = worktree_info_from_thread_paths(&metadata.worktree_paths, branch_by_path);
        let has_notification = live_notified_terminal_ids.contains(&metadata.terminal_id);
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
    for worktree_path_list in linked_worktree_path_lists {
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
    terminals
}
