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

pub(super) struct GroupThreadEntries {
    pub(super) threads: Vec<Arc<ThreadEntry>>,
    pub(super) has_running_threads: bool,
    pub(super) waiting_thread_count: usize,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn thread_entries_for_group(
    group_key: &ProjectGroupKey,
    group_workspaces: &[Entity<Workspace>],
    linked_worktree_path_lists: &[PathList],
    should_load_threads: bool,
    is_active: bool,
    active_workspace: Option<&Entity<Workspace>>,
    active_entry: Option<&ActiveEntry>,
    pending_thread_activation: Option<ThreadId>,
    old_statuses: &HashMap<acp::SessionId, (AgentThreadStatus, ThreadId)>,
    new_live_statuses: &mut HashMap<acp::SessionId, (AgentThreadStatus, ThreadId)>,
    notified_threads: &mut HashSet<ThreadId>,
    seen_thread_ids: &mut HashSet<ThreadId>,
    branch_by_path: &HashMap<PathBuf, SharedString>,
    agent_server_store: Option<&Entity<project::AgentServerStore>>,
    cx: &App,
) -> GroupThreadEntries {
    let live_infos = group_workspaces
        .iter()
        .flat_map(|ws| all_thread_infos_for_workspace(ws, cx));
    let mut threads = Vec::new();
    let mut has_running_threads = false;
    let mut waiting_thread_count = 0;
    let group_host = group_key.host();

    if should_load_threads {
        let thread_store = ThreadMetadataStore::global(cx);
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
        let make_thread_entry = |row: ThreadMetadata,
                                 workspace: ThreadEntryWorkspace|
         -> Arc<ThreadEntry> {
            let (icon, icon_from_external_svg) =
                resolve_agent_icon(agent_server_store, &row.agent_id, cx);
            let worktrees = worktree_info_from_thread_paths(&row.worktree_paths, branch_by_path);
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

        for row in thread_store
            .read(cx)
            .entries_for_main_worktree_path(group_key.path_list(), group_host.as_ref())
            .cloned()
        {
            if row.is_draft() || !seen_thread_ids.insert(row.thread_id) {
                continue;
            }
            let workspace = resolve_workspace(row.folder_paths());
            threads.push(make_thread_entry(row, workspace));
        }

        for row in thread_store
            .read(cx)
            .entries_for_path(group_key.path_list(), group_host.as_ref())
            .cloned()
        {
            if row.is_draft() || !seen_thread_ids.insert(row.thread_id) {
                continue;
            }
            let workspace = resolve_workspace(row.folder_paths());
            threads.push(make_thread_entry(row, workspace));
        }

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
                if row.is_draft() || !seen_thread_ids.insert(row.thread_id) {
                    continue;
                }
                threads.push(make_thread_entry(
                    row,
                    ThreadEntryWorkspace::Open(ws.clone()),
                ));
            }
        }

        for worktree_path_list in linked_worktree_path_lists {
            for row in thread_store
                .read(cx)
                .entries_for_path(worktree_path_list, group_host.as_ref())
                .cloned()
            {
                if row.is_draft() || !seen_thread_ids.insert(row.thread_id) {
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
            if let Some((label, kind)) =
                draft_display_label_for_thread_metadata(&thread.metadata, &thread.workspace, cx)
            {
                let thread = Arc::make_mut(thread);
                thread.metadata.title = Some(label);
                thread.draft = Some(kind);
            }
        }
        threads.retain(|thread| thread.draft.is_none() || thread.metadata.title.is_some());

        let active_panel_thread_id = active_workspace
            .and_then(|ws| ws.read(cx).panel::<AgentPanel>(cx))
            .and_then(|panel| panel.read(cx).active_thread_id(cx));
        threads.retain(|thread| {
            if thread.draft != Some(DraftKind::Empty) {
                return true;
            }
            if pending_thread_activation.is_some() {
                return false;
            }
            Some(thread.metadata.thread_id) == active_panel_thread_id
        });

        let mut live_info_by_session: HashMap<acp::SessionId, ActiveThreadInfo> = HashMap::new();
        for info in live_infos {
            if info.status == AgentThreadStatus::Running {
                has_running_threads = true;
            }
            if info.status == AgentThreadStatus::WaitingForConfirmation {
                waiting_thread_count += 1;
            }
            live_info_by_session.insert(info.session_id.clone(), info);
        }

        for thread in &mut threads {
            if let Some(session_id) = thread.metadata.session_id.clone()
                && let Some(info) = live_info_by_session.get(&session_id)
            {
                let status = info.status;
                let thread_id = thread.metadata.thread_id;
                Arc::make_mut(thread).apply_active_info(info);
                new_live_statuses.insert(session_id, (status, thread_id));
            }

            let session_id = &thread.metadata.session_id;
            let is_active_thread = active_entry.is_some_and(|entry| {
                entry.is_active_thread(&thread.metadata.thread_id)
                    && active_workspace.is_some_and(|active| active == entry.workspace())
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
            let a_time = Sidebar::thread_display_time(&a.metadata);
            let b_time = Sidebar::thread_display_time(&b.metadata);
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

        if is_active && let Some(ActiveEntry::Thread { thread_id, .. }) = active_entry {
            notified_threads.remove(thread_id);
        }
    }

    GroupThreadEntries {
        threads,
        has_running_threads,
        waiting_thread_count,
    }
}
