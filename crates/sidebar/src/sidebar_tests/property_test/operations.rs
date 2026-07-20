use super::state::{Operation, TestState, UnopenedWorktree};
use super::*;

pub(super) fn save_thread_to_path_with_main(
    state: &mut TestState,
    path_list: PathList,
    main_worktree_paths: PathList,
    cx: &mut gpui::VisualTestContext,
) {
    let session_id = state.next_metadata_only_thread_id();
    let title: SharedString = format!("Thread {}", session_id).into();
    let updated_at = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0).unwrap()
        + chrono::Duration::seconds(state.thread_counter as i64);
    let metadata = ThreadMetadata {
        thread_id: ThreadId::new(),
        session_id: Some(session_id),
        agent_id: agent::MAV_AGENT_ID.clone(),
        title: Some(title),
        title_override: None,
        updated_at,
        created_at: None,
        interacted_at: None,
        worktree_paths: WorktreePaths::from_path_lists(main_worktree_paths, path_list).unwrap(),
        archived: false,
        remote_connection: None,
    };
    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx))
    });
    cx.run_until_parked();
}

pub(super) async fn perform_operation(
    operation: Operation,
    state: &mut TestState,
    multi_workspace: &Entity<MultiWorkspace>,
    sidebar: &Entity<Sidebar>,
    cx: &mut gpui::VisualTestContext,
) {
    match operation {
        Operation::SaveThread {
            project_group_index,
        } => {
            // Find a workspace for this project group and create a real
            // thread via its agent panel.
            let (workspace, project) = multi_workspace.read_with(cx, |mw, cx| {
                let keys = mw.project_group_keys();
                let key = &keys[project_group_index];
                let ws = mw
                    .workspaces_for_project_group(key, cx)
                    .and_then(|ws| ws.first().cloned())
                    .unwrap_or_else(|| mw.workspace().clone());
                let project = ws.read(cx).project().clone();
                (ws, project)
            });

            let panel = workspace.read_with(cx, |workspace, cx| workspace.panel::<AgentPanel>(cx));
            if let Some(panel) = panel {
                let agent_id = AgentId::new(format!("prop-agent-{}", state.thread_counter));
                let connection = StubAgentConnection::new().with_agent_id(agent_id.clone());
                open_thread_with_custom_connection(&panel, connection.clone(), cx);
                let thread_id = active_thread_id(&panel, cx);
                let session_id = active_session_id(&panel, cx);
                // Make the thread non-draft without exercising the prompt
                // send path; these invariants are about sidebar state, not
                // git checkpointing during user prompts.
                cx.update(|_, cx| {
                    connection.send_update(
                        session_id.clone(),
                        acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                            "Done".into(),
                        )),
                        cx,
                    );
                });
                cx.run_until_parked();
                state.saved_thread_ids.push(session_id.clone());

                let title: SharedString = format!("Thread {}", state.thread_counter).into();
                state.thread_counter += 1;
                let updated_at =
                    chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0).unwrap()
                        + chrono::Duration::seconds(state.thread_counter as i64);
                let metadata = cx.update(|_, cx| ThreadMetadata {
                    thread_id,
                    session_id: Some(session_id),
                    agent_id,
                    title: Some(title),
                    title_override: None,
                    updated_at,
                    created_at: None,
                    interacted_at: None,
                    worktree_paths: project.read(cx).worktree_paths(cx),
                    archived: false,
                    remote_connection: project.read(cx).remote_connection_options(cx),
                });
                cx.update(|_, cx| {
                    ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx))
                });
                cx.run_until_parked();
            }
        }
        Operation::SaveWorktreeThread { worktree_index } => {
            let worktree = &state.unopened_worktrees[worktree_index];
            let path_list = PathList::new(&[std::path::PathBuf::from(&worktree.path)]);
            let main_worktree_paths =
                PathList::new(&[std::path::PathBuf::from(&worktree.main_workspace_path)]);
            save_thread_to_path_with_main(state, path_list, main_worktree_paths, cx);
        }

        Operation::ToggleAgentPanel => {
            let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
            let panel_open =
                workspace.read_with(cx, |_, cx| AgentPanel::is_visible(&workspace, cx));
            workspace.update_in(cx, |workspace, window, cx| {
                if panel_open {
                    workspace.close_panel::<AgentPanel>(window, cx);
                } else {
                    workspace.open_panel::<AgentPanel>(window, cx);
                }
            });
        }
        Operation::CreateDraftThread => {
            let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
            let panel = workspace.read_with(cx, |workspace, cx| workspace.panel::<AgentPanel>(cx));
            if let Some(panel) = panel {
                panel.update_in(cx, |panel, window, cx| {
                    panel.new_thread(&NewThread, window, cx);
                });
                cx.run_until_parked();
            }
            workspace.update_in(cx, |workspace, window, cx| {
                workspace.focus_panel::<AgentPanel>(window, cx);
            });
        }
        Operation::AddProject { use_worktree } => {
            let path = if use_worktree {
                // Open an existing linked worktree as a project (simulates Cmd+O
                // on a worktree directory).
                state.unopened_worktrees.remove(0).path
            } else {
                // Create a brand new project.
                let path = state.next_workspace_path();
                state
                    .fs
                    .insert_tree(
                        &path,
                        serde_json::json!({
                            ".git": {},
                            "src": {},
                        }),
                    )
                    .await;
                path
            };
            let project =
                project::Project::test(state.fs.clone() as Arc<dyn fs::Fs>, [path.as_ref()], cx)
                    .await;
            project.update(cx, |p, cx| p.git_scans_complete(cx)).await;
            multi_workspace.update_in(cx, |mw, window, cx| {
                mw.test_add_workspace(project.clone(), window, cx)
            });
        }

        Operation::ArchiveThread { index } => {
            let session_id = state.saved_thread_ids[index].clone();
            sidebar.update_in(cx, |sidebar: &mut Sidebar, window, cx| {
                sidebar.archive_thread(&session_id, window, cx);
            });
            cx.run_until_parked();
            state.saved_thread_ids.remove(index);
        }
        Operation::SwitchToThread { index } => {
            let session_id = state.saved_thread_ids[index].clone();
            // Find the thread's position in the sidebar entries and select it.
            let thread_index = sidebar.read_with(cx, |sidebar, _| {
                sidebar.contents.entries.iter().position(|entry| {
                    matches!(
                        entry,
                        ListEntry::Thread(t) if t.metadata.session_id.as_ref() == Some(&session_id)
                    )
                })
            });
            if let Some(ix) = thread_index {
                sidebar.update_in(cx, |sidebar, window, cx| {
                    sidebar.selection = Some(ix);
                    sidebar.confirm(&Confirm, window, cx);
                });
                cx.run_until_parked();
            }
        }
        Operation::SwitchToProjectGroup { index } => {
            let workspace = multi_workspace.read_with(cx, |mw, cx| {
                let keys = mw.project_group_keys();
                let key = &keys[index];
                mw.workspaces_for_project_group(key, cx)
                    .and_then(|ws| ws.first().cloned())
                    .unwrap_or_else(|| mw.workspace().clone())
            });
            multi_workspace.update_in(cx, |mw, window, cx| {
                mw.activate(workspace, None, window, cx);
            });
        }
        Operation::AddLinkedWorktree {
            project_group_index,
        } => {
            // Get the main worktree path from the project group key.
            let main_path = multi_workspace.read_with(cx, |mw, _| {
                let keys = mw.project_group_keys();
                let key = &keys[project_group_index];
                key.path_list()
                    .paths()
                    .first()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            });
            let dot_git = format!("{}/.git", main_path);
            let worktree_name = state.next_worktree_name();
            let worktree_path = format!("/worktrees/{}", worktree_name);

            state
                .fs
                .insert_tree(
                    &worktree_path,
                    serde_json::json!({
                        ".git": format!("gitdir: {}/.git/worktrees/{}", main_path, worktree_name),
                        "src": {},
                    }),
                )
                .await;

            // Also create the worktree metadata dir inside the main repo's .git
            state
                .fs
                .insert_tree(
                    &format!("{}/.git/worktrees/{}", main_path, worktree_name),
                    serde_json::json!({
                        "commondir": "../../",
                        "HEAD": format!("ref: refs/heads/{}", worktree_name),
                    }),
                )
                .await;

            let dot_git_path = std::path::Path::new(&dot_git);
            let worktree_pathbuf = std::path::PathBuf::from(&worktree_path);
            state
                .fs
                .add_linked_worktree_for_repo(
                    dot_git_path,
                    false,
                    git::repository::Worktree {
                        path: worktree_pathbuf,
                        ref_name: Some(format!("refs/heads/{}", worktree_name).into()),
                        sha: "aaa".into(),
                        is_main: false,
                        is_bare: false,
                    },
                )
                .await;

            // Re-scan the main workspace's project so it discovers the new worktree.
            let main_workspace = multi_workspace.read_with(cx, |mw, cx| {
                let keys = mw.project_group_keys();
                let key = &keys[project_group_index];
                mw.workspaces_for_project_group(key, cx)
                    .and_then(|ws| ws.first().cloned())
                    .unwrap()
            });
            let main_project = main_workspace.read_with(cx, |ws, _| ws.project().clone());
            main_project
                .update(cx, |p, cx| p.git_scans_complete(cx))
                .await;

            state.unopened_worktrees.push(UnopenedWorktree {
                path: worktree_path,
                main_workspace_path: main_path.clone(),
            });
        }
        Operation::AddWorktreeToProject {
            project_group_index,
        } => {
            let workspace = multi_workspace.read_with(cx, |mw, cx| {
                let keys = mw.project_group_keys();
                let key = &keys[project_group_index];
                mw.workspaces_for_project_group(key, cx)
                    .and_then(|ws| ws.first().cloned())
            });
            let Some(workspace) = workspace else { return };
            let project = workspace.read_with(cx, |ws, _| ws.project().clone());

            let new_path = state.next_workspace_path();
            state
                .fs
                .insert_tree(&new_path, serde_json::json!({ ".git": {}, "src": {} }))
                .await;

            let result = project
                .update(cx, |project, cx| {
                    project.find_or_create_worktree(&new_path, true, cx)
                })
                .await;
            if result.is_err() {
                return;
            }
            cx.run_until_parked();
        }
        Operation::RemoveWorktreeFromProject {
            project_group_index,
        } => {
            let workspace = multi_workspace.read_with(cx, |mw, cx| {
                let keys = mw.project_group_keys();
                let key = &keys[project_group_index];
                mw.workspaces_for_project_group(key, cx)
                    .and_then(|ws| ws.first().cloned())
            });
            let Some(workspace) = workspace else { return };
            let project = workspace.read_with(cx, |ws, _| ws.project().clone());

            let worktree_count = project.read_with(cx, |p, cx| p.visible_worktrees(cx).count());
            if worktree_count <= 1 {
                return;
            }

            let worktree_id = project.read_with(cx, |p, cx| {
                p.visible_worktrees(cx).last().map(|wt| wt.read(cx).id())
            });
            if let Some(worktree_id) = worktree_id {
                project.update(cx, |project, cx| {
                    project.remove_worktree(worktree_id, cx);
                });
                cx.run_until_parked();
            }
        }
    }
}
