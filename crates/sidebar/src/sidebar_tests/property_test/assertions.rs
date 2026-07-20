use super::*;

pub(super) fn update_sidebar(sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext) {
    sidebar.update_in(cx, |sidebar, _window, cx| {
        if let Some(mw) = sidebar.multi_workspace.upgrade() {
            mw.update(cx, |mw, _cx| mw.test_expand_all_groups());
        }
        sidebar.update_entries(cx);
    });
}

pub(super) fn validate_sidebar_properties(sidebar: &Sidebar, cx: &App) -> anyhow::Result<()> {
    verify_every_group_in_multiworkspace_is_shown(sidebar, cx)?;
    verify_no_duplicate_threads(sidebar)?;
    verify_all_threads_are_shown(sidebar, cx)?;
    verify_active_state_matches_current_workspace(sidebar, cx)?;
    verify_all_workspaces_are_reachable(sidebar, cx)?;
    verify_workspace_group_key_integrity(sidebar, cx)?;
    Ok(())
}

fn verify_no_duplicate_threads(sidebar: &Sidebar) -> anyhow::Result<()> {
    let mut seen: HashSet<acp::SessionId> = HashSet::default();
    let mut duplicates: Vec<(acp::SessionId, String)> = Vec::new();

    for entry in &sidebar.contents.entries {
        if let Some(session_id) = entry.session_id() {
            if !seen.insert(session_id.clone()) {
                let title = match entry {
                    ListEntry::Thread(thread) => thread.metadata.display_title().to_string(),
                    _ => "<unknown>".to_string(),
                };
                duplicates.push((session_id.clone(), title));
            }
        }
    }

    anyhow::ensure!(
        duplicates.is_empty(),
        "threads appear more than once in sidebar: {:?}",
        duplicates,
    );
    Ok(())
}

fn verify_every_group_in_multiworkspace_is_shown(
    sidebar: &Sidebar,
    cx: &App,
) -> anyhow::Result<()> {
    let Some(multi_workspace) = sidebar.multi_workspace.upgrade() else {
        anyhow::bail!("sidebar should still have an associated multi-workspace");
    };

    let mw = multi_workspace.read(cx);

    // Every project group key in the multi-workspace that has a
    // non-empty path list should appear as a ProjectHeader in the
    // sidebar.
    let all_keys = mw.project_group_keys();
    let expected_keys: HashSet<&ProjectGroupKey> = all_keys
        .iter()
        .filter(|k| !k.path_list().paths().is_empty())
        .collect();

    let sidebar_keys: HashSet<&ProjectGroupKey> = sidebar
        .contents
        .entries
        .iter()
        .filter_map(|entry| match entry {
            ListEntry::ProjectHeader { key, .. } => Some(key),
            _ => None,
        })
        .collect();

    let missing = &expected_keys - &sidebar_keys;
    let stray = &sidebar_keys - &expected_keys;

    anyhow::ensure!(
        missing.is_empty() && stray.is_empty(),
        "sidebar project groups don't match multi-workspace.\n\
         Only in multi-workspace (missing): {:?}\n\
         Only in sidebar (stray): {:?}",
        missing,
        stray,
    );

    Ok(())
}

fn verify_all_threads_are_shown(sidebar: &Sidebar, cx: &App) -> anyhow::Result<()> {
    let Some(multi_workspace) = sidebar.multi_workspace.upgrade() else {
        anyhow::bail!("sidebar should still have an associated multi-workspace");
    };
    let workspaces = multi_workspace
        .read(cx)
        .workspaces()
        .cloned()
        .collect::<Vec<_>>();
    let thread_store = ThreadMetadataStore::global(cx);

    let sidebar_thread_ids: HashSet<acp::SessionId> = sidebar
        .contents
        .entries
        .iter()
        .filter_map(|entry| entry.session_id().cloned())
        .collect();

    let mut metadata_thread_ids: HashSet<acp::SessionId> = HashSet::default();

    // Query using the same approach as the sidebar: iterate project
    // group keys, then do main + legacy queries per group.
    let mw = multi_workspace.read(cx);
    let mut workspaces_by_group: HashMap<ProjectGroupKey, Vec<Entity<Workspace>>> =
        HashMap::default();
    for workspace in &workspaces {
        let key = workspace.read(cx).project_group_key(cx);
        workspaces_by_group
            .entry(key)
            .or_default()
            .push(workspace.clone());
    }

    for group_key in mw.project_group_keys() {
        let path_list = group_key.path_list().clone();
        if path_list.paths().is_empty() {
            continue;
        }

        let group_workspaces = workspaces_by_group
            .get(&group_key)
            .map(|ws| ws.as_slice())
            .unwrap_or_default();

        // Main code path queries (run for all groups, even without workspaces).
        // Skip drafts (session_id: None) — they are not shown in the
        // sidebar entries.
        for metadata in thread_store
            .read(cx)
            .entries_for_main_worktree_path(&path_list, None)
        {
            if let Some(sid) = metadata.session_id.clone() {
                metadata_thread_ids.insert(sid);
            }
        }
        for metadata in thread_store.read(cx).entries_for_path(&path_list, None) {
            if let Some(sid) = metadata.session_id.clone() {
                metadata_thread_ids.insert(sid);
            }
        }

        // Legacy: per-workspace queries for different root paths.
        let covered_paths: HashSet<std::path::PathBuf> = group_workspaces
            .iter()
            .flat_map(|ws| {
                ws.read(cx)
                    .root_paths(cx)
                    .into_iter()
                    .map(|p| p.to_path_buf())
            })
            .collect();

        for workspace in group_workspaces {
            let ws_path_list = workspace_path_list(workspace, cx);
            if ws_path_list != path_list {
                for metadata in thread_store.read(cx).entries_for_path(&ws_path_list, None) {
                    if let Some(sid) = metadata.session_id.clone() {
                        metadata_thread_ids.insert(sid);
                    }
                }
            }
        }

        for workspace in group_workspaces {
            for snapshot in root_repository_snapshots(workspace, cx) {
                let Some(main_worktree_abs_path) = snapshot.main_worktree_abs_path() else {
                    continue;
                };
                let repo_path_list = PathList::new(&[main_worktree_abs_path.to_path_buf()]);
                if repo_path_list != path_list {
                    continue;
                }
                for linked_worktree in snapshot.linked_worktrees() {
                    if covered_paths.contains(&*linked_worktree.path) {
                        continue;
                    }
                    let worktree_path_list =
                        PathList::new(std::slice::from_ref(&linked_worktree.path));
                    for metadata in thread_store
                        .read(cx)
                        .entries_for_path(&worktree_path_list, None)
                    {
                        if let Some(sid) = metadata.session_id.clone() {
                            metadata_thread_ids.insert(sid);
                        }
                    }
                }
            }
        }
    }

    anyhow::ensure!(
        sidebar_thread_ids == metadata_thread_ids,
        "sidebar threads don't match metadata store: sidebar has {:?}, store has {:?}",
        sidebar_thread_ids,
        metadata_thread_ids,
    );
    Ok(())
}

fn verify_active_state_matches_current_workspace(
    sidebar: &Sidebar,
    cx: &App,
) -> anyhow::Result<()> {
    let Some(multi_workspace) = sidebar.multi_workspace.upgrade() else {
        anyhow::bail!("sidebar should still have an associated multi-workspace");
    };

    let active_workspace = multi_workspace.read(cx).workspace();

    // 1. active_entry should be Some when the panel has content.
    //    It may be None when the panel is uninitialized (no drafts,
    //    no threads), which is fine.
    //    It may also temporarily point at a different workspace
    //    when the workspace just changed and the new panel has no
    //    content yet.
    let panel = active_workspace.read(cx).panel::<AgentPanel>(cx).unwrap();
    let panel_has_content = panel.read(cx).active_thread_id(cx).is_some()
        || panel.read(cx).active_conversation_view().is_some()
        || panel.read(cx).active_terminal_id().is_some();

    let Some(entry) = sidebar.active_entry.as_ref() else {
        if panel_has_content {
            anyhow::bail!("active_entry is None but panel has content");
        }
        return Ok(());
    };

    // If the entry workspace doesn't match the active workspace
    // and the panel has no content, this is a transient state that
    // will resolve when the panel gets content.
    if entry.workspace().entity_id() != active_workspace.entity_id() && !panel_has_content {
        return Ok(());
    }

    // 2. The entry's workspace must agree with the multi-workspace's
    //    active workspace.
    anyhow::ensure!(
        entry.workspace().entity_id() == active_workspace.entity_id(),
        "active_entry workspace ({:?}) != active workspace ({:?})",
        entry.workspace().entity_id(),
        active_workspace.entity_id(),
    );

    // 3. The entry must match the agent panel's current state.
    if panel.read(cx).active_thread_id(cx).is_some() {
        anyhow::ensure!(
            matches!(entry, ActiveEntry::Thread { .. }),
            "panel shows a tracked draft but active_entry is {:?}",
            entry,
        );
    } else if let Some(thread_id) = panel
        .read(cx)
        .active_conversation_view()
        .map(|cv| cv.read(cx).parent_id())
    {
        anyhow::ensure!(
            matches!(entry, ActiveEntry::Thread { thread_id: tid, .. } if *tid == thread_id),
            "panel has thread {:?} but active_entry is {:?}",
            thread_id,
            entry,
        );
    }

    // 4. Exactly one entry in sidebar contents must be uniquely
    //    identified by the active_entry — unless the panel is showing
    //    the new-draft slot (which is represented by the + button's
    //    active state rather than a sidebar row) or nothing at all.
    // Active terminals must still match a row, so don't treat the absence
    // of a conversation view as "new-draft" when a terminal is active.
    let hidden_from_sidebar = panel.read(cx).active_terminal_id().is_none()
        && (panel.read(cx).active_view_is_new_draft(cx)
            || panel.read(cx).active_conversation_view().is_none());
    if hidden_from_sidebar {
        return Ok(());
    }
    let matching_count = sidebar
        .contents
        .entries
        .iter()
        .filter(|e| entry.matches_entry(e))
        .count();
    if matching_count != 1 {
        let thread_entries: Vec<_> = sidebar
            .contents
            .entries
            .iter()
            .filter_map(|e| match e {
                ListEntry::Thread(t) => Some(format!(
                    "tid={:?} sid={:?}",
                    t.metadata.thread_id, t.metadata.session_id
                )),
                _ => None,
            })
            .collect();
        let store = agent_ui::thread_metadata_store::ThreadMetadataStore::global(cx).read(cx);
        let store_entries: Vec<_> = store
            .entries()
            .map(|m| {
                format!(
                    "tid={:?} sid={:?} archived={} paths={:?}",
                    m.thread_id,
                    m.session_id,
                    m.archived,
                    m.folder_paths()
                )
            })
            .collect();
        anyhow::bail!(
            "expected exactly 1 sidebar entry matching active_entry {:?}, found {}. sidebar threads: {:?}. store: {:?}",
            entry,
            matching_count,
            thread_entries,
            store_entries,
        );
    }

    Ok(())
}

/// Every workspace in the multi-workspace should be "reachable" from
/// the sidebar — meaning there is at least one entry (thread, draft,
/// new-thread, or project header) that, when clicked, would activate
/// that workspace.
fn verify_all_workspaces_are_reachable(sidebar: &Sidebar, cx: &App) -> anyhow::Result<()> {
    let Some(multi_workspace) = sidebar.multi_workspace.upgrade() else {
        anyhow::bail!("sidebar should still have an associated multi-workspace");
    };

    let multi_workspace = multi_workspace.read(cx);

    let reachable_workspaces: HashSet<gpui::EntityId> = sidebar
        .contents
        .entries
        .iter()
        .flat_map(|entry| entry.reachable_workspaces(multi_workspace, cx))
        .map(|ws| ws.entity_id())
        .collect();

    let all_workspace_ids: HashSet<gpui::EntityId> = multi_workspace
        .workspaces()
        .map(|ws| ws.entity_id())
        .collect();

    let unreachable = &all_workspace_ids - &reachable_workspaces;

    anyhow::ensure!(
        unreachable.is_empty(),
        "The following workspaces are not reachable from any sidebar entry: {:?}",
        unreachable,
    );

    Ok(())
}

fn verify_workspace_group_key_integrity(sidebar: &Sidebar, cx: &App) -> anyhow::Result<()> {
    let Some(multi_workspace) = sidebar.multi_workspace.upgrade() else {
        anyhow::bail!("sidebar should still have an associated multi-workspace");
    };
    multi_workspace
        .read(cx)
        .assert_project_group_key_integrity(cx)
}
