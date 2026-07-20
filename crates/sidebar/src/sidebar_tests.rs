use super::*;
use acp_thread::{AcpThread, PermissionOptions, StubAgentConnection};
use agent::ThreadStore;
use agent_ui::{
    ThreadId,
    terminal_thread_metadata_store::{
        TerminalThreadMetadata, TerminalThreadMetadataStore, TestTerminalMetadataDbName,
    },
    test_support::{
        active_session_id, active_thread_id, open_thread_with_connection,
        open_thread_with_custom_connection, send_message,
    },
    thread_metadata_store::{ThreadMetadata, WorktreePaths},
};
use chrono::DateTime;
use fs::{FakeFs, Fs};
use gpui::TestAppContext;
use pretty_assertions::assert_eq;
use project::AgentId;
use settings::SettingsStore;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use util::{path_list::PathList, rel_path::rel_path};

fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
        // Use an isolated DB so parallel tests can't see each other's
        // persisted records (e.g. created-worktree records).
        cx.set_global(db::AppDatabase::test_new());
        theme_settings::init(theme::LoadThemes::JustBase, cx);
        editor::init(cx);
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        TerminalThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });
}

#[track_caller]
fn assert_active_thread(sidebar: &Sidebar, session_id: &acp::SessionId, msg: &str) {
    let active = sidebar.active_entry.as_ref();
    let matches = active.is_some_and(|entry| {
        matches!(entry, ActiveEntry::Thread { session_id: Some(active_session_id), .. } if active_session_id == session_id)
            || sidebar.contents.entries.iter().any(|list_entry| {
                matches!(list_entry, ListEntry::Thread(t)
                    if t.metadata.session_id.as_ref() == Some(session_id)
                        && entry.matches_entry(list_entry))
            })
    });
    assert!(
        matches,
        "{msg}: expected active_entry for session {session_id:?}, got {:?}",
        active,
    );
}

#[track_caller]
fn is_active_session(sidebar: &Sidebar, session_id: &acp::SessionId) -> bool {
    let thread_id = sidebar
        .contents
        .entries
        .iter()
        .find_map(|entry| match entry {
            ListEntry::Thread(t) if t.metadata.session_id.as_ref() == Some(session_id) => {
                Some(t.metadata.thread_id)
            }
            _ => None,
        });
    match thread_id {
        Some(tid) => {
            matches!(&sidebar.active_entry, Some(ActiveEntry::Thread { thread_id, .. }) if *thread_id == tid)
        }
        // Thread not in sidebar entries — can't confirm it's active.
        None => false,
    }
}

#[track_caller]
fn assert_active_draft(sidebar: &Sidebar, workspace: &Entity<Workspace>, msg: &str) {
    assert!(
        matches!(&sidebar.active_entry, Some(ActiveEntry::Thread { workspace: ws, .. }) if ws == workspace),
        "{msg}: expected active_entry to be Draft for workspace {:?}, got {:?}",
        workspace.entity_id(),
        sidebar.active_entry,
    );
}

fn has_thread_entry(sidebar: &Sidebar, session_id: &acp::SessionId) -> bool {
    sidebar
        .contents
        .entries
        .iter()
        .any(|entry| matches!(entry, ListEntry::Thread(t) if t.metadata.session_id.as_ref() == Some(session_id)))
}

#[track_caller]
fn assert_project_header_has_threads(
    sidebar: &Entity<Sidebar>,
    project_name: &str,
    expected_has_threads: bool,
    cx: &mut gpui::VisualTestContext,
) {
    sidebar.read_with(cx, |sidebar, _cx| {
        let has_threads = sidebar.contents.entries.iter().find_map(|entry| {
            if let ListEntry::ProjectHeader {
                label, has_threads, ..
            } = entry
                && label.as_ref() == project_name
            {
                Some(*has_threads)
            } else {
                None
            }
        });

        assert_eq!(
            has_threads,
            Some(expected_has_threads),
            "expected project header `{project_name}` to have has_threads={expected_has_threads}, got {has_threads:?}"
        );
    });
}

#[track_caller]
fn assert_remote_project_integration_sidebar_state(
    sidebar: &mut Sidebar,
    main_thread_id: &acp::SessionId,
    remote_thread_id: &acp::SessionId,
) {
    let mut project_headers = sidebar.contents.entries.iter().filter_map(|entry| {
        if let ListEntry::ProjectHeader { label, .. } = entry {
            Some(label.as_ref())
        } else {
            None
        }
    });

    let Some(project_header) = project_headers.next() else {
        panic!("expected exactly one sidebar project header named `project`, found none");
    };
    assert_eq!(
        project_header, "project",
        "expected the only sidebar project header to be `project`"
    );
    if let Some(unexpected_header) = project_headers.next() {
        panic!(
            "expected exactly one sidebar project header named `project`, found extra header `{unexpected_header}`"
        );
    }

    let mut saw_main_thread = false;
    let mut saw_remote_thread = false;
    for entry in &sidebar.contents.entries {
        match entry {
            ListEntry::ProjectHeader { label, .. } => {
                assert_eq!(
                    label.as_ref(),
                    "project",
                    "expected the only sidebar project header to be `project`"
                );
            }
            ListEntry::Thread(thread)
                if thread.metadata.session_id.as_ref() == Some(main_thread_id) =>
            {
                saw_main_thread = true;
            }
            ListEntry::Thread(thread)
                if thread.metadata.session_id.as_ref() == Some(remote_thread_id) =>
            {
                saw_remote_thread = true;
            }
            ListEntry::Thread(thread) => {
                let title = thread.metadata.display_title();
                panic!(
                    "unexpected sidebar thread while simulating remote project integration flicker: title=`{}`",
                    title
                );
            }
            ListEntry::Terminal(terminal) => {
                panic!(
                    "unexpected sidebar terminal while simulating remote project integration flicker: title=`{}`",
                    terminal.metadata.title
                );
            }
        }
    }

    assert!(
        saw_main_thread,
        "expected the sidebar to keep showing `Main Thread` under `project`"
    );
    assert!(
        saw_remote_thread,
        "expected the sidebar to keep showing `Worktree Thread` under `project`"
    );
}

async fn init_test_project(
    worktree_path: &str,
    cx: &mut TestAppContext,
) -> Entity<project::Project> {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(worktree_path, serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    project::Project::test(fs, [worktree_path.as_ref()], cx).await
}

fn setup_sidebar(
    multi_workspace: &Entity<MultiWorkspace>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<Sidebar> {
    let sidebar = setup_sidebar_closed(multi_workspace, cx);
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.toggle_sidebar(window, cx);
    });
    cx.run_until_parked();
    sidebar
}

fn setup_sidebar_closed(
    multi_workspace: &Entity<MultiWorkspace>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<Sidebar> {
    let multi_workspace = multi_workspace.clone();
    let sidebar = cx.update(|window, cx| {
        let sidebar = cx.new(|cx| Sidebar::new(multi_workspace.clone(), window, cx));
        multi_workspace.update(cx, |mw, cx| {
            mw.register_sidebar(sidebar.clone(), window, cx);
        });
        sidebar
    });
    cx.run_until_parked();
    sidebar
}

async fn save_n_test_threads(
    count: u32,
    project: &Entity<project::Project>,
    cx: &mut gpui::VisualTestContext,
) {
    for i in 0..count {
        save_thread_metadata(
            acp::SessionId::new(Arc::from(format!("thread-{}", i))),
            Some(format!("Thread {}", i + 1).into()),
            chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, i).unwrap(),
            None,
            None,
            project,
            cx,
        )
    }
    cx.run_until_parked();
}

async fn save_test_thread_metadata(
    session_id: &acp::SessionId,
    project: &Entity<project::Project>,
    cx: &mut TestAppContext,
) {
    save_thread_metadata(
        session_id.clone(),
        Some("Test".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        project,
        cx,
    )
}

async fn save_named_thread_metadata(
    session_id: &str,
    title: &str,
    project: &Entity<project::Project>,
    cx: &mut gpui::VisualTestContext,
) {
    save_thread_metadata(
        acp::SessionId::new(Arc::from(session_id)),
        Some(SharedString::from(title.to_string())),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        project,
        cx,
    );
    cx.run_until_parked();
}

/// Seeds a pre-built [`ThreadMetadata`] into the global store so tests can
/// exercise flows that resolve a thread by id.
fn seed_thread_metadata(metadata: ThreadMetadata, cx: &mut TestAppContext) {
    cx.update(|cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();
}

/// Spins up a fresh remote project backed by a headless server sharing
/// `server_fs`, opens the given worktree path on it, and returns the
/// project together with the headless entity (which the caller must keep
/// alive for the duration of the test) and the `RemoteConnectionOptions`
/// used for the fake server. Passing those options back into
/// `reuse_opts` on a subsequent call makes the new project share the
/// same `RemoteConnectionIdentity`, matching how Mav treats multiple
/// projects on the same SSH host.
async fn start_remote_project(
    server_fs: &Arc<FakeFs>,
    worktree_path: &Path,
    app_state: &Arc<workspace::AppState>,
    reuse_opts: Option<&remote::RemoteConnectionOptions>,
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) -> (
    Entity<project::Project>,
    Entity<remote_server::HeadlessProject>,
    remote::RemoteConnectionOptions,
) {
    // Bare `_` on the guard so it's dropped immediately; holding onto it
    // would deadlock `connect_mock` below since the client waits on the
    // guard before completing the mock handshake.
    let (opts, server_session) = match reuse_opts {
        Some(existing) => {
            let (session, _) = remote::RemoteClient::fake_server_with_opts(existing, cx, server_cx);
            (existing.clone(), session)
        }
        None => {
            let (opts, session, _) = remote::RemoteClient::fake_server(cx, server_cx);
            (opts, session)
        }
    };

    server_cx.update(remote_server::HeadlessProject::init);
    let server_executor = server_cx.executor();
    let fs = server_fs.clone();
    let headless = server_cx.new(|cx| {
        remote_server::HeadlessProject::new(
            remote_server::HeadlessAppState {
                session: server_session,
                fs,
                http_client: Arc::new(http_client::BlockedHttpClient),
                node_runtime: node_runtime::NodeRuntime::unavailable(),
                languages: Arc::new(language::LanguageRegistry::new(server_executor.clone())),
                extension_host_proxy: Arc::new(extension::ExtensionHostProxy::new()),
                startup_time: std::time::Instant::now(),
            },
            false,
            cx,
        )
    });

    let remote_client = remote::RemoteClient::connect_mock(opts.clone(), cx).await;
    let project = cx.update(|cx| {
        let project_client = client::Client::new(
            Arc::new(clock::FakeSystemClock::new()),
            http_client::FakeHttpClient::with_404_response(),
            cx,
        );
        let user_store = cx.new(|cx| client::UserStore::new(project_client.clone(), cx));
        project::Project::remote(
            remote_client,
            project_client,
            node_runtime::NodeRuntime::unavailable(),
            user_store,
            app_state.languages.clone(),
            app_state.fs.clone(),
            false,
            cx,
        )
    });

    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(worktree_path, true, cx)
        })
        .await
        .expect("should open remote worktree");
    cx.run_until_parked();

    (project, headless, opts)
}

fn save_thread_metadata(
    session_id: acp::SessionId,
    title: Option<SharedString>,
    updated_at: DateTime<Utc>,
    created_at: Option<DateTime<Utc>>,
    interacted_at: Option<DateTime<Utc>>,
    project: &Entity<project::Project>,
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let worktree_paths = project.read(cx).worktree_paths(cx);
        let remote_connection = project.read(cx).remote_connection_options(cx);
        let thread_id = ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .unwrap_or_else(ThreadId::new);
        let metadata = ThreadMetadata {
            thread_id,
            session_id: Some(session_id),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title,
            title_override: None,
            updated_at,
            created_at,
            interacted_at,
            worktree_paths,
            archived: false,
            remote_connection,
        };
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();
}

fn save_thread_metadata_with_main_paths(
    session_id: &str,
    title: &str,
    folder_paths: PathList,
    main_worktree_paths: PathList,
    updated_at: DateTime<Utc>,
    cx: &mut TestAppContext,
) {
    let session_id = acp::SessionId::new(Arc::from(session_id));
    let title = SharedString::from(title.to_string());
    let thread_id = cx.update(|cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .unwrap_or_else(ThreadId::new)
    });
    let metadata = ThreadMetadata {
        thread_id,
        session_id: Some(session_id),
        agent_id: agent::MAV_AGENT_ID.clone(),
        title: Some(title),
        title_override: None,
        updated_at,
        created_at: None,
        interacted_at: None,
        worktree_paths: WorktreePaths::from_path_lists(main_worktree_paths, folder_paths).unwrap(),
        archived: false,
        remote_connection: None,
    };
    cx.update(|cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();
}

fn save_draft_metadata_with_main_paths(
    title: Option<SharedString>,
    folder_paths: PathList,
    main_worktree_paths: PathList,
    updated_at: DateTime<Utc>,
    cx: &mut TestAppContext,
) -> ThreadId {
    let thread_id = ThreadId::new();
    let metadata = ThreadMetadata {
        thread_id,
        session_id: None,
        agent_id: agent::MAV_AGENT_ID.clone(),
        title,
        title_override: None,
        updated_at,
        created_at: None,
        interacted_at: None,
        worktree_paths: WorktreePaths::from_path_lists(main_worktree_paths, folder_paths).unwrap(),
        archived: false,
        remote_connection: None,
    };
    cx.update(|cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();
    thread_id
}

fn focus_sidebar(sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext) {
    sidebar.update_in(cx, |_, window, cx| {
        cx.focus_self(window);
    });
    cx.run_until_parked();
}

fn request_test_tool_authorization(
    thread: &Entity<AcpThread>,
    tool_call_id: &str,
    option_id: &str,
    cx: &mut gpui::VisualTestContext,
) {
    let tool_call_id = acp::ToolCallId::new(tool_call_id);
    let label = format!("Tool {tool_call_id}");
    let option_id = acp::PermissionOptionId::new(option_id);
    let _authorization_task = cx.update(|_, cx| {
        thread.update(cx, |thread, cx| {
            thread
                .request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id, label)
                        .kind(acp::ToolKind::Edit)
                        .into(),
                    PermissionOptions::Flat(vec![acp::PermissionOption::new(
                        option_id,
                        "Allow",
                        acp::PermissionOptionKind::AllowOnce,
                    )]),
                    acp_thread::AuthorizationKind::PermissionGrant,
                    cx,
                )
                .unwrap()
        })
    });
    cx.run_until_parked();
}

fn format_linked_worktree_chips(worktrees: &[ThreadItemWorktreeInfo]) -> String {
    let mut seen = Vec::new();
    let mut chips = Vec::new();
    for wt in worktrees {
        if wt.kind == ui::WorktreeKind::Main {
            continue;
        }
        let Some(name) = wt.worktree_name.as_ref() else {
            continue;
        };
        if !seen.contains(name) {
            seen.push(name.clone());
            chips.push(format!("{{{}}}", name));
        }
    }
    if chips.is_empty() {
        String::new()
    } else {
        format!(" {}", chips.join(", "))
    }
}

fn visible_entries_as_strings(
    sidebar: &Entity<Sidebar>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<String> {
    sidebar.read_with(cx, |sidebar, cx| {
        sidebar
            .contents
            .entries
            .iter()
            .enumerate()
            .map(|(ix, entry)| {
                let selected = if sidebar.selection == Some(ix) {
                    "  <== selected"
                } else {
                    ""
                };
                match entry {
                    ListEntry::ProjectHeader {
                        label,
                        key,
                        highlight_positions: _,
                        ..
                    } => {
                        let icon = if sidebar.is_group_collapsed(key, cx) {
                            ">"
                        } else {
                            "v"
                        };
                        format!("{} [{}]{}", icon, label, selected)
                    }
                    ListEntry::Thread(thread) => {
                        let title = thread.metadata.display_title();
                        let worktree = format_linked_worktree_chips(&thread.worktrees);

                        {
                            let live = if thread.is_live { " *" } else { "" };
                            let status_str = match thread.status {
                                AgentThreadStatus::Running => " (running)",
                                AgentThreadStatus::Error => " (error)",
                                AgentThreadStatus::WaitingForConfirmation => " (waiting)",
                                _ => "",
                            };
                            let notified = if sidebar
                                .contents
                                .is_thread_notified(&thread.metadata.thread_id)
                            {
                                " (!)"
                            } else {
                                ""
                            };
                            format!("  {title}{worktree}{live}{status_str}{notified}{selected}")
                        }
                    }
                    ListEntry::Terminal(terminal) => {
                        let title = terminal.metadata.display_title();
                        let worktree = format_linked_worktree_chips(&terminal.worktrees);
                        format!("  {title}{worktree}{selected}")
                    }
                }
            })
            .collect()
    })
}

#[gpui::test]
async fn test_thread_metadata_update_preserves_sticky_header_measurements(cx: &mut TestAppContext) {
    let (fs, project_a) = init_multi_project_test(&["/project-a", "/project-b"], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    add_test_project("/project-b", &fs, &multi_workspace, cx).await;

    save_thread_metadata(
        acp::SessionId::new(Arc::from("project-a-thread")),
        Some("Project A Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project_a,
        cx,
    );
    save_thread_metadata_with_main_paths(
        "project-b-thread",
        "Project B Thread",
        PathList::new(&[PathBuf::from("/project-b")]),
        PathList::new(&[PathBuf::from("/project-b")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        cx,
    );

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(400.), px(240.)),
        |_, _| sidebar.clone().into_any_element(),
    );
    cx.run_until_parked();

    let next_header_ix = sidebar.read_with(cx, |sidebar, _| {
        assert!(
            sidebar.contents.project_header_indices.len() == 2,
            "test setup should render exctly two project headers"
        );
        sidebar.contents.project_header_indices[1]
    });

    sidebar.update_in(cx, |sidebar, _window, cx| {
        sidebar.list_state.scroll_to(gpui::ListOffset {
            item_ix: next_header_ix - 1,
            offset_in_item: px(24.),
        });
        cx.notify();
    });
    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(400.), px(240.)),
        |_, _| sidebar.clone().into_any_element(),
    );
    cx.run_until_parked();

    let bounds_before = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .list_state
            .bounds_for_item(next_header_ix)
            .expect("next project header should be measured before metadata update")
    });

    save_thread_metadata(
        acp::SessionId::new(Arc::from("project-a-thread")),
        Some("Renamed Project A Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 1, 0).unwrap(),
        None,
        None,
        &project_a,
        cx,
    );

    let bounds_after = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .list_state
            .bounds_for_item(next_header_ix)
            .expect("same-shape metadata update should preserve next header measurements")
    });
    assert_eq!(bounds_before, bounds_after);
}

#[gpui::test]
async fn test_thread_status_update_does_not_reset_list_measurements(cx: &mut TestAppContext) {
    // When a thread's status changes (e.g. Running -> Completed after sending a message), the
    // shape sequence is unchanged, so `update_entries` should not reset the underlying
    // `ListState`. Resetting throws away measured item bounds for one frame, which makes the
    // sticky project header flicker between its pushed-off and fully-on-screen positions.
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(2, &project, cx).await;
    cx.run_until_parked();

    let before = sidebar.read_with(cx, |sidebar, app| {
        sidebar
            .entry_shapes(multi_workspace.read(app))
            .collect::<Vec<_>>()
    });
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    let after = sidebar.read_with(cx, |sidebar, app| {
        sidebar
            .entry_shapes(multi_workspace.read(app))
            .collect::<Vec<_>>()
    });

    assert_eq!(
        before, after,
        "a no-op rebuild should produce an identical shape sequence"
    );
}

#[gpui::test]
async fn test_collapse_changes_entry_shape(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(2, &project, cx).await;
    cx.run_until_parked();

    let project_group_key = project.read_with(cx, |project, cx| project.project_group_key(cx));

    let before = sidebar.read_with(cx, |sidebar, app| {
        sidebar
            .entry_shapes(multi_workspace.read(app))
            .collect::<Vec<_>>()
    });
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();
    let after = sidebar.read_with(cx, |sidebar, app| {
        sidebar
            .entry_shapes(multi_workspace.read(app))
            .collect::<Vec<_>>()
    });

    assert_ne!(
        before, after,
        "collapsing the project group should change the shape sequence so the list resets"
    );
}

#[gpui::test]
async fn test_serialization_round_trip(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(3, &project, cx).await;

    let project_group_key = project.read_with(cx, |project, cx| project.project_group_key(cx));

    // Set a custom width and collapse the group.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.set_width(Some(px(420.0)), cx);
        sidebar.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();

    // Capture the serialized state from the first sidebar.
    let serialized = sidebar.read_with(cx, |sidebar, cx| sidebar.serialized_state(cx));
    let serialized = serialized.expect("serialized_state should return Some");

    // Create a fresh sidebar and restore into it.
    let sidebar2 =
        cx.update(|window, cx| cx.new(|cx| Sidebar::new(multi_workspace.clone(), window, cx)));
    cx.run_until_parked();

    sidebar2.update_in(cx, |sidebar, window, cx| {
        sidebar.restore_serialized_state(&serialized, window, cx);
    });
    cx.run_until_parked();

    // Assert all serialized fields match.
    let width1 = sidebar.read_with(cx, |s, _| s.width);
    let width2 = sidebar2.read_with(cx, |s, _| s.width);

    assert_eq!(width1, width2);
    assert_eq!(width1, px(420.0));
}

#[gpui::test]
async fn test_restore_serialized_archive_view_does_not_panic(cx: &mut TestAppContext) {
    // A regression test to ensure that restoring a serialized archive view does not panic.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.update(|_window, cx| {
        AgentRegistryStore::init_test_global(cx, vec![]);
    });

    let serialized = serde_json::to_string(&SerializedSidebar {
        width: Some(400.0),
        active_view: SerializedSidebarView::History,
    })
    .expect("serialization should succeed");

    multi_workspace.update_in(cx, |multi_workspace, window, cx| {
        if let Some(sidebar) = multi_workspace.sidebar() {
            sidebar.restore_serialized_state(&serialized, window, cx);
        }
    });
    cx.run_until_parked();

    // After the deferred `show_archive` runs, the view should be Archive.
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(
            matches!(sidebar.view, SidebarView::Archive(_)),
            "expected sidebar view to be Archive after restore, got ThreadList"
        );
    });
}

#[gpui::test]
async fn test_entities_released_on_window_close(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let weak_workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().downgrade());
    let weak_sidebar = sidebar.downgrade();
    let weak_multi_workspace = multi_workspace.downgrade();

    drop(sidebar);
    drop(multi_workspace);
    cx.update(|window, _cx| window.remove_window());
    cx.run_until_parked();

    weak_multi_workspace.assert_released();
    weak_sidebar.assert_released();
    weak_workspace.assert_released();
}

#[gpui::test]
async fn test_single_workspace_no_threads(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let (_sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    assert_eq!(
        visible_entries_as_strings(&_sidebar, cx),
        vec!["v [my-project]"]
    );
}

#[gpui::test]
async fn test_single_workspace_with_saved_threads(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-1")),
        Some("Fix crash in project panel".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );

    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-2")),
        Some("Add inline diff view".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Fix crash in project panel",
            "  Add inline diff view",
        ]
    );
}

#[gpui::test]
async fn test_workspace_lifecycle(cx: &mut TestAppContext) {
    let project = init_test_project("/project-a", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Single workspace with a thread
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-a1")),
        Some("Thread A1".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project-a]",
            "  Thread A1",
        ]
    );

    // Add a second workspace
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.create_test_workspace(window, cx).detach();
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project-a]",
            "  Thread A1",
        ]
    );
}

#[gpui::test]
async fn test_collapse_and_expand_group(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;

    let project_group_key = project.read_with(cx, |project, cx| project.project_group_key(cx));

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread 1",
        ]
    );

    // Collapse
    sidebar.update_in(cx, |s, window, cx| {
        s.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "> [my-project]",
        ]
    );

    // Expand
    sidebar.update_in(cx, |s, window, cx| {
        s.toggle_collapse(&project_group_key, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread 1",
        ]
    );
}

#[gpui::test]
async fn test_collapse_state_survives_worktree_key_change(cx: &mut TestAppContext) {
    // When a worktree is added to a project, the project group key changes.
    // The sidebar's collapsed/expanded state is keyed by ProjectGroupKey, so
    // UI state must survive the key change.
    let (_fs, project) = init_multi_project_test(&["/project-a", "/project-b"], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(2, &project, cx).await;
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [project-a]", "  Thread 2", "  Thread 1",]
    );

    // Collapse the group.
    let old_key = project.read_with(cx, |project, cx| project.project_group_key(cx));
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.toggle_collapse(&old_key, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["> [project-a]"]
    );

    // Add a second worktree — the key changes from [/project-a] to
    // [/project-a, /project-b].
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    // The group should still be collapsed under the new key.
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["> [project-a, project-b]"]
    );
}

#[gpui::test]
async fn test_visible_entries_as_strings(cx: &mut TestAppContext) {
    use workspace::ProjectGroup;

    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let expanded_path = PathList::new(&[std::path::PathBuf::from("/expanded")]);
    let collapsed_path = PathList::new(&[std::path::PathBuf::from("/collapsed")]);

    // Set the collapsed group state through multi_workspace
    multi_workspace.update(cx, |mw, _cx| {
        mw.test_add_project_group(ProjectGroup {
            key: ProjectGroupKey::new(None, collapsed_path.clone()),
            workspaces: Vec::new(),
            expanded: false,
        });
    });

    sidebar.update_in(cx, |s, _window, _cx| {
        let notified_thread_id = ThreadId::new();
        s.contents.notified_threads.insert(notified_thread_id);
        s.contents.entries = vec![
            // Expanded project header
            ListEntry::ProjectHeader {
                key: ProjectGroupKey::new(None, expanded_path.clone()),
                label: "expanded-project".into(),
                highlight_positions: Vec::new(),
                has_running_threads: false,
                waiting_thread_count: 0,
                has_notifications: false,
                is_active: true,
                has_threads: true,
            },
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(acp::SessionId::new(Arc::from("t-1"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Completed thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::Completed,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: false,
                is_background: false,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Active thread with Running status
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(acp::SessionId::new(Arc::from("t-2"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Running thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::Running,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: true,
                is_background: false,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Active thread with Error status
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(acp::SessionId::new(Arc::from("t-3"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Error thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::Error,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: true,
                is_background: false,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Thread with WaitingForConfirmation status, not active
            // remote_connection: None,
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: ThreadId::new(),
                    session_id: Some(acp::SessionId::new(Arc::from("t-4"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Waiting thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::WaitingForConfirmation,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: false,
                is_background: false,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Background thread that completed (should show notification)
            // remote_connection: None,
            ListEntry::Thread(Arc::new(ThreadEntry {
                metadata: ThreadMetadata {
                    thread_id: notified_thread_id,
                    session_id: Some(acp::SessionId::new(Arc::from("t-5"))),
                    agent_id: AgentId::new("mav-agent"),
                    worktree_paths: WorktreePaths::default(),
                    title: Some("Notified thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: Some(Utc::now()),
                    interacted_at: None,
                    archived: false,
                    remote_connection: None,
                },
                icon: IconName::MavAgent,
                icon_from_external_svg: None,
                status: AgentThreadStatus::Completed,
                workspace: ThreadEntryWorkspace::Open(workspace.clone()),
                is_live: true,
                is_background: true,
                is_title_generating: false,
                draft: None,
                highlight_positions: Vec::new(),
                worktrees: Vec::new(),
                diff_stats: DiffStats::default(),
            })),
            // Collapsed project header
            ListEntry::ProjectHeader {
                key: ProjectGroupKey::new(None, collapsed_path.clone()),
                label: "collapsed-project".into(),
                highlight_positions: Vec::new(),
                has_running_threads: false,
                waiting_thread_count: 0,
                has_notifications: false,
                is_active: false,
                has_threads: false,
            },
        ];

        // Select the Running thread (index 2)
        s.selection = Some(2);
    });

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [expanded-project]",
            "  Completed thread",
            "  Running thread * (running)  <== selected",
            "  Error thread * (error)",
            "  Waiting thread (waiting)",
            "  Notified thread * (!)",
            "> [collapsed-project]",
        ]
    );

    // Move selection to the collapsed header
    sidebar.update_in(cx, |s, _window, _cx| {
        s.selection = Some(6);
    });

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx).last().cloned(),
        Some("> [collapsed-project]  <== selected".to_string()),
    );

    // Clear selection
    sidebar.update_in(cx, |s, _window, _cx| {
        s.selection = None;
    });

    // No entry should have the selected marker
    let entries = visible_entries_as_strings(&sidebar, cx);
    for entry in &entries {
        assert!(
            !entry.contains("<== selected"),
            "unexpected selection marker in: {}",
            entry
        );
    }
}

#[gpui::test]
async fn test_keyboard_select_next_and_previous(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(3, &project, cx).await;

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // Entries: [header, thread3, thread2, thread1]
    // Focusing the sidebar does not set a selection; select_next/select_previous
    // handle None gracefully by starting from the first or last entry.
    focus_sidebar(&sidebar, cx);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // First SelectNext from None starts at index 0
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // Move down through remaining entries
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));

    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(2));

    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(3));

    // At the end, wraps back to first entry
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // Navigate back to the end
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(2));
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(3));

    // Move back up
    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(2));

    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));

    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // At the top, selection clears (focus returns to editor)
    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);
}

#[gpui::test]
async fn test_keyboard_select_first_and_last(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(3, &project, cx).await;
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    focus_sidebar(&sidebar, cx);

    // SelectLast jumps to the end
    cx.dispatch_action(SelectLast);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(3));

    // SelectFirst jumps to the beginning
    cx.dispatch_action(SelectFirst);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));
}

#[gpui::test]
async fn test_keyboard_focus_in_does_not_set_selection(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Initially no selection
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // Open the sidebar so it's rendered, then focus it to trigger focus_in.
    // focus_in no longer sets a default selection.
    focus_sidebar(&sidebar, cx);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // Manually set a selection, blur, then refocus — selection should be preserved
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(0);
    });

    cx.update(|window, _cx| {
        window.blur();
    });
    cx.run_until_parked();

    sidebar.update_in(cx, |_, window, cx| {
        cx.focus_self(window);
    });
    cx.run_until_parked();
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));
}

#[gpui::test]
async fn test_keyboard_confirm_on_project_header_toggles_collapse(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread 1",
        ]
    );

    // Focus the sidebar and select the header
    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(0);
    });

    // Confirm on project header collapses the group
    cx.dispatch_action(Confirm);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "> [my-project]  <== selected",
        ]
    );

    // Confirm again expands the group
    cx.dispatch_action(Confirm);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]  <== selected",
            "  Thread 1",
        ]
    );
}

#[gpui::test]
async fn test_keyboard_expand_and_collapse_selected_entry(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread 1",
        ]
    );

    // Focus sidebar and manually select the header (index 0). Press left to collapse.
    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(0);
    });

    cx.dispatch_action(SelectParent);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "> [my-project]  <== selected",
        ]
    );

    // Press right to expand
    cx.dispatch_action(SelectChild);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]  <== selected",
            "  Thread 1",
        ]
    );

    // Press right again on already-expanded header moves selection down
    cx.dispatch_action(SelectChild);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));
}

#[gpui::test]
async fn test_keyboard_collapse_from_child_selects_parent(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // Focus sidebar (selection starts at None), then navigate down to the thread (child)
    focus_sidebar(&sidebar, cx);
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread 1  <== selected",
        ]
    );

    // Pressing left on a child collapses the parent group and selects it
    cx.dispatch_action(SelectParent);
    cx.run_until_parked();

    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "> [my-project]  <== selected",
        ]
    );
}

#[gpui::test]
async fn test_keyboard_navigation_on_empty_list(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/empty-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let (sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // An empty project has only the header (no auto-created draft).
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [empty-project]"]
    );

    // Focus sidebar — focus_in does not set a selection
    focus_sidebar(&sidebar, cx);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // First SelectNext from None starts at index 0 (header)
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // SelectNext with only one entry stays at index 0
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));

    // SelectPrevious from first entry clears selection (returns to editor)
    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), None);

    // SelectPrevious from None selects the last entry
    cx.dispatch_action(SelectPrevious);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(0));
}

#[gpui::test]
async fn test_new_entry_noops_without_open_project(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    cx.update(|cx| <dyn Fs>::set_global(fs.clone(), cx));
    let project = project::Project::test(fs, [], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    let workspace = multi_workspace.read_with(cx, |multi_workspace, _cx| {
        multi_workspace.workspace().clone()
    });

    assert!(
        !sidebar.read_with(cx, |sidebar, _cx| sidebar.contents.has_open_projects),
        "empty workspaces should be treated as having no open projects"
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.create_new_entry(&workspace, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert!(
            panel.active_conversation_view().is_none(),
            "sidebar should not create an agent thread without an open project"
        );
    });
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        Vec::<String>::new()
    );
}

#[gpui::test]
async fn test_selection_clamps_after_entry_removal(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_n_test_threads(1, &project, cx).await;
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // Focus sidebar (selection starts at None), navigate down to the thread (index 1)
    focus_sidebar(&sidebar, cx);
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(SelectNext);
    assert_eq!(sidebar.read_with(cx, |s, _| s.selection), Some(1));

    // Collapse the group, which removes the thread from the list
    cx.dispatch_action(SelectParent);
    cx.run_until_parked();

    // Selection should be clamped to the last valid index (0 = header)
    let selection = sidebar.read_with(cx, |s, _| s.selection);
    let entry_count = sidebar.read_with(cx, |s, _| s.contents.entries.len());
    assert!(
        selection.unwrap_or(0) < entry_count,
        "selection {} should be within bounds (entries: {})",
        selection.unwrap_or(0),
        entry_count,
    );
}

async fn init_test_project_with_agent_panel(
    worktree_path: &str,
    cx: &mut TestAppContext,
) -> Entity<project::Project> {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(worktree_path, serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    project::Project::test(fs, [worktree_path.as_ref()], cx).await
}

fn add_agent_panel(
    workspace: &Entity<Workspace>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<AgentPanel> {
    workspace.update_in(cx, |workspace, window, cx| {
        let panel = cx.new(|cx| AgentPanel::test_new(workspace, window, cx));
        workspace.add_panel(panel.clone(), window, cx);
        panel
    })
}

fn setup_sidebar_with_agent_panel(
    multi_workspace: &Entity<MultiWorkspace>,
    cx: &mut gpui::VisualTestContext,
) -> (Entity<Sidebar>, Entity<AgentPanel>) {
    let sidebar = setup_sidebar(multi_workspace, cx);
    let workspace = multi_workspace.read_with(cx, |mw, _cx| mw.workspace().clone());
    let panel = add_agent_panel(&workspace, cx);
    (sidebar, panel)
}

#[path = "sidebar_tests/agent_panel_terminals.rs"]
mod agent_panel_terminals;
#[path = "sidebar_tests/archive_cross_window.rs"]
mod archive_cross_window;
#[path = "sidebar_tests/archive_path_resolution.rs"]
mod archive_path_resolution;
#[path = "sidebar_tests/archive_worktree_cleanup.rs"]
mod archive_worktree_cleanup;
#[path = "sidebar_tests/draft_lifecycle.rs"]
mod draft_lifecycle;
#[path = "sidebar_tests/draft_removal.rs"]
mod draft_removal;
#[path = "sidebar_tests/draft_visibility.rs"]
mod draft_visibility;
#[path = "sidebar_tests/focused_thread.rs"]
mod focused_thread;
#[path = "sidebar_tests/historical_threads.rs"]
mod historical_threads;
#[path = "sidebar_tests/linked_worktree_archive.rs"]
mod linked_worktree_archive;
#[path = "sidebar_tests/linked_worktree_terminal_close.rs"]
mod linked_worktree_terminal_close;
#[path = "sidebar_tests/search.rs"]
mod search;
#[path = "sidebar_tests/thread_rename.rs"]
mod thread_rename;
#[path = "sidebar_tests/thread_switcher_ordering.rs"]
mod thread_switcher_ordering;
#[path = "sidebar_tests/worktree_activation.rs"]
mod worktree_activation;
#[path = "sidebar_tests/worktree_chips.rs"]
mod worktree_chips;
#[path = "sidebar_tests/worktree_discovery.rs"]
mod worktree_discovery;
#[path = "sidebar_tests/worktree_live_open.rs"]
mod worktree_live_open;
#[path = "sidebar_tests/worktree_restore_git.rs"]
mod worktree_restore_git;
#[path = "sidebar_tests/worktree_restore_sidebar.rs"]
mod worktree_restore_sidebar;

#[gpui::test]
async fn test_thread_switcher_can_activate_agent_panel_terminal(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let build_terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Build", true, window, cx)
        })
        .expect("build test terminal should be inserted");
    let server_terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Server", true, window, cx)
        })
        .expect("server test terminal should be inserted");
    cx.run_until_parked();

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    let (entry_terminal_ids, selected_terminal_id) = sidebar.read_with(cx, |sidebar, cx| {
        let switcher = sidebar
            .thread_switcher
            .as_ref()
            .expect("switcher should be open");
        let switcher = switcher.read(cx);
        let entry_terminal_ids = switcher
            .entries()
            .iter()
            .map(|entry| {
                entry
                    .terminal_id()
                    .expect("expected terminal switcher entry")
            })
            .collect::<Vec<_>>();
        let selected_terminal_id = switcher
            .selected_entry()
            .expect("switcher should have selected entry")
            .terminal_id()
            .expect("expected selected terminal switcher entry");
        (entry_terminal_ids, selected_terminal_id)
    });

    assert_eq!(entry_terminal_ids.len(), 2);
    assert!(entry_terminal_ids.contains(&build_terminal_id));
    assert!(entry_terminal_ids.contains(&server_terminal_id));

    sidebar.update_in(cx, |sidebar, window, cx| {
        let switcher = sidebar
            .thread_switcher
            .as_ref()
            .expect("switcher should be open");
        let focus = switcher.focus_handle(cx);
        focus.dispatch_action(&menu::Confirm, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert_eq!(panel.active_terminal_id(), Some(selected_terminal_id));
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(
            matches!(&sidebar.active_entry, Some(ActiveEntry::Terminal { terminal_id, .. }) if *terminal_id == selected_terminal_id),
            "expected selected terminal to become active, got {:?}",
            sidebar.active_entry,
        );
    });
}

#[gpui::test]
async fn test_thread_switcher_includes_terminal_metadata_for_open_project_group(
    cx: &mut TestAppContext,
) {
    let project = init_test_project_with_agent_panel("/project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Feature Terminal", true, window, cx)
        })
        .expect("test terminal should be inserted");
    panel.update_in(cx, |panel, window, cx| {
        panel.close_terminal(terminal_id, window, cx);
    });
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-newer")),
        Some("Newer Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-older")),
        Some("Older Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );

    let created_at = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap();
    let metadata = TerminalThreadMetadata {
        terminal_id,
        title: "Feature Terminal".into(),
        custom_title: None,
        created_at,
        worktree_paths: WorktreePaths::from_path_lists(
            PathList::new(&[PathBuf::from("/project")]),
            PathList::new(&[PathBuf::from("/project-feature")]),
        )
        .unwrap(),
        remote_connection: None,
        working_directory: None,
    };
    cx.update(|_, cx| {
        TerminalThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, cx| {
        let switcher = sidebar
            .thread_switcher
            .as_ref()
            .expect("switcher should be open");
        assert!(
            switcher
                .read(cx)
                .entries()
                .iter()
                .any(|entry| entry.terminal_id() == Some(terminal_id)),
            "terminal metadata row should be included like a closed thread row"
        );
    });
}

#[gpui::test]
async fn test_thread_switcher_preserves_closed_terminal_linked_worktree_workspace(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/worktrees/project/feature-a/project",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/project/feature-a/project"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    main_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Feature Terminal", true, window, cx)
        })
        .expect("test terminal should be inserted");
    panel.update_in(cx, |panel, window, cx| {
        panel.close_terminal(terminal_id, window, cx);
    });
    let created_at = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap();
    let worktree_folder_paths =
        PathList::new(&[PathBuf::from("/worktrees/project/feature-a/project")]);
    let metadata = TerminalThreadMetadata {
        terminal_id,
        title: "Feature Terminal".into(),
        custom_title: None,
        created_at,
        worktree_paths: WorktreePaths::from_path_lists(
            PathList::new(&[PathBuf::from("/project")]),
            worktree_folder_paths.clone(),
        )
        .unwrap(),
        remote_connection: None,
        working_directory: None,
    };
    cx.update(|_, cx| {
        TerminalThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });
    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "linked worktree workspace should start closed"
    );

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, cx| {
        let switcher = sidebar
            .thread_switcher
            .as_ref()
            .expect("switcher should be open");
        match switcher
            .read(cx)
            .selected_entry()
            .expect("switcher should select the terminal row by default")
        {
            ThreadSwitcherEntry::Terminal(entry) => {
                assert_eq!(entry.metadata.terminal_id, terminal_id);
                match &entry.workspace {
                    ThreadEntryWorkspace::Closed {
                        folder_paths,
                        project_group_key,
                    } => {
                        assert_eq!(folder_paths, &worktree_folder_paths);
                        assert_eq!(
                            project_group_key.path_list(),
                            &PathList::new(&[PathBuf::from("/project")])
                        );
                    }
                    ThreadEntryWorkspace::Open(_) => {
                        panic!("closed terminal row should retain its linked worktree target")
                    }
                }
            }
            ThreadSwitcherEntry::Thread(_) => {
                panic!("terminal row should be selected by default")
            }
        }
    });
}

#[gpui::test]
async fn test_archive_selected_terminal_archives_closed_linked_worktree(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/worktrees/project/feature-a/project",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/project/feature-a/project"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/project/feature-a/project"),
        None,
        cx,
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    main_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Feature Terminal", true, window, cx)
        })
        .expect("test terminal should be inserted");
    panel.update_in(cx, |panel, window, cx| {
        panel.close_terminal(terminal_id, window, cx);
    });
    let worktree_folder_paths =
        PathList::new(&[PathBuf::from("/worktrees/project/feature-a/project")]);
    let metadata = TerminalThreadMetadata {
        terminal_id,
        title: "Feature Terminal".into(),
        custom_title: None,
        created_at: chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        worktree_paths: WorktreePaths::from_path_lists(
            PathList::new(&[PathBuf::from("/project")]),
            worktree_folder_paths.clone(),
        )
        .unwrap(),
        remote_connection: None,
        working_directory: None,
    };
    cx.update(|_, cx| {
        TerminalThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    });
    let empty_draft_id = save_draft_metadata_with_main_paths(
        None,
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        cx,
    );
    cx.update(|_, cx| {
        assert!(
            agent_ui::draft_prompt_store::read(empty_draft_id, cx).is_none(),
            "empty draft should not have persisted prompt content"
        );
    });
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    let terminal_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| matches!(entry, ListEntry::Terminal(terminal) if terminal.metadata.terminal_id == terminal_id))
            .expect("terminal should be visible in sidebar")
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        match &sidebar.contents.entries[terminal_index] {
            ListEntry::Terminal(terminal) => match &terminal.workspace {
                ThreadEntryWorkspace::Closed { folder_paths, .. } => {
                    assert_eq!(folder_paths, &worktree_folder_paths);
                }
                ThreadEntryWorkspace::Open(_) => {
                    panic!("linked worktree terminal should start closed")
                }
            },
            _ => panic!("expected terminal row"),
        }
    });

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(terminal_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    for _ in 0..8 {
        cx.run_until_parked();
    }

    let terminal_metadata_deleted = cx.update(|_, cx| {
        TerminalThreadMetadataStore::global(cx)
            .read(cx)
            .entry(terminal_id)
            .is_none()
    });
    assert!(
        terminal_metadata_deleted,
        "terminal metadata should be deleted after closing from the sidebar"
    );
    let empty_draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(empty_draft_id)
            .is_none()
    });
    assert!(
        empty_draft_metadata_deleted,
        "empty draft metadata should be deleted before archiving the linked worktree"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "temporary linked worktree workspace should be removed after archiving"
    );
    assert_eq!(
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace
            .workspaces()
            .count()),
        1,
        "closing a closed linked worktree terminal should leave only the main workspace"
    );
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from disk after closing its terminal"
    );
}

#[gpui::test]
async fn test_archive_selected_thread_archives_closed_linked_worktree(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/worktrees/project/feature-a/project",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/project/feature-a/project"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/project/feature-a/project"),
        None,
        cx,
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    main_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_session_id = acp::SessionId::new(Arc::from("worktree-thread"));
    let worktree_folder_paths =
        PathList::new(&[PathBuf::from("/worktrees/project/feature-a/project")]);
    save_thread_metadata_with_main_paths(
        "worktree-thread",
        "Worktree Thread",
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        cx,
    );
    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );
    let empty_draft_id = save_draft_metadata_with_main_paths(
        None,
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        cx,
    );
    cx.update(|_, cx| {
        assert!(
            agent_ui::draft_prompt_store::read(empty_draft_id, cx).is_none(),
            "empty draft should not have persisted prompt content"
        );
    });
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    let thread_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| matches!(entry, ListEntry::Thread(thread) if thread.metadata.session_id.as_ref() == Some(&worktree_session_id)))
            .expect("worktree thread should be visible in sidebar")
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        match &sidebar.contents.entries[thread_index] {
            ListEntry::Thread(thread) => match &thread.workspace {
                ThreadEntryWorkspace::Closed { folder_paths, .. } => {
                    assert_eq!(folder_paths, &worktree_folder_paths);
                }
                ThreadEntryWorkspace::Open(_) => {
                    panic!("linked worktree thread should start closed")
                }
            },
            _ => panic!("expected thread row"),
        }
    });

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(thread_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    for _ in 0..8 {
        cx.run_until_parked();
    }

    let thread_archived = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&worktree_session_id)
            .map(|thread| thread.archived)
    });
    assert_eq!(
        thread_archived,
        Some(true),
        "thread metadata should remain archived after worktree archival"
    );
    let empty_draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(empty_draft_id)
            .is_none()
    });
    assert!(
        empty_draft_metadata_deleted,
        "empty draft metadata should be deleted before archiving the linked worktree"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "temporary linked worktree workspace should be removed after archiving"
    );
    assert_eq!(
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace
            .workspaces()
            .count()),
        1,
        "archiving a closed linked worktree thread should leave only the main workspace"
    );
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from disk after archiving its thread"
    );
}

#[gpui::test]
async fn test_archive_selected_thread_deletes_empty_draft_when_linked_worktree_has_no_archive_root(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    fs.insert_branches(Path::new("/project/.git"), &["main", "feature-a"]);
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/external-worktree"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    main_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_session_id = acp::SessionId::new(Arc::from("external-worktree-thread"));
    let worktree_folder_paths = PathList::new(&[PathBuf::from("/external-worktree")]);
    save_thread_metadata_with_main_paths(
        "external-worktree-thread",
        "External Worktree Thread",
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        cx,
    );
    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );
    let empty_draft_id = save_draft_metadata_with_main_paths(
        None,
        worktree_folder_paths.clone(),
        PathList::new(&[PathBuf::from("/project")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        cx,
    );
    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    let thread_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| matches!(entry, ListEntry::Thread(thread) if thread.metadata.session_id.as_ref() == Some(&worktree_session_id)))
            .expect("worktree thread should be visible in sidebar")
    });
    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(thread_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    for _ in 0..8 {
        cx.run_until_parked();
    }

    let thread_archived = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&worktree_session_id)
            .map(|thread| thread.archived)
    });
    assert_eq!(
        thread_archived,
        Some(true),
        "thread metadata should remain archived after workspace removal"
    );
    let empty_draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(empty_draft_id)
            .is_none()
    });
    assert!(
        empty_draft_metadata_deleted,
        "empty draft metadata should be deleted when removing the linked worktree workspace"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "linked worktree workspace should be removed after archiving its last thread"
    );
    assert!(
        fs.is_dir(Path::new("/external-worktree")).await,
        "external linked worktree directory should remain on disk when no archive root is produced"
    );
}

#[gpui::test]
async fn test_archive_selected_thread_closes_selected_agent_panel_terminal(
    cx: &mut TestAppContext,
) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Dev Server", true, window, cx)
        })
        .expect("test terminal should be inserted");
    cx.run_until_parked();

    focus_sidebar(&sidebar, cx);
    let terminal_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| matches!(entry, ListEntry::Terminal(terminal) if terminal.metadata.terminal_id == terminal_id))
            .expect("terminal should be visible in sidebar")
    });
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(terminal_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert!(!panel.has_terminal(terminal_id));
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(sidebar.contents.entries.iter().all(|entry| {
            !matches!(entry, ListEntry::Terminal(terminal) if terminal.metadata.terminal_id == terminal_id)
        }));
    });
    sidebar.read_with(cx, |_sidebar, cx| {
        let store = TerminalThreadMetadataStore::global(cx).read(cx);
        assert!(
            store.entry(terminal_id).is_none(),
            "terminal metadata should be deleted when closing from the sidebar"
        );
    });
}

#[gpui::test]
async fn test_closing_active_agent_panel_terminal_activates_neighbor(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    let build_terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Build", true, window, cx)
        })
        .expect("build test terminal should be inserted");
    let server_terminal_id = panel
        .update_in(cx, |panel, window, cx| {
            panel.insert_test_terminal("Server", true, window, cx)
        })
        .expect("server test terminal should be inserted");
    cx.run_until_parked();

    let (server_metadata, server_workspace) = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .find_map(|entry| match entry {
                ListEntry::Terminal(terminal)
                    if terminal.metadata.terminal_id == server_terminal_id =>
                {
                    Some((terminal.metadata.clone(), terminal.workspace.clone()))
                }
                _ => None,
            })
            .expect("server terminal should be visible in sidebar")
    });
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.close_terminal(&server_metadata, &server_workspace, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, _cx| {
        assert!(!panel.has_terminal(server_terminal_id));
        assert_eq!(panel.active_terminal_id(), Some(build_terminal_id));
    });
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(
            matches!(&sidebar.active_entry, Some(ActiveEntry::Terminal { terminal_id, .. }) if *terminal_id == build_terminal_id),
            "expected remaining terminal to become active, got {:?}",
            sidebar.active_entry,
        );
    });
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [my-project]", "  Build"]
    );
}

#[gpui::test]
async fn test_parallel_threads_shown_with_live_status(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Open thread A and keep it generating.
    let connection = StubAgentConnection::new();
    open_thread_with_connection(&panel, connection.clone(), cx);
    send_message(&panel, cx);

    let session_id_a = active_session_id(&panel, cx);
    save_test_thread_metadata(&session_id_a, &project, cx).await;

    cx.update(|_, cx| {
        connection.send_update(
            session_id_a.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("working...".into())),
            cx,
        );
    });
    cx.run_until_parked();

    // Open thread B (idle, default response) — thread A goes to background.
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);

    let session_id_b = active_session_id(&panel, cx);
    save_test_thread_metadata(&session_id_b, &project, cx).await;

    cx.run_until_parked();

    let mut entries = visible_entries_as_strings(&sidebar, cx);
    entries[1..].sort();
    assert_eq!(
        entries,
        vec![
            //
            "v [my-project]",
            "  Hello *",
            "  Hello * (running)",
        ]
    );
}

#[gpui::test]
async fn test_subagent_permission_request_marks_parent_sidebar_thread_waiting(
    cx: &mut TestAppContext,
) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let connection = StubAgentConnection::new().with_supports_load_session(true);
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);

    let parent_session_id = active_session_id(&panel, cx);
    save_test_thread_metadata(&parent_session_id, &project, cx).await;

    let subagent_session_id = acp::SessionId::new("subagent-session");
    cx.update(|_, cx| {
        let parent_thread = panel.read(cx).active_agent_thread(cx).unwrap();
        parent_thread.update(cx, |thread: &mut AcpThread, cx| {
            thread.subagent_spawned(subagent_session_id.clone(), cx);
        });
    });
    cx.run_until_parked();

    let subagent_thread = panel.read_with(cx, |panel, cx| {
        panel
            .active_conversation_view()
            .and_then(|conversation| conversation.read(cx).thread_view(&subagent_session_id))
            .map(|thread_view| thread_view.read(cx).thread.clone())
            .expect("Expected subagent thread to be loaded into the conversation")
    });
    request_test_tool_authorization(&subagent_thread, "subagent-tool-call", "allow-subagent", cx);

    let parent_status = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .find_map(|entry| match entry {
                ListEntry::Thread(thread)
                    if thread.metadata.session_id.as_ref() == Some(&parent_session_id) =>
                {
                    Some(thread.status)
                }
                _ => None,
            })
            .expect("Expected parent thread entry in sidebar")
    });

    assert_eq!(parent_status, AgentThreadStatus::WaitingForConfirmation);
}

#[gpui::test]
async fn test_background_thread_completion_triggers_notification(cx: &mut TestAppContext) {
    let project_a = init_test_project_with_agent_panel("/project-a", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let (sidebar, panel_a) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Open thread on workspace A and keep it generating.
    let connection_a = StubAgentConnection::new();
    open_thread_with_connection(&panel_a, connection_a.clone(), cx);
    send_message(&panel_a, cx);

    let session_id_a = active_session_id(&panel_a, cx);
    save_test_thread_metadata(&session_id_a, &project_a, cx).await;

    cx.update(|_, cx| {
        connection_a.send_update(
            session_id_a.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("chunk".into())),
            cx,
        );
    });
    cx.run_until_parked();

    // Add a second workspace and activate it (making workspace A the background).
    let fs = cx.update(|_, cx| <dyn fs::Fs>::global(cx));
    let project_b = project::Project::test(fs, [], cx).await;
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b, window, cx);
    });
    cx.run_until_parked();

    // Thread A is still running; no notification yet.
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project-a]",
            "  Hello * (running)",
        ]
    );

    // Complete thread A's turn (transition Running → Completed).
    connection_a.end_turn(session_id_a.clone(), acp::StopReason::EndTurn);
    cx.run_until_parked();

    // The completed background thread shows a notification indicator.
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [project-a]",
            "  Hello * (!)",
        ]
    );
}

#[gpui::test]
async fn test_click_clears_selection_and_focus_in_restores_it(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_thread_metadata(
        acp::SessionId::new(Arc::from("t-1")),
        Some("Thread A".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );

    save_thread_metadata(
        acp::SessionId::new(Arc::from("t-2")),
        Some("Thread B".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );

    cx.run_until_parked();
    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            //
            "v [my-project]",
            "  Thread A",
            "  Thread B",
        ]
    );

    // Keyboard confirm preserves selection.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.selection = Some(1);
        sidebar.confirm(&Confirm, window, cx);
    });
    assert_eq!(
        sidebar.read_with(cx, |sidebar, _| sidebar.selection),
        Some(1)
    );

    // Click handlers clear selection to None so no highlight lingers
    // after a click regardless of focus state. The hover style provides
    // visual feedback during mouse interaction instead.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.selection = None;
        let path_list = PathList::new(&[std::path::PathBuf::from("/my-project")]);
        let project_group_key = ProjectGroupKey::new(None, path_list);
        sidebar.toggle_collapse(&project_group_key, window, cx);
    });
    assert_eq!(sidebar.read_with(cx, |sidebar, _| sidebar.selection), None);

    // When the user tabs back into the sidebar, focus_in no longer
    // restores selection — it stays None.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.focus_in(window, cx);
    });
    assert_eq!(sidebar.read_with(cx, |sidebar, _| sidebar.selection), None);
}

async fn init_test_project_with_git(
    worktree_path: &str,
    cx: &mut TestAppContext,
) -> (Entity<project::Project>, Arc<dyn fs::Fs>) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        worktree_path,
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project = project::Project::test(fs.clone(), [worktree_path.as_ref()], cx).await;
    (project, fs)
}

#[gpui::test]
async fn test_archive_thread_keeps_metadata_but_hides_from_sidebar(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-to-archive")),
        Some("Thread To Archive".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries.iter().any(|e| e.contains("Thread To Archive")),
        "expected thread to be visible before archiving, got: {entries:?}"
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(
            &acp::SessionId::new(Arc::from("thread-to-archive")),
            window,
            cx,
        );
    });
    cx.run_until_parked();

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries.iter().any(|e| e.contains("Thread To Archive")),
        "expected thread to be hidden after archiving, got: {entries:?}"
    );

    cx.update(|_, cx| {
        let store = ThreadMetadataStore::global(cx);
        let archived: Vec<_> = store.read(cx).archived_entries().collect();
        assert_eq!(archived.len(), 1);
        assert_eq!(
            archived[0].session_id.as_ref().unwrap().0.as_ref(),
            "thread-to-archive"
        );
        assert!(archived[0].archived);
    });
}

#[gpui::test]
async fn test_archive_thread_drops_retained_conversation_view(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/project-a", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);
    let session_id = active_session_id(&panel, cx);
    let thread_id = active_thread_id(&panel, cx);
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _| {
        assert!(
            is_active_session(sidebar, &session_id),
            "expected the newly created thread to be active before archiving",
        );
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&session_id, window, cx);
    });
    cx.run_until_parked();

    panel.read_with(cx, |panel, _| {
        assert!(
            !panel.is_retained_thread(&thread_id),
            "archiving a thread must drop its ConversationView from retained_threads, \
             but the archived thread id {thread_id:?} is still retained",
        );
    });
}

#[gpui::test]
async fn test_archive_thread_active_entry_management(cx: &mut TestAppContext) {
    // Tests two archive scenarios:
    // 1. Archiving a thread in a non-active workspace leaves active_entry
    //    as the current draft.
    // 2. Archiving the thread the user is looking at falls back to a draft
    //    on the same workspace.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let (sidebar, panel_a) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    // Explicitly create a draft on workspace_b so the sidebar tracks one.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.create_new_thread(&workspace_b, window, cx);
    });
    cx.run_until_parked();

    // --- Scenario 1: archive a thread in the non-active workspace ---

    // Create a thread in project-a (non-active — project-b is active).
    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel_a, connection, cx);
    agent_ui::test_support::send_message(&panel_a, cx);
    let thread_a = agent_ui::test_support::active_session_id(&panel_a, cx);
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&thread_a, window, cx);
    });
    cx.run_until_parked();

    // active_entry should still be a draft on workspace_b (the active one).
    sidebar.read_with(cx, |sidebar, _| {
        assert!(
            matches!(&sidebar.active_entry, Some(ActiveEntry::Thread { workspace: ws, .. }) if ws == &workspace_b),
            "expected Draft(workspace_b) after archiving non-active thread, got: {:?}",
            sidebar.active_entry,
        );
    });

    // --- Scenario 2: archive the thread the user is looking at ---

    // Create a thread in project-b (the active workspace) and verify it
    // becomes the active entry.
    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel_b, connection, cx);
    agent_ui::test_support::send_message(&panel_b, cx);
    let thread_b = agent_ui::test_support::active_session_id(&panel_b, cx);
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _| {
        assert!(
            is_active_session(&sidebar, &thread_b),
            "expected active_entry to be Thread({thread_b}), got: {:?}",
            sidebar.active_entry,
        );
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&thread_b, window, cx);
    });
    cx.run_until_parked();

    // Archiving the active thread activates a draft on the same workspace
    // (via clear_base_view → activate_draft). The draft is not shown as a
    // sidebar row but active_entry tracks it.
    sidebar.read_with(cx, |sidebar, _| {
        assert!(
            matches!(&sidebar.active_entry, Some(ActiveEntry::Thread { workspace: ws, .. }) if ws == &workspace_b),
            "expected draft on workspace_b after archiving active thread, got: {:?}",
            sidebar.active_entry,
        );
    });
}

#[gpui::test]
async fn test_unarchive_only_shows_restored_thread(cx: &mut TestAppContext) {
    // Full flow: create a thread, archive it (removing the workspace),
    // then unarchive. Only the restored thread should appear — no
    // leftover drafts or previously-serialized threads.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    // Create a thread and send a message so it's a real thread.
    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Hello".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel, connection, cx);
    agent_ui::test_support::send_message(&panel, cx);
    let session_id = agent_ui::test_support::active_session_id(&panel, cx);
    cx.run_until_parked();

    // Archive it.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&session_id, window, cx);
    });
    cx.run_until_parked();

    // Grab metadata for unarchive.
    let thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("thread should exist")
    });
    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(thread_id)
            .cloned()
            .expect("metadata should exist")
    });

    // Unarchive it — the draft should be replaced by the restored thread.
    let restored_title = metadata.display_title();
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });
    cx.run_until_parked();

    // The restored thread should be visible. A fresh draft may also be
    // visible as a sidebar row: archive_thread auto-activates one via
    // clear_base_view, and the unarchive then parks it by pushing the
    // restored thread into the base view.
    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries.iter().any(|e| e.contains(restored_title.as_ref())),
        "expected the restored thread to be visible, got entries: {entries:?}"
    );
    let thread_count = entries
        .iter()
        .filter(|e| !e.starts_with("v ") && !e.starts_with("> "))
        .count();
    assert!(
        thread_count <= 2,
        "expected at most the restored thread plus a parked draft, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_first_thread_in_group_does_not_create_spurious_draft(
    cx: &mut TestAppContext,
) {
    // When a thread is unarchived into a project group that has no open
    // workspace, the sidebar opens a new workspace and loads the thread.
    // No spurious draft should appear alongside the unarchived thread.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    cx.run_until_parked();

    // Save an archived thread whose folder_paths point to project-b,
    // which has no open workspace.
    let session_id = acp::SessionId::new(Arc::from("archived-thread"));
    let path_list_b = PathList::new(&[std::path::PathBuf::from("/project-b")]);
    let thread_id = ThreadId::new();
    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id,
                    session_id: Some(session_id.clone()),
                    agent_id: agent::MAV_AGENT_ID.clone(),
                    title: Some("Unarchived Thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: None,
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_folder_paths(&path_list_b),
                    archived: true,
                    remote_connection: None,
                },
                cx,
            )
        });
    });
    cx.run_until_parked();

    // Verify no workspace for project-b exists yet.
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "should start with only the project-a workspace"
    );

    // Un-archive the thread — should open project-b workspace and load it.
    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(thread_id)
            .cloned()
            .expect("metadata should exist")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });
    cx.run_until_parked();

    // A second workspace should have been created for project-b.
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "should have opened a workspace for the unarchived thread"
    );

    // The sidebar should show the unarchived thread without a spurious draft
    // in the project-b group.
    let entries = visible_entries_as_strings(&sidebar, cx);
    let draft_count = entries.iter().filter(|e| e.contains("Draft")).count();
    // project-a gets a draft (it's the active workspace with no threads),
    // but project-b should NOT have one — only the unarchived thread.
    assert!(
        draft_count <= 1,
        "expected at most one draft (for project-a), got entries: {entries:?}"
    );
    assert!(
        entries.iter().any(|e| e.contains("Unarchived Thread")),
        "expected unarchived thread to appear, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_into_new_workspace_does_not_create_duplicate_real_thread(
    cx: &mut TestAppContext,
) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    cx.run_until_parked();

    let session_id = acp::SessionId::new(Arc::from("restore-into-new-workspace"));
    let path_list_b = PathList::new(&[PathBuf::from("/project-b")]);
    let original_thread_id = ThreadId::new();
    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id: original_thread_id,
                    session_id: Some(session_id.clone()),
                    agent_id: agent::MAV_AGENT_ID.clone(),
                    title: Some("Unarchived Thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: None,
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_folder_paths(&path_list_b),
                    archived: true,
                    remote_connection: None,
                },
                cx,
            )
        });
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(original_thread_id)
            .cloned()
            .expect("metadata should exist before unarchive")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });

    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "expected unarchive to open the target workspace"
    );

    let restored_workspace = multi_workspace.read_with(cx, |mw, cx| {
        mw.workspaces()
            .find(|workspace| PathList::new(&workspace.read(cx).root_paths(cx)) == path_list_b)
            .cloned()
            .expect("expected restored workspace for unarchived thread")
    });
    let restored_panel = restored_workspace.read_with(cx, |workspace, cx| {
        workspace
            .panel::<AgentPanel>(cx)
            .expect("expected unarchive to install an agent panel in the new workspace")
    });

    let restored_thread_id = restored_panel.read_with(cx, |panel, cx| panel.active_thread_id(cx));
    assert_eq!(
        restored_thread_id,
        Some(original_thread_id),
        "expected the new workspace's agent panel to target the restored archived thread id"
    );

    let session_entries = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .filter(|entry| entry.session_id.as_ref() == Some(&session_id))
            .cloned()
            .collect::<Vec<_>>()
    });
    assert_eq!(
        session_entries.len(),
        1,
        "expected exactly one metadata row for restored session after opening a new workspace, got: {session_entries:?}"
    );
    assert_eq!(
        session_entries[0].thread_id, original_thread_id,
        "expected restore into a new workspace to reuse the original thread id"
    );
    assert!(
        !session_entries[0].archived,
        "expected restored thread metadata to be unarchived, got: {:?}",
        session_entries[0]
    );

    let mapped_thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
    });
    assert_eq!(
        mapped_thread_id,
        Some(original_thread_id),
        "expected session mapping to remain stable after opening the new workspace"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    let real_thread_rows = entries
        .iter()
        .filter(|entry| !entry.starts_with("v ") && !entry.starts_with("> "))
        .filter(|entry| !entry.contains("Draft"))
        .count();
    assert_eq!(
        real_thread_rows, 1,
        "expected exactly one visible real thread row after restore into a new workspace, got entries: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.contains("Unarchived Thread")),
        "expected restored thread row to be visible, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_into_existing_workspace_replaces_draft(cx: &mut TestAppContext) {
    // When a workspace already exists with an empty draft and a thread
    // is unarchived into it, the draft should be replaced — not kept
    // alongside the loaded thread.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/my-project", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project = project::Project::test(fs.clone(), ["/my-project".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    // Create a thread and send a message so it's no longer a draft.
    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel, connection, cx);
    agent_ui::test_support::send_message(&panel, cx);
    let session_id = agent_ui::test_support::active_session_id(&panel, cx);
    cx.run_until_parked();

    // Archive the thread — the group is left empty (no draft created).
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&session_id, window, cx);
    });
    cx.run_until_parked();

    // Un-archive the thread.
    let thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("thread should exist in store")
    });
    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(thread_id)
            .cloned()
            .expect("metadata should exist")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });
    cx.run_until_parked();

    // The draft should be gone — only the unarchived thread remains.
    let entries = visible_entries_as_strings(&sidebar, cx);
    let draft_count = entries.iter().filter(|e| e.contains("Draft")).count();
    assert_eq!(
        draft_count, 0,
        "expected no drafts after unarchiving, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_into_inactive_existing_workspace_does_not_leave_active_draft(
    cx: &mut TestAppContext,
) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    let session_id = acp::SessionId::new(Arc::from("unarchive-into-inactive-existing-workspace"));
    let thread_id = ThreadId::new();
    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id,
                    session_id: Some(session_id.clone()),
                    agent_id: agent::MAV_AGENT_ID.clone(),
                    title: Some("Restored In Inactive Workspace".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: None,
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[
                        PathBuf::from("/project-b"),
                    ])),
                    archived: true,
                    remote_connection: None,
                },
                cx,
            )
        });
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(thread_id)
            .cloned()
            .expect("archived metadata should exist before restore")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });

    let panel_b_before_settle = workspace_b.read_with(cx, |workspace, cx| {
        workspace.panel::<AgentPanel>(cx).expect(
            "target workspace should still have an agent panel immediately after activation",
        )
    });
    let immediate_active_thread_id =
        panel_b_before_settle.read_with(cx, |panel, cx| panel.active_thread_id(cx));

    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id,
            "unarchiving into an inactive existing workspace should end on the restored thread",
        );
    });

    let panel_b = workspace_b.read_with(cx, |workspace, cx| {
        workspace
            .panel::<AgentPanel>(cx)
            .expect("target workspace should still have an agent panel")
    });
    assert_eq!(
        panel_b.read_with(cx, |panel, cx| panel.active_thread_id(cx)),
        Some(thread_id),
        "expected target panel to activate the restored thread id"
    );
    assert!(
        immediate_active_thread_id.is_none() || immediate_active_thread_id == Some(thread_id),
        "expected immediate panel state to be either still loading or already on the restored thread, got active_thread_id={immediate_active_thread_id:?}"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    let target_rows: Vec<_> = entries
        .iter()
        .filter(|entry| entry.contains("Restored In Inactive Workspace") || entry.contains("Draft"))
        .cloned()
        .collect();
    assert_eq!(
        target_rows.len(),
        1,
        "expected only the restored row and no surviving draft in the target group, got entries: {entries:?}"
    );
    assert!(
        target_rows[0].contains("Restored In Inactive Workspace"),
        "expected the remaining row to be the restored thread, got entries: {entries:?}"
    );
    assert!(
        !target_rows[0].contains("Draft"),
        "expected no surviving draft row after unarchive into inactive existing workspace, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_after_removing_parent_project_group_restores_real_thread(
    cx: &mut TestAppContext,
) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel_b, connection, cx);
    agent_ui::test_support::send_message(&panel_b, cx);
    let session_id = agent_ui::test_support::active_session_id(&panel_b, cx);
    save_test_thread_metadata(&session_id, &project_b, cx).await;
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&session_id, window, cx);
    });

    cx.run_until_parked();

    let archived_metadata = cx.update(|_, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        let thread_id = store
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("archived thread should still exist in metadata store");
        let metadata = store
            .entry(thread_id)
            .cloned()
            .expect("archived metadata should still exist after archive");
        assert!(
            metadata.archived,
            "thread should be archived before project removal"
        );
        metadata
    });

    let group_key_b =
        project_b.read_with(cx, |project, cx| ProjectGroupKey::from_project(project, cx));
    let remove_task = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.remove_project_group(&group_key_b, window, cx)
    });
    remove_task
        .await
        .expect("remove project group task should complete");
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "removing the archived thread's parent project group should remove its workspace"
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(archived_metadata.clone(), window, cx);
    });
    cx.run_until_parked();

    let restored_workspace = multi_workspace.read_with(cx, |mw, cx| {
        mw.workspaces()
            .find(|workspace| {
                PathList::new(&workspace.read(cx).root_paths(cx))
                    == PathList::new(&[PathBuf::from("/project-b")])
            })
            .cloned()
            .expect("expected unarchive to recreate the removed project workspace")
    });
    let restored_panel = restored_workspace.read_with(cx, |workspace, cx| {
        workspace
            .panel::<AgentPanel>(cx)
            .expect("expected restored workspace to bootstrap an agent panel")
    });

    let restored_thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("session should still map to restored thread id")
    });
    assert_eq!(
        restored_panel.read_with(cx, |panel, cx| panel.active_thread_id(cx)),
        Some(restored_thread_id),
        "expected unarchive after project removal to activate the restored real thread"
    );

    sidebar.read_with(cx, |sidebar, _cx| {
        assert_active_thread(
            sidebar,
            &session_id,
            "expected sidebar active entry to track the restored thread after project removal",
        );
    });

    let entries = visible_entries_as_strings(&sidebar, cx);
    let restored_title = archived_metadata.display_title().to_string();
    let matching_rows: Vec<_> = entries
        .iter()
        .filter(|entry| entry.contains(&restored_title) || entry.contains("Draft"))
        .cloned()
        .collect();
    assert_eq!(
        matching_rows.len(),
        1,
        "expected only one restored row and no surviving draft after unarchive following project removal, got entries: {entries:?}"
    );
    assert!(
        !matching_rows[0].contains("Draft"),
        "expected no draft row after unarchive following project removal, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_unarchive_does_not_create_duplicate_real_thread_metadata(cx: &mut TestAppContext) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/my-project", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project = project::Project::test(fs.clone(), ["/my-project".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);
    cx.run_until_parked();

    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel, connection, cx);
    agent_ui::test_support::send_message(&panel, cx);
    let session_id = agent_ui::test_support::active_session_id(&panel, cx);
    cx.run_until_parked();

    let original_thread_id = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .find(|e| e.session_id.as_ref() == Some(&session_id))
            .map(|e| e.thread_id)
            .expect("thread should exist in store before archiving")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&session_id, window, cx);
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(original_thread_id)
            .cloned()
            .expect("metadata should exist after archiving")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });
    cx.run_until_parked();

    let session_entries = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .filter(|entry| entry.session_id.as_ref() == Some(&session_id))
            .cloned()
            .collect::<Vec<_>>()
    });

    assert_eq!(
        session_entries.len(),
        1,
        "expected exactly one metadata row for the restored session, got: {session_entries:?}"
    );
    assert_eq!(
        session_entries[0].thread_id, original_thread_id,
        "expected unarchive to reuse the original thread id instead of creating a duplicate row"
    );
    assert!(
        session_entries[0].session_id.is_some(),
        "expected restored metadata to be a real thread, got: {:?}",
        session_entries[0]
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    let real_thread_rows = entries
        .iter()
        .filter(|entry| !entry.starts_with("v ") && !entry.starts_with("> "))
        .filter(|entry| !entry.contains("Draft"))
        // Parked drafts render with the default title until the user types.
        .filter(|entry| !entry.contains(DEFAULT_THREAD_TITLE))
        .count();
    assert_eq!(
        real_thread_rows, 1,
        "expected exactly one visible real thread row after unarchive, got entries: {entries:?}"
    );
    assert!(
        !entries.iter().any(|entry| entry.contains("Draft")),
        "expected no draft rows after restoring, got entries: {entries:?}"
    );
}

#[gpui::test]
async fn test_switch_to_workspace_with_archived_thread_shows_no_active_entry(
    cx: &mut TestAppContext,
) {
    // When a thread is archived while the user is in a different workspace,
    // clear_base_view creates a draft on the archived workspace's panel.
    // Switching back to that workspace shows the draft as active_entry.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let (sidebar, panel_a) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    // Create a thread in project-a's panel (currently non-active).
    let connection = acp_thread::StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    agent_ui::test_support::open_thread_with_connection(&panel_a, connection, cx);
    agent_ui::test_support::send_message(&panel_a, cx);
    let thread_a = agent_ui::test_support::active_session_id(&panel_a, cx);
    cx.run_until_parked();

    // Archive it while project-b is active.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&thread_a, window, cx);
    });
    cx.run_until_parked();

    // Switch back to project-a. Its panel was cleared during archiving
    // (clear_base_view activated a draft), so active_entry should point
    // to the draft on workspace_a.
    let workspace_a =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, _window, cx| {
        sidebar.update_entries(cx);
    });
    cx.run_until_parked();

    sidebar.read_with(cx, |sidebar, _| {
        assert_active_draft(
            sidebar,
            &workspace_a,
            "after switching to workspace with archived thread, active_entry should be the draft",
        );
    });
}

#[gpui::test]
async fn test_archived_threads_excluded_from_sidebar_entries(cx: &mut TestAppContext) {
    let project = init_test_project("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    save_thread_metadata(
        acp::SessionId::new(Arc::from("visible-thread")),
        Some("Visible Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );

    let archived_thread_session_id = acp::SessionId::new(Arc::from("archived-thread"));
    save_thread_metadata(
        archived_thread_session_id.clone(),
        Some("Archived Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );

    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            let thread_id = store
                .entries()
                .find(|e| e.session_id.as_ref() == Some(&archived_thread_session_id))
                .map(|e| e.thread_id)
                .unwrap();
            store.archive(thread_id, None, cx)
        })
    });
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries.iter().any(|e| e.contains("Visible Thread")),
        "expected visible thread in sidebar, got: {entries:?}"
    );
    assert!(
        !entries.iter().any(|e| e.contains("Archived Thread")),
        "expected archived thread to be hidden from sidebar, got: {entries:?}"
    );

    cx.update(|_, cx| {
        let store = ThreadMetadataStore::global(cx);
        let all: Vec<_> = store.read(cx).entries().collect();
        assert_eq!(
            all.len(),
            2,
            "expected 2 total entries in the store, got: {}",
            all.len()
        );

        let archived: Vec<_> = store.read(cx).archived_entries().collect();
        assert_eq!(archived.len(), 1);
        assert_eq!(
            archived[0].session_id.as_ref().unwrap().0.as_ref(),
            "archived-thread"
        );
    });
}

#[gpui::test]
async fn test_archive_last_thread_on_linked_worktree_does_not_create_new_thread_on_worktree(
    cx: &mut TestAppContext,
) {
    // When a linked worktree has a single thread and that thread is archived,
    // the sidebar must NOT create a new thread on the same worktree (which
    // would prevent the worktree from being cleaned up on disk). Instead,
    // archive_thread switches to a sibling thread on the main workspace (or
    // creates a draft there) before archiving the metadata.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-ochre-drift"),
            ref_name: Some("refs/heads/ochre-drift".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project =
        project::Project::test(fs.clone(), ["/wt-ochre-drift".as_ref()], cx).await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    // Set up both workspaces with agent panels.
    let main_workspace =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    let _main_panel = add_agent_panel(&main_workspace, cx);
    let worktree_panel = add_agent_panel(&worktree_workspace, cx);

    // Activate the linked worktree workspace so the sidebar tracks it.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(worktree_workspace.clone(), None, window, cx);
    });

    // Open a thread in the linked worktree panel and send a message
    // so it becomes the active thread.
    let connection = StubAgentConnection::new();
    open_thread_with_connection(&worktree_panel, connection.clone(), cx);
    send_message(&worktree_panel, cx);

    let worktree_thread_id = active_session_id(&worktree_panel, cx);

    // Give the thread a response chunk so it has content.
    cx.update(|_, cx| {
        connection.send_update(
            worktree_thread_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("done".into())),
            cx,
        );
    });

    // Save the worktree thread's metadata.
    save_thread_metadata(
        worktree_thread_id.clone(),
        Some("Ochre Drift Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );

    // Also save a thread on the main project so there's a sibling in the
    // group that can be selected after archiving.
    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-project-thread")),
        Some("Main Project Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );

    cx.run_until_parked();

    // Verify the linked worktree thread appears with its chip.
    // The live thread title comes from the message text ("Hello"), not
    // the metadata title we saved.
    let entries_before = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries_before
            .iter()
            .any(|s| s.contains("{wt-ochre-drift}")),
        "expected worktree thread with chip before archiving, got: {entries_before:?}"
    );
    assert!(
        entries_before
            .iter()
            .any(|s| s.contains("Main Project Thread")),
        "expected main project thread before archiving, got: {entries_before:?}"
    );

    // Confirm the worktree thread is the active entry.
    sidebar.read_with(cx, |s, _| {
        assert_active_thread(
            s,
            &worktree_thread_id,
            "worktree thread should be active before archiving",
        );
    });

    // Archive the worktree thread — it's the only thread using ochre-drift.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&worktree_thread_id, window, cx);
    });

    cx.run_until_parked();

    // The archived thread should no longer appear in the sidebar.
    let entries_after = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries_after
            .iter()
            .any(|s| s.contains("Ochre Drift Thread")),
        "archived thread should be hidden, got: {entries_after:?}"
    );

    // No "+ New Thread" entry should appear with the ochre-drift worktree
    // chip — that would keep the worktree alive and prevent cleanup.
    assert!(
        !entries_after.iter().any(|s| s.contains("{wt-ochre-drift}")),
        "no entry should reference the archived worktree, got: {entries_after:?}"
    );

    // The main project thread should still be visible.
    assert!(
        entries_after
            .iter()
            .any(|s| s.contains("Main Project Thread")),
        "main project thread should still be visible, got: {entries_after:?}"
    );
}

#[gpui::test]
async fn test_archive_last_thread_on_linked_worktree_with_no_siblings_leaves_group_empty(
    cx: &mut TestAppContext,
) {
    // When a linked worktree thread is the ONLY thread in the project group
    // (no threads on the main repo either), archiving it should leave the
    // group empty with no active entry.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-ochre-drift"),
            ref_name: Some("refs/heads/ochre-drift".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project =
        project::Project::test(fs.clone(), ["/wt-ochre-drift".as_ref()], cx).await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    let main_workspace =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    let _main_panel = add_agent_panel(&main_workspace, cx);
    let worktree_panel = add_agent_panel(&worktree_workspace, cx);

    // Activate the linked worktree workspace.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(worktree_workspace.clone(), None, window, cx);
    });

    // Open a thread on the linked worktree — this is the ONLY thread.
    let connection = StubAgentConnection::new();
    open_thread_with_connection(&worktree_panel, connection.clone(), cx);
    send_message(&worktree_panel, cx);

    let worktree_thread_id = active_session_id(&worktree_panel, cx);

    cx.update(|_, cx| {
        connection.send_update(
            worktree_thread_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("done".into())),
            cx,
        );
    });

    save_thread_metadata(
        worktree_thread_id.clone(),
        Some("Ochre Drift Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );

    cx.run_until_parked();

    // Archive it — there are no other threads in the group.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&worktree_thread_id, window, cx);
    });

    cx.run_until_parked();

    let entries_after = visible_entries_as_strings(&sidebar, cx);

    // No entry should reference the linked worktree.
    assert!(
        !entries_after.iter().any(|s| s.contains("{wt-ochre-drift}")),
        "no entry should reference the archived worktree, got: {entries_after:?}"
    );

    // The active entry should be None — no draft is created.
    sidebar.read_with(cx, |s, _| {
        assert!(
            s.active_entry.is_none(),
            "expected no active entry after archiving the last thread, got: {:?}",
            s.active_entry,
        );
    });
}

#[gpui::test]
async fn test_unarchive_linked_worktree_thread_into_project_group_shows_only_restored_real_thread(
    cx: &mut TestAppContext,
) {
    // When an archived thread belongs to a linked worktree whose main repo is
    // already open, unarchiving should reopen the linked workspace into the
    // same project group and show only the restored real thread row.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    fs.insert_tree(
        "/wt-ochre-drift",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/ochre-drift",
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-ochre-drift"),
            ref_name: Some("refs/heads/ochre-drift".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project =
        project::Project::test(fs.clone(), ["/wt-ochre-drift".as_ref()], cx).await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);
    let main_workspace =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    let _main_panel = add_agent_panel(&main_workspace, cx);
    cx.run_until_parked();

    let session_id = acp::SessionId::new(Arc::from("linked-worktree-unarchive"));
    let original_thread_id = ThreadId::new();
    let main_paths = PathList::new(&[PathBuf::from("/project")]);
    let folder_paths = PathList::new(&[PathBuf::from("/wt-ochre-drift")]);

    cx.update(|_, cx| {
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id: original_thread_id,
                    session_id: Some(session_id.clone()),
                    agent_id: agent::MAV_AGENT_ID.clone(),
                    title: Some("Unarchived Linked Thread".into()),
                    title_override: None,
                    updated_at: Utc::now(),
                    created_at: None,
                    interacted_at: None,
                    worktree_paths: WorktreePaths::from_path_lists(
                        main_paths.clone(),
                        folder_paths.clone(),
                    )
                    .expect("main and folder paths should be well-formed"),
                    archived: true,
                    remote_connection: None,
                },
                cx,
            )
        });
    });
    cx.run_until_parked();

    let metadata = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(original_thread_id)
            .cloned()
            .expect("archived linked-worktree metadata should exist before restore")
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.open_thread_from_archive(metadata, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "expected unarchive to open the linked worktree workspace into the project group"
    );

    let session_entries = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entries()
            .filter(|entry| entry.session_id.as_ref() == Some(&session_id))
            .cloned()
            .collect::<Vec<_>>()
    });
    assert_eq!(
        session_entries.len(),
        1,
        "expected exactly one metadata row for restored linked worktree session, got: {session_entries:?}"
    );
    assert_eq!(
        session_entries[0].thread_id, original_thread_id,
        "expected unarchive to reuse the original linked worktree thread id"
    );
    assert!(
        !session_entries[0].archived,
        "expected restored linked worktree metadata to be unarchived, got: {:?}",
        session_entries[0]
    );

    let assert_no_extra_rows = |entries: &[String]| {
        let real_thread_rows = entries
            .iter()
            .filter(|entry| !entry.starts_with("v ") && !entry.starts_with("> "))
            .filter(|entry| !entry.contains("Draft"))
            .count();
        assert_eq!(
            real_thread_rows, 1,
            "expected exactly one visible real thread row after linked-worktree unarchive, got entries: {entries:?}"
        );
        assert!(
            !entries.iter().any(|entry| entry.contains("Draft")),
            "expected no draft rows after linked-worktree unarchive, got entries: {entries:?}"
        );
        assert!(
            !entries
                .iter()
                .any(|entry| entry.contains(DEFAULT_THREAD_TITLE)),
            "expected no default-titled real placeholder row after linked-worktree unarchive, got entries: {entries:?}"
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.contains("Unarchived Linked Thread")),
            "expected restored linked worktree thread row to be visible, got entries: {entries:?}"
        );
    };

    let entries_after_restore = visible_entries_as_strings(&sidebar, cx);
    assert_no_extra_rows(&entries_after_restore);

    // The reported bug may only appear after an extra scheduling turn.
    cx.run_until_parked();

    let entries_after_extra_turns = visible_entries_as_strings(&sidebar, cx);
    assert_no_extra_rows(&entries_after_extra_turns);
}

#[gpui::test]
async fn test_archive_thread_on_linked_worktree_selects_sibling_thread(cx: &mut TestAppContext) {
    // When a linked worktree thread is archived but the group has other
    // threads (e.g. on the main project), archive_thread should select
    // the nearest sibling.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-ochre-drift"),
            ref_name: Some("refs/heads/ochre-drift".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project =
        project::Project::test(fs.clone(), ["/wt-ochre-drift".as_ref()], cx).await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    let main_workspace =
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().next().unwrap().clone());
    let _main_panel = add_agent_panel(&main_workspace, cx);
    let worktree_panel = add_agent_panel(&worktree_workspace, cx);

    // Activate the linked worktree workspace.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(worktree_workspace.clone(), None, window, cx);
    });

    // Open a thread on the linked worktree.
    let connection = StubAgentConnection::new();
    open_thread_with_connection(&worktree_panel, connection.clone(), cx);
    send_message(&worktree_panel, cx);

    let worktree_thread_id = active_session_id(&worktree_panel, cx);

    cx.update(|_, cx| {
        connection.send_update(
            worktree_thread_id.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("done".into())),
            cx,
        );
    });

    save_thread_metadata(
        worktree_thread_id.clone(),
        Some("Ochre Drift Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &worktree_project,
        cx,
    );

    // Save a sibling thread on the main project.
    let main_thread_id = acp::SessionId::new(Arc::from("main-project-thread"));
    save_thread_metadata(
        main_thread_id,
        Some("Main Project Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );

    cx.run_until_parked();

    // Confirm the worktree thread is active.
    sidebar.read_with(cx, |s, _| {
        assert_active_thread(
            s,
            &worktree_thread_id,
            "worktree thread should be active before archiving",
        );
    });

    // Archive the worktree thread.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&worktree_thread_id, window, cx);
    });

    cx.run_until_parked();

    // The worktree workspace was removed and a draft was created on the
    // main workspace. No entry should reference the linked worktree.
    let entries_after = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries_after.iter().any(|s| s.contains("{wt-ochre-drift}")),
        "no entry should reference the archived worktree, got: {entries_after:?}"
    );

    // The main project thread should still be visible.
    assert!(
        entries_after
            .iter()
            .any(|s| s.contains("Main Project Thread")),
        "main project thread should still be visible, got: {entries_after:?}"
    );
}

#[gpui::test]
async fn test_linked_worktree_workspace_shows_main_worktree_threads(cx: &mut TestAppContext) {
    // When only a linked worktree workspace is open (not the main repo),
    // threads saved against the main repo should still appear in the sidebar.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    // Create the main repo with a linked worktree.
    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;

    fs.insert_tree(
        "/wt-feature-a",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        std::path::Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "abc".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    // Only open the linked worktree as a workspace — NOT the main repo.
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        MultiWorkspace::test_new(worktree_project.clone(), window, cx)
    });
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Save a thread against the MAIN repo path.
    save_named_thread_metadata("main-thread", "Main Repo Thread", &main_project, cx).await;

    // Save a thread against the linked worktree path.
    save_named_thread_metadata("wt-thread", "Worktree Thread", &worktree_project, cx).await;

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // Both threads should be visible: the worktree thread by direct lookup,
    // and the main repo thread because the workspace is a linked worktree
    // and we also query the main repo path.
    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries.iter().any(|e| e.contains("Main Repo Thread")),
        "expected main repo thread to be visible in linked worktree workspace, got: {entries:?}"
    );
    assert!(
        entries.iter().any(|e| e.contains("Worktree Thread")),
        "expected worktree thread to be visible, got: {entries:?}"
    );
}

async fn init_multi_project_test(
    paths: &[&str],
    cx: &mut TestAppContext,
) -> (Arc<FakeFs>, Entity<project::Project>) {
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });
    let fs = FakeFs::new(cx.executor());
    for path in paths {
        fs.insert_tree(path, serde_json::json!({ ".git": {}, "src": {} }))
            .await;
    }
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, [paths[0].as_ref()], cx).await;
    (fs, project)
}

async fn add_test_project(
    path: &str,
    fs: &Arc<FakeFs>,
    multi_workspace: &Entity<MultiWorkspace>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<Workspace> {
    let project = project::Project::test(fs.clone() as Arc<dyn fs::Fs>, [path.as_ref()], cx).await;
    let workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project, window, cx)
    });
    cx.run_until_parked();
    workspace
}

#[gpui::test]
async fn test_workspace_lifecycle_retains_projects_when_sidebar_is_closed(cx: &mut TestAppContext) {
    let (fs, project_a) =
        init_multi_project_test(&["/project-a", "/project-b", "/project-c"], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));
    let _sidebar = setup_sidebar_closed(&multi_workspace, cx);

    let workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    assert!(!multi_workspace.read_with(cx, |mw, _| mw.sidebar_open()));
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspace() == &workspace_a));

    let workspace_b = add_test_project("/project-b", &fs, &multi_workspace, cx).await;
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspace() == &workspace_b));
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspaces().any(|w| w == &workspace_a)));

    let workspace_c = add_test_project("/project-c", &fs, &multi_workspace, cx).await;
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        3
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspace() == &workspace_c));
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspaces().any(|w| w == &workspace_b)));
}

#[gpui::test]
async fn test_workspaces_remain_retained_after_sidebar_closes(cx: &mut TestAppContext) {
    let (fs, project_a) = init_multi_project_test(
        &["/project-a", "/project-b", "/project-c", "/project-d"],
        cx,
    )
    .await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));
    let _sidebar = setup_sidebar(&multi_workspace, cx);
    assert!(multi_workspace.read_with(cx, |mw, _| mw.sidebar_open()));
    let workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let workspace_b = add_test_project("/project-b", &fs, &multi_workspace, cx).await;
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2
    );

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a, None, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspaces().any(|w| w == &workspace_b)));

    multi_workspace.update_in(cx, |mw, window, cx| mw.close_sidebar(window, cx));
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2
    );

    let workspace_c = add_test_project("/project-c", &fs, &multi_workspace, cx).await;
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        3
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspace() == &workspace_c));

    let workspace_d = add_test_project("/project-d", &fs, &multi_workspace, cx).await;
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        4
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspace() == &workspace_d));
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspaces().any(|w| w == &workspace_c)));
}

#[gpui::test]
async fn test_sidebar_opening_keeps_existing_retained_workspaces(cx: &mut TestAppContext) {
    let (fs, project_a) =
        init_multi_project_test(&["/project-a", "/project-b", "/project-c"], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a, window, cx));
    setup_sidebar_closed(&multi_workspace, cx);

    let workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let workspace_b = add_test_project("/project-b", &fs, &multi_workspace, cx).await;
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspace() == &workspace_b));
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspaces().any(|w| w == &workspace_a)));

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.toggle_sidebar(window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspaces().any(|w| w == &workspace_b)));

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.toggle_sidebar(window, cx);
    });

    let workspace_c = add_test_project("/project-c", &fs, &multi_workspace, cx).await;
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        3
    );
    assert!(multi_workspace.read_with(cx, |mw, _| mw.workspace() == &workspace_c));
}

#[gpui::test]
async fn test_legacy_thread_with_canonical_path_opens_main_repo_workspace(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;

    fs.insert_tree(
        "/wt-feature-a",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {},
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "abc".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    // Only a linked worktree workspace is open — no workspace for /project.
    let worktree_project = project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        MultiWorkspace::test_new(worktree_project.clone(), window, cx)
    });
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Save a legacy thread: folder_paths = main repo, main_worktree_paths = empty.
    let legacy_session = acp::SessionId::new(Arc::from("legacy-main-thread"));
    cx.update(|_, cx| {
        let metadata = ThreadMetadata {
            thread_id: ThreadId::new(),
            session_id: Some(legacy_session.clone()),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title: Some("Legacy Main Thread".into()),
            title_override: None,
            updated_at: chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
            created_at: None,
            interacted_at: None,
            worktree_paths: WorktreePaths::from_folder_paths(&PathList::new(&[PathBuf::from(
                "/project",
            )])),
            archived: false,
            remote_connection: None,
        };
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // The legacy thread should appear in the sidebar under the project group.
    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        entries.iter().any(|e| e.contains("Legacy Main Thread")),
        "legacy thread should be visible: {entries:?}",
    );

    // Verify only 1 workspace before clicking.
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
    );

    // Focus and select the legacy thread, then confirm.
    focus_sidebar(&sidebar, cx);
    let thread_index = sidebar.read_with(cx, |sidebar, _| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|e| e.session_id().is_some_and(|id| id == &legacy_session))
            .expect("legacy thread should be in entries")
    });
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(thread_index);
    });
    cx.dispatch_action(Confirm);
    cx.run_until_parked();

    let new_workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let new_path_list =
        new_workspace.read_with(cx, |_, cx| workspace_path_list(&new_workspace, cx));
    assert_eq!(
        new_path_list,
        PathList::new(&[PathBuf::from("/project")]),
        "the new workspace should be for the main repo, not the linked worktree",
    );
}

#[gpui::test]
async fn test_linked_worktree_workspace_reachable_after_adding_unrelated_project(
    cx: &mut TestAppContext,
) {
    // Regression test for a property-test finding:
    //   AddLinkedWorktree { project_group_index: 0 }
    //   AddProject { use_worktree: true }
    //   AddProject { use_worktree: false }
    // After these three steps, the linked-worktree workspace was not
    // reachable from any sidebar entry.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);

        cx.observe_new(
            |workspace: &mut Workspace,
             window: Option<&mut Window>,
             cx: &mut gpui::Context<Workspace>| {
                if let Some(window) = window {
                    let panel = cx.new(|cx| AgentPanel::test_new(workspace, window, cx));
                    workspace.add_panel(panel, window, cx);
                }
            },
        )
        .detach();
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/my-project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, ["/my-project".as_ref()], cx).await;
    project.update(cx, |p, cx| p.git_scans_complete(cx)).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Step 1: Create a linked worktree for the main project.
    let worktree_name = "wt-0";
    let worktree_path = "/worktrees/wt-0";

    fs.insert_tree(
        worktree_path,
        serde_json::json!({
            ".git": "gitdir: /my-project/.git/worktrees/wt-0",
            "src": {},
        }),
    )
    .await;
    fs.insert_tree(
        "/my-project/.git/worktrees/wt-0",
        serde_json::json!({
            "commondir": "../../",
            "HEAD": "ref: refs/heads/wt-0",
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/my-project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from(worktree_path),
            ref_name: Some(format!("refs/heads/{}", worktree_name).into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    let main_workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let main_project = main_workspace.read_with(cx, |ws, _| ws.project().clone());
    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    // Step 2: Open the linked worktree as its own workspace.
    let worktree_project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, [worktree_path.as_ref()], cx).await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    let worktree_workspace = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });
    cx.run_until_parked();

    // Step 3: Add an unrelated project.
    fs.insert_tree(
        "/other-project",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    let other_project = project::Project::test(
        fs.clone() as Arc<dyn fs::Fs>,
        ["/other-project".as_ref()],
        cx,
    )
    .await;
    other_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(other_project.clone(), window, cx);
    });
    cx.run_until_parked();

    // Force a full sidebar rebuild with all groups expanded.
    sidebar.update_in(cx, |sidebar, _window, cx| {
        if let Some(mw) = sidebar.multi_workspace.upgrade() {
            mw.update(cx, |mw, _cx| mw.test_expand_all_groups());
        }
        sidebar.update_entries(cx);
    });
    cx.run_until_parked();

    // The linked-worktree workspace must be reachable from at least one
    // sidebar entry — otherwise the user has no way to navigate to it.
    let worktree_ws_id = worktree_workspace.entity_id();
    let (all_ids, reachable_ids) = sidebar.read_with(cx, |sidebar, cx| {
        let mw = multi_workspace.read(cx);

        let all: HashSet<gpui::EntityId> = mw.workspaces().map(|ws| ws.entity_id()).collect();
        let reachable: HashSet<gpui::EntityId> = sidebar
            .contents
            .entries
            .iter()
            .flat_map(|entry| entry.reachable_workspaces(mw, cx))
            .map(|ws| ws.entity_id())
            .collect();
        (all, reachable)
    });

    let unreachable = &all_ids - &reachable_ids;
    eprintln!("{}", visible_entries_as_strings(&sidebar, cx).join("\n"));

    assert!(
        unreachable.is_empty(),
        "workspaces not reachable from any sidebar entry: {:?}\n\
         (linked-worktree workspace id: {:?})",
        unreachable,
        worktree_ws_id,
    );
}

#[gpui::test]
async fn test_startup_failed_restoration_shows_no_draft(cx: &mut TestAppContext) {
    // Empty project groups no longer auto-create drafts via reconciliation.
    // A fresh startup with no restorable thread should show only the header.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, _panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let _workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert_eq!(
        entries,
        vec!["v [my-project]"],
        "empty group should show only the header, no auto-created draft"
    );
}

#[gpui::test]
async fn test_startup_successful_restoration_no_spurious_draft(cx: &mut TestAppContext) {
    // Rule 5: When the app starts and the AgentPanel successfully loads
    // a thread, no spurious draft should appear.
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Create and send a message to make a real thread.
    let connection = StubAgentConnection::new();
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel, connection, cx);
    send_message(&panel, cx);
    let session_id = active_session_id(&panel, cx);
    save_test_thread_metadata(&session_id, &project, cx).await;
    cx.run_until_parked();

    // Should show the thread, NOT a spurious draft.
    let entries = visible_entries_as_strings(&sidebar, cx);
    assert_eq!(entries, vec!["v [my-project]", "  Hello *"]);

    // active_entry should be Thread, not Draft.
    sidebar.read_with(cx, |sidebar, _| {
        assert_active_thread(sidebar, &session_id, "should be on the thread, not a draft");
    });
}

#[gpui::test]
async fn test_project_header_click_restores_last_viewed(cx: &mut TestAppContext) {
    // Rule 9: Clicking a project header should restore whatever the
    // user was last looking at in that group, not create new drafts
    // or jump to the first entry.
    let project_a = init_test_project_with_agent_panel("/project-a", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let (sidebar, panel_a) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    // Create two threads in project-a.
    let conn1 = StubAgentConnection::new();
    conn1.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel_a, conn1, cx);
    send_message(&panel_a, cx);
    let thread_a1 = active_session_id(&panel_a, cx);
    save_test_thread_metadata(&thread_a1, &project_a, cx).await;

    let conn2 = StubAgentConnection::new();
    conn2.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done".into()),
    )]);
    open_thread_with_connection(&panel_a, conn2, cx);
    send_message(&panel_a, cx);
    let thread_a2 = active_session_id(&panel_a, cx);
    save_test_thread_metadata(&thread_a2, &project_a, cx).await;
    cx.run_until_parked();

    // The user is now looking at thread_a2.
    sidebar.read_with(cx, |sidebar, _| {
        assert_active_thread(sidebar, &thread_a2, "should be on thread_a2");
    });

    // Add project-b and switch to it.
    let fs = cx.update(|_window, cx| <dyn fs::Fs>::global(cx));
    fs.as_fake()
        .insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    let project_b =
        project::Project::test(fs.clone() as Arc<dyn Fs>, ["/project-b".as_ref()], cx).await;
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    // Now switch BACK to project-a by activating its workspace.
    let workspace_a = multi_workspace.read_with(cx, |mw, cx| {
        mw.workspaces()
            .find(|ws| {
                ws.read(cx)
                    .project()
                    .read(cx)
                    .visible_worktrees(cx)
                    .any(|wt| {
                        wt.read(cx)
                            .abs_path()
                            .to_string_lossy()
                            .contains("project-a")
                    })
            })
            .unwrap()
            .clone()
    });
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();

    // The panel should still show thread_a2 (the last thing the user
    // was viewing in project-a), not a draft or thread_a1.
    sidebar.read_with(cx, |sidebar, _| {
        assert_active_thread(
            sidebar,
            &thread_a2,
            "switching back to project-a should restore thread_a2",
        );
    });

    // No spurious draft entries should have been created in
    // project-a's group (project-b may have a placeholder).
    let entries = visible_entries_as_strings(&sidebar, cx);
    // Find project-a's section and check it has no drafts.
    let project_a_start = entries
        .iter()
        .position(|e| e.contains("project-a"))
        .unwrap();
    let project_a_end = entries[project_a_start + 1..]
        .iter()
        .position(|e| e.starts_with("v "))
        .map(|i| i + project_a_start + 1)
        .unwrap_or(entries.len());
    let project_a_drafts = entries[project_a_start..project_a_end]
        .iter()
        .filter(|e| e.contains("Draft"))
        .count();
    assert_eq!(
        project_a_drafts, 0,
        "switching back to project-a should not create drafts in its group"
    );
}

#[gpui::test]
async fn test_activating_workspace_with_draft_does_not_create_extras(cx: &mut TestAppContext) {
    // When a workspace has a draft (from the panel's load fallback)
    // and the user activates it (e.g. by clicking the placeholder or
    // the project header), no extra drafts should be created.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let project_a =
        project::Project::test(fs.clone() as Arc<dyn Fs>, ["/project-a".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project_a.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    let workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let _panel_a = add_agent_panel(&workspace_a, cx);
    cx.run_until_parked();

    // Add project-b with its own workspace and agent panel.
    let project_b =
        project::Project::test(fs.clone() as Arc<dyn Fs>, ["/project-b".as_ref()], cx).await;
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });
    let _panel_b = add_agent_panel(&workspace_b, cx);
    cx.run_until_parked();

    // Explicitly create a draft on workspace_b so the sidebar tracks one.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.create_new_thread(&workspace_b, window, cx);
    });
    cx.run_until_parked();

    // Count project-b's drafts.
    let count_b_drafts = |cx: &mut gpui::VisualTestContext| {
        let entries = visible_entries_as_strings(&sidebar, cx);
        entries
            .iter()
            .skip_while(|e| !e.contains("project-b"))
            .take_while(|e| !e.starts_with("v ") || e.contains("project-b"))
            .filter(|e| e.contains("Draft"))
            .count()
    };
    let drafts_before = count_b_drafts(cx);

    // Switch away from project-b, then back.
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_b.clone(), None, window, cx);
    });
    cx.run_until_parked();

    let drafts_after = count_b_drafts(cx);
    assert_eq!(
        drafts_before, drafts_after,
        "activating workspace should not create extra drafts"
    );

    // The draft should be highlighted as active after switching back.
    sidebar.read_with(cx, |sidebar, _| {
        assert_active_draft(
            sidebar,
            &workspace_b,
            "draft should be active after switching back to its workspace",
        );
    });
}

#[gpui::test]
async fn test_non_archive_thread_paths_migrate_on_worktree_add_and_remove(cx: &mut TestAppContext) {
    // Historical threads (not open in any agent panel) should have their
    // worktree paths updated when a folder is added to or removed from the
    // project.
    let (_fs, project) = init_multi_project_test(&["/project-a", "/project-b"], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Save two threads directly into the metadata store (not via the agent
    // panel), so they are purely historical — no open views hold them.
    // Use different timestamps so sort order is deterministic.
    save_thread_metadata(
        acp::SessionId::new(Arc::from("hist-1")),
        Some("Historical 1".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    save_thread_metadata(
        acp::SessionId::new(Arc::from("hist-2")),
        Some("Historical 2".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 1).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    // Sanity-check: both threads exist under the initial key [/project-a].
    let old_key_paths = PathList::new(&[PathBuf::from("/project-a")]);
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store
                .entries_for_main_worktree_path(&old_key_paths, None)
                .count(),
            2,
            "should have 2 historical threads under old key before worktree add"
        );
    });

    // Add a second worktree to the project.
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    // The historical threads should now be indexed under the new combined
    // key [/project-a, /project-b].
    let new_key_paths = PathList::new(&[PathBuf::from("/project-a"), PathBuf::from("/project-b")]);
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store
                .entries_for_main_worktree_path(&old_key_paths, None)
                .count(),
            0,
            "should have 0 historical threads under old key after worktree add"
        );
        assert_eq!(
            store
                .entries_for_main_worktree_path(&new_key_paths, None)
                .count(),
            2,
            "should have 2 historical threads under new key after worktree add"
        );
    });

    // Sidebar should show threads under the new header.
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            "v [project-a, project-b]",
            "  Historical 2",
            "  Historical 1",
        ]
    );

    // Now remove the second worktree.
    let worktree_id = project.read_with(cx, |project, cx| {
        project
            .visible_worktrees(cx)
            .find(|wt| wt.read(cx).abs_path().as_ref() == Path::new("/project-b"))
            .map(|wt| wt.read(cx).id())
            .expect("should find project-b worktree")
    });
    project.update(cx, |project, cx| {
        project.remove_worktree(worktree_id, cx);
    });
    cx.run_until_parked();

    // Historical threads should migrate back to the original key.
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store
                .entries_for_main_worktree_path(&new_key_paths, None)
                .count(),
            0,
            "should have 0 historical threads under new key after worktree remove"
        );
        assert_eq!(
            store
                .entries_for_main_worktree_path(&old_key_paths, None)
                .count(),
            2,
            "should have 2 historical threads under old key after worktree remove"
        );
    });

    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec!["v [project-a]", "  Historical 2", "  Historical 1",]
    );
}

#[gpui::test]
async fn test_worktree_add_only_regroups_threads_for_changed_workspace(cx: &mut TestAppContext) {
    // When two workspaces share the same project group (same main path)
    // but have different folder paths (main repo vs linked worktree),
    // adding a worktree to the main workspace should regroup only that
    // workspace and its threads into the new project group. Threads for the
    // linked worktree workspace should remain under the original group.
    agent_ui::test_support::init_test(cx);
    cx.update(|cx| {
        cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
        ThreadStore::init_global(cx);
        ThreadMetadataStore::init_global(cx);
        language_model::LanguageModelRegistry::test(cx);
        prompt_store::init(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ ".git": {}, "src": {} }))
        .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-feature"),
            ref_name: Some("refs/heads/feature".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    // Workspace A: main repo at /project.
    let main_project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, ["/project".as_ref()], cx).await;
    // Workspace B: linked worktree of the same repo (same group, different folder).
    let worktree_project =
        project::Project::test(fs.clone() as Arc<dyn fs::Fs>, ["/wt-feature".as_ref()], cx).await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx);
    });
    cx.run_until_parked();

    // Save a thread for each workspace's folder paths.
    let time_main = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 1).unwrap();
    let time_wt = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 2).unwrap();
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-main")),
        Some("Main Thread".into()),
        time_main,
        Some(time_main),
        None,
        &main_project,
        cx,
    );
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-wt")),
        Some("Worktree Thread".into()),
        time_wt,
        Some(time_wt),
        None,
        &worktree_project,
        cx,
    );
    cx.run_until_parked();

    let folder_paths_main = PathList::new(&[PathBuf::from("/project")]);
    let folder_paths_wt = PathList::new(&[PathBuf::from("/wt-feature")]);

    // Sanity-check: each thread is indexed under its own folder paths, but
    // both appear under the shared sidebar group keyed by the main worktree.
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store.entries_for_path(&folder_paths_main, None).count(),
            1,
            "one thread under [/project]"
        );
        assert_eq!(
            store.entries_for_path(&folder_paths_wt, None).count(),
            1,
            "one thread under [/wt-feature]"
        );
    });
    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            "v [project]",
            "  Worktree Thread {wt-feature}",
            "  Main Thread",
        ]
    );

    // Add /project-b to the main project only.
    main_project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    // Main Thread (folder paths [/project]) should be regrouped to
    // [/project, /project-b]. Worktree Thread should remain under the
    // original [/project] group.
    let folder_paths_main_b =
        PathList::new(&[PathBuf::from("/project"), PathBuf::from("/project-b")]);
    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx).read(cx);
        assert_eq!(
            store.entries_for_path(&folder_paths_main, None).count(),
            0,
            "main thread should no longer be under old folder paths [/project]"
        );
        assert_eq!(
            store.entries_for_path(&folder_paths_main_b, None).count(),
            1,
            "main thread should now be under [/project, /project-b]"
        );
        assert_eq!(
            store.entries_for_path(&folder_paths_wt, None).count(),
            1,
            "worktree thread should remain unchanged under [/wt-feature]"
        );
    });

    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&sidebar, cx),
        vec![
            "v [project]",
            "  Worktree Thread {wt-feature}",
            "v [project, project-b]",
            "  Main Thread",
        ]
    );
}

#[gpui::test]
async fn test_linked_worktree_workspace_reachable_after_adding_worktree_to_project(
    cx: &mut TestAppContext,
) {
    // When a linked worktree is opened as its own workspace and then a new
    // folder is added to the main project group, the linked worktree
    // workspace must still be reachable from some sidebar entry.
    let (_fs, project) = init_multi_project_test(&["/my-project"], cx).await;
    let fs = _fs.clone();

    // Set up git worktree infrastructure.
    fs.insert_tree(
        "/my-project/.git/worktrees/wt-0",
        serde_json::json!({
            "commondir": "../../",
            "HEAD": "ref: refs/heads/wt-0",
        }),
    )
    .await;
    fs.insert_tree(
        "/worktrees/wt-0",
        serde_json::json!({
            ".git": "gitdir: /my-project/.git/worktrees/wt-0",
            "src": {},
        }),
    )
    .await;
    fs.add_linked_worktree_for_repo(
        Path::new("/my-project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/wt-0"),
            ref_name: Some("refs/heads/wt-0".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    // Re-scan so the main project discovers the linked worktree.
    project.update(cx, |p, cx| p.git_scans_complete(cx)).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Open the linked worktree as its own workspace.
    let worktree_project = project::Project::test(
        fs.clone() as Arc<dyn fs::Fs>,
        ["/worktrees/wt-0".as_ref()],
        cx,
    )
    .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx);
    });
    cx.run_until_parked();

    // Both workspaces should be reachable.
    let workspace_count = multi_workspace.read_with(cx, |mw, _| mw.workspaces().count());
    assert_eq!(workspace_count, 2, "should have 2 workspaces");

    // Add a new folder to the main project, changing the project group key.
    fs.insert_tree(
        "/other-project",
        serde_json::json!({ ".git": {}, "src": {} }),
    )
    .await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/other-project", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    sidebar.update_in(cx, |sidebar, _window, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    // The linked worktree workspace must still be reachable.
    let entries = visible_entries_as_strings(&sidebar, cx);
    let mw_workspaces: Vec<_> = multi_workspace.read_with(cx, |mw, _| {
        mw.workspaces().map(|ws| ws.entity_id()).collect()
    });
    sidebar.read_with(cx, |sidebar, cx| {
        let multi_workspace = multi_workspace.read(cx);
        let reachable: std::collections::HashSet<gpui::EntityId> = sidebar
            .contents
            .entries
            .iter()
            .flat_map(|entry| entry.reachable_workspaces(multi_workspace, cx))
            .map(|ws| ws.entity_id())
            .collect();
        let all: std::collections::HashSet<gpui::EntityId> =
            mw_workspaces.iter().copied().collect();
        let unreachable = &all - &reachable;
        assert!(
            unreachable.is_empty(),
            "all workspaces should be reachable after adding folder; \
             unreachable: {:?}, entries: {:?}",
            unreachable,
            entries,
        );
    });
}

mod property_test {
    use super::*;
    use gpui::proptest::prelude::*;

    struct UnopenedWorktree {
        path: String,
        main_workspace_path: String,
    }

    struct TestState {
        fs: Arc<FakeFs>,
        thread_counter: u32,
        workspace_counter: u32,
        worktree_counter: u32,
        saved_thread_ids: Vec<acp::SessionId>,
        unopened_worktrees: Vec<UnopenedWorktree>,
    }

    impl TestState {
        fn new(fs: Arc<FakeFs>) -> Self {
            Self {
                fs,
                thread_counter: 0,
                workspace_counter: 1,
                worktree_counter: 0,
                saved_thread_ids: Vec::new(),
                unopened_worktrees: Vec::new(),
            }
        }

        fn next_metadata_only_thread_id(&mut self) -> acp::SessionId {
            let id = self.thread_counter;
            self.thread_counter += 1;
            acp::SessionId::new(Arc::from(format!("prop-thread-{id}")))
        }

        fn next_workspace_path(&mut self) -> String {
            let id = self.workspace_counter;
            self.workspace_counter += 1;
            format!("/prop-project-{id}")
        }

        fn next_worktree_name(&mut self) -> String {
            let id = self.worktree_counter;
            self.worktree_counter += 1;
            format!("wt-{id}")
        }
    }

    #[derive(Debug)]
    enum Operation {
        SaveThread { project_group_index: usize },
        SaveWorktreeThread { worktree_index: usize },
        ToggleAgentPanel,
        CreateDraftThread,
        AddProject { use_worktree: bool },
        ArchiveThread { index: usize },
        SwitchToThread { index: usize },
        SwitchToProjectGroup { index: usize },
        AddLinkedWorktree { project_group_index: usize },
        AddWorktreeToProject { project_group_index: usize },
        RemoveWorktreeFromProject { project_group_index: usize },
    }

    // Distribution (out of 24 slots):
    //   SaveThread:                5 slots (~21%)
    //   SaveWorktreeThread:        2 slots (~8%)
    //   ToggleAgentPanel:          1 slot  (~4%)
    //   CreateDraftThread:         1 slot  (~4%)
    //   AddProject:                1 slot  (~4%)
    //   ArchiveThread:             2 slots (~8%)
    //   SwitchToThread:            2 slots (~8%)
    //   SwitchToProjectGroup:      2 slots (~8%)
    //   AddLinkedWorktree:         4 slots (~17%)
    //   AddWorktreeToProject:      2 slots (~8%)
    //   RemoveWorktreeFromProject: 2 slots (~8%)
    const DISTRIBUTION_SLOTS: u32 = 24;

    impl TestState {
        fn generate_operation(&self, raw: u32, project_group_count: usize) -> Operation {
            let extra = (raw / DISTRIBUTION_SLOTS) as usize;

            match raw % DISTRIBUTION_SLOTS {
                0..=4 => Operation::SaveThread {
                    project_group_index: extra % project_group_count,
                },
                5..=6 if !self.unopened_worktrees.is_empty() => Operation::SaveWorktreeThread {
                    worktree_index: extra % self.unopened_worktrees.len(),
                },
                5..=6 => Operation::SaveThread {
                    project_group_index: extra % project_group_count,
                },
                7 => Operation::ToggleAgentPanel,
                8 => Operation::CreateDraftThread,
                9 => Operation::AddProject {
                    use_worktree: !self.unopened_worktrees.is_empty(),
                },
                10..=11 if !self.saved_thread_ids.is_empty() => Operation::ArchiveThread {
                    index: extra % self.saved_thread_ids.len(),
                },
                10..=11 => Operation::AddProject {
                    use_worktree: !self.unopened_worktrees.is_empty(),
                },
                12..=13 if !self.saved_thread_ids.is_empty() => Operation::SwitchToThread {
                    index: extra % self.saved_thread_ids.len(),
                },
                12..=13 => Operation::SwitchToProjectGroup {
                    index: extra % project_group_count,
                },
                14..=15 => Operation::SwitchToProjectGroup {
                    index: extra % project_group_count,
                },
                16..=19 if project_group_count > 0 => Operation::AddLinkedWorktree {
                    project_group_index: extra % project_group_count,
                },
                16..=19 => Operation::SaveThread {
                    project_group_index: extra % project_group_count,
                },
                20..=21 if project_group_count > 0 => Operation::AddWorktreeToProject {
                    project_group_index: extra % project_group_count,
                },
                20..=21 => Operation::SaveThread {
                    project_group_index: extra % project_group_count,
                },
                22..=23 if project_group_count > 0 => Operation::RemoveWorktreeFromProject {
                    project_group_index: extra % project_group_count,
                },
                22..=23 => Operation::SaveThread {
                    project_group_index: extra % project_group_count,
                },
                _ => unreachable!(),
            }
        }
    }

    fn save_thread_to_path_with_main(
        state: &mut TestState,
        path_list: PathList,
        main_worktree_paths: PathList,
        cx: &mut gpui::VisualTestContext,
    ) {
        let session_id = state.next_metadata_only_thread_id();
        let title: SharedString = format!("Thread {}", session_id).into();
        let updated_at = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0)
            .unwrap()
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

    async fn perform_operation(
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

                let panel =
                    workspace.read_with(cx, |workspace, cx| workspace.panel::<AgentPanel>(cx));
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
                        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0)
                            .unwrap()
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
                        ThreadMetadataStore::global(cx)
                            .update(cx, |store, cx| store.save(metadata, cx))
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
                let panel =
                    workspace.read_with(cx, |workspace, cx| workspace.panel::<AgentPanel>(cx));
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
                let project = project::Project::test(
                    state.fs.clone() as Arc<dyn fs::Fs>,
                    [path.as_ref()],
                    cx,
                )
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

                state.fs
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

    fn update_sidebar(sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext) {
        sidebar.update_in(cx, |sidebar, _window, cx| {
            if let Some(mw) = sidebar.multi_workspace.upgrade() {
                mw.update(cx, |mw, _cx| mw.test_expand_all_groups());
            }
            sidebar.update_entries(cx);
        });
    }

    fn validate_sidebar_properties(sidebar: &Sidebar, cx: &App) -> anyhow::Result<()> {
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

    #[gpui::property_test(config = ProptestConfig {
        cases: 20,
        ..Default::default()
    })]
    async fn test_sidebar_invariants(
        #[strategy = gpui::proptest::collection::vec(0u32..DISTRIBUTION_SLOTS * 10, 1..10)]
        raw_operations: Vec<u32>,
        cx: &mut TestAppContext,
    ) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT_PROPTEST_DB: AtomicUsize = AtomicUsize::new(0);

        let test_db_id = NEXT_PROPTEST_DB.fetch_add(1, Ordering::SeqCst);
        cx.update(|cx| {
            cx.set_global(TestTerminalMetadataDbName(format!(
                "PROPTEST_TERMINAL_THREAD_METADATA_{test_db_id}"
            )));
        });

        agent_ui::test_support::init_test(cx);
        cx.update(|cx| {
            cx.set_global(db::AppDatabase::test_new());
            cx.set_global(agent_ui::MaxIdleRetainedThreads(1));
            cx.set_global(agent_ui::thread_metadata_store::TestMetadataDbName(
                format!("PROPTEST_THREAD_METADATA_{test_db_id}"),
            ));

            ThreadStore::init_global(cx);
            ThreadMetadataStore::init_global(cx);
            language_model::LanguageModelRegistry::test(cx);
            prompt_store::init(cx);

            // Auto-add an AgentPanel to every workspace so that implicitly
            // created workspaces (e.g. from thread activation) also have one.
            cx.observe_new(
                |workspace: &mut Workspace,
                 window: Option<&mut Window>,
                 cx: &mut gpui::Context<Workspace>| {
                    if let Some(window) = window {
                        let panel = cx.new(|cx| AgentPanel::test_new(workspace, window, cx));
                        workspace.add_panel(panel, window, cx);
                    }
                },
            )
            .detach();
        });

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            "/my-project",
            serde_json::json!({
                ".git": {},
                "src": {},
            }),
        )
        .await;
        cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
        let project =
            project::Project::test(fs.clone() as Arc<dyn fs::Fs>, ["/my-project".as_ref()], cx)
                .await;
        project.update(cx, |p, cx| p.git_scans_complete(cx)).await;

        let (multi_workspace, cx) =
            cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let sidebar = setup_sidebar(&multi_workspace, cx);

        let mut state = TestState::new(fs);
        let mut executed: Vec<String> = Vec::new();

        for &raw_op in &raw_operations {
            let project_group_count =
                multi_workspace.read_with(cx, |mw, _| mw.project_group_keys().len());
            let operation = state.generate_operation(raw_op, project_group_count);
            executed.push(format!("{:?}", operation));
            perform_operation(operation, &mut state, &multi_workspace, &sidebar, cx).await;
            cx.run_until_parked();

            update_sidebar(&sidebar, cx);
            cx.run_until_parked();

            let result =
                sidebar.read_with(cx, |sidebar, cx| validate_sidebar_properties(sidebar, cx));
            if let Err(err) = result {
                let log = executed.join("\n  ");
                panic!(
                    "Property violation after step {}:\n{err}\n\nOperations:\n  {log}",
                    executed.len(),
                );
            }
        }
    }
}

#[gpui::test]
async fn test_remote_project_integration_does_not_briefly_render_as_separate_project(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    init_test(cx);

    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        workspace::init(app_state.clone(), cx);
        app_state
    });

    // Set up the remote server side.
    let server_fs = FakeFs::new(server_cx.executor());
    server_fs
        .insert_tree(
            "/project",
            serde_json::json!({
                ".git": {},
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;
    server_fs.set_branch_name(Path::new("/project/.git"), Some("main"));

    // Create the linked worktree checkout path on the remote server,
    // but do not yet register it as a git-linked worktree. The real
    // regrouping update in this test should happen only after the
    // sidebar opens the closed remote thread.
    server_fs
        .insert_tree(
            "/project-wt-1",
            serde_json::json!({
                "src": { "main.rs": "fn main() {}" }
            }),
        )
        .await;

    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let (original_opts, server_session, _) = remote::RemoteClient::fake_server(cx, server_cx);

    server_cx.update(remote_server::HeadlessProject::init);
    let server_executor = server_cx.executor();
    let _headless = server_cx.new(|cx| {
        remote_server::HeadlessProject::new(
            remote_server::HeadlessAppState {
                session: server_session,
                fs: server_fs.clone(),
                http_client: Arc::new(http_client::BlockedHttpClient),
                node_runtime: node_runtime::NodeRuntime::unavailable(),
                languages: Arc::new(language::LanguageRegistry::new(server_executor.clone())),
                extension_host_proxy: Arc::new(extension::ExtensionHostProxy::new()),
                startup_time: std::time::Instant::now(),
            },
            false,
            cx,
        )
    });

    // Connect the client side and build a remote project.
    let remote_client = remote::RemoteClient::connect_mock(original_opts.clone(), cx).await;
    let project = cx.update(|cx| {
        let project_client = client::Client::new(
            Arc::new(clock::FakeSystemClock::new()),
            http_client::FakeHttpClient::with_404_response(),
            cx,
        );
        let user_store = cx.new(|cx| client::UserStore::new(project_client.clone(), cx));
        project::Project::remote(
            remote_client,
            project_client,
            node_runtime::NodeRuntime::unavailable(),
            user_store,
            app_state.languages.clone(),
            app_state.fs.clone(),
            false,
            cx,
        )
    });

    // Open the remote worktree.
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(Path::new("/project"), true, cx)
        })
        .await
        .expect("should open remote worktree");
    cx.run_until_parked();

    // Verify the project is remote.
    project.read_with(cx, |project, cx| {
        assert!(!project.is_local(), "project should be remote");
        assert!(
            project.remote_connection_options(cx).is_some(),
            "project should have remote connection options"
        );
    });

    cx.update(|cx| <dyn fs::Fs>::set_global(app_state.fs.clone(), cx));

    // Create MultiWorkspace with the remote project.
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    cx.run_until_parked();

    // Save a thread for the main remote workspace (folder_paths match
    // the open workspace, so it will be classified as Open).
    let main_thread_id = acp::SessionId::new(Arc::from("main-thread"));
    save_thread_metadata(
        main_thread_id.clone(),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();

    // Save a thread whose folder_paths point to a linked worktree path
    // that doesn't have an open workspace ("/project-wt-1"), but whose
    // main_worktree_paths match the project group key so it appears
    // in the sidebar under the same remote group. This simulates a
    // linked worktree workspace that was closed.
    let remote_thread_id = acp::SessionId::new(Arc::from("remote-thread"));
    let (main_worktree_paths, remote_connection) = project.read_with(cx, |p, cx| {
        (
            p.project_group_key(cx).path_list().clone(),
            p.remote_connection_options(cx),
        )
    });
    cx.update(|_window, cx| {
        let metadata = ThreadMetadata {
            thread_id: ThreadId::new(),
            session_id: Some(remote_thread_id.clone()),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title: Some("Worktree Thread".into()),
            title_override: None,
            updated_at: chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 1).unwrap(),
            created_at: None,
            interacted_at: None,
            worktree_paths: WorktreePaths::from_path_lists(
                main_worktree_paths,
                PathList::new(&[PathBuf::from("/project-wt-1")]),
            )
            .unwrap(),
            archived: false,
            remote_connection,
        };
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = sidebar.contents.entries.iter().position(|entry| {
            matches!(
                entry,
                ListEntry::Thread(thread) if thread.metadata.session_id.as_ref() == Some(&remote_thread_id)
            )
        });
    });

    let saw_separate_project_header = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let saw_separate_project_header_for_observer = saw_separate_project_header.clone();

    sidebar
        .update(cx, |_, cx| {
            cx.observe_self(move |sidebar, _cx| {
                let mut project_headers = sidebar.contents.entries.iter().filter_map(|entry| {
                    if let ListEntry::ProjectHeader { label, .. } = entry {
                        Some(label.as_ref())
                    } else {
                        None
                    }
                });

                let Some(project_header) = project_headers.next() else {
                    saw_separate_project_header_for_observer
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    return;
                };

                if project_header != "project" || project_headers.next().is_some() {
                    saw_separate_project_header_for_observer
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                }
            })
        })
        .detach();

    multi_workspace.update(cx, |multi_workspace, cx| {
        let workspace = multi_workspace.workspace().clone();
        workspace.update(cx, |workspace: &mut Workspace, cx| {
            let remote_client = workspace
                .project()
                .read(cx)
                .remote_client()
                .expect("main remote project should have a remote client");
            remote_client.update(cx, |remote_client: &mut remote::RemoteClient, cx| {
                remote_client.force_server_not_running(cx);
            });
        });
    });
    cx.run_until_parked();

    let (server_session_2, connect_guard_2) =
        remote::RemoteClient::fake_server_with_opts(&original_opts, cx, server_cx);
    let _headless_2 = server_cx.new(|cx| {
        remote_server::HeadlessProject::new(
            remote_server::HeadlessAppState {
                session: server_session_2,
                fs: server_fs.clone(),
                http_client: Arc::new(http_client::BlockedHttpClient),
                node_runtime: node_runtime::NodeRuntime::unavailable(),
                languages: Arc::new(language::LanguageRegistry::new(server_executor.clone())),
                extension_host_proxy: Arc::new(extension::ExtensionHostProxy::new()),
                startup_time: std::time::Instant::now(),
            },
            false,
            cx,
        )
    });
    drop(connect_guard_2);

    let window = cx.windows()[0];
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Confirm.boxed_clone(), cx);
    })
    .unwrap();

    cx.run_until_parked();

    let new_workspace = multi_workspace.read_with(cx, |mw, _| {
        assert_eq!(
            mw.workspaces().count(),
            2,
            "confirming a closed remote thread should open a second workspace"
        );
        mw.workspaces()
            .find(|workspace| workspace.entity_id() != mw.workspace().entity_id())
            .unwrap()
            .clone()
    });

    server_fs
        .add_linked_worktree_for_repo(
            Path::new("/project/.git"),
            true,
            git::repository::Worktree {
                path: PathBuf::from("/project-wt-1"),
                ref_name: Some("refs/heads/feature-wt".into()),
                sha: "abc123".into(),
                is_main: false,
                is_bare: false,
            },
        )
        .await;

    server_cx.run_until_parked();
    cx.run_until_parked();
    server_cx.run_until_parked();
    cx.run_until_parked();

    let entries_after_update = visible_entries_as_strings(&sidebar, cx);
    let group_after_update = new_workspace.read_with(cx, |workspace, cx| {
        workspace.project().read(cx).project_group_key(cx)
    });

    assert_eq!(
        group_after_update,
        project.read_with(cx, |project, cx| ProjectGroupKey::from_project(project, cx)),
        "expected the remote worktree workspace to be grouped under the main remote project after the real update; \
         final sidebar entries: {:?}",
        entries_after_update,
    );

    sidebar.update(cx, |sidebar, _cx| {
        assert_remote_project_integration_sidebar_state(
            sidebar,
            &main_thread_id,
            &remote_thread_id,
        );
    });

    assert!(
        !saw_separate_project_header.load(std::sync::atomic::Ordering::SeqCst),
        "sidebar briefly rendered the remote worktree as a separate project during the real remote open/update sequence; \
         final group: {:?}; final sidebar entries: {:?}",
        group_after_update,
        entries_after_update,
    );
}

#[gpui::test]
async fn test_archive_removes_worktree_even_when_workspace_paths_diverge(cx: &mut TestAppContext) {
    // When the thread's folder_paths don't exactly match any workspace's
    // root paths (e.g. because a folder was added to the workspace after
    // the thread was created), workspace_to_remove is None. But the linked
    // worktree workspace still needs to be removed so that its worktree
    // entities are released, allowing git worktree removal to proceed.
    //
    // With the fix, archive_thread scans roots_to_archive for any linked
    // worktree workspaces and includes them in the removal set, even when
    // the thread's folder_paths don't match the workspace's root paths.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-a": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-a",
                    },
                },
            },
            "src": {},
        }),
    )
    .await;

    fs.insert_tree(
        "/worktrees/project/feature-a/project",
        serde_json::json!({
            ".git": "gitdir: /project/.git/worktrees/feature-a",
            "src": {
                "main.rs": "fn main() {}",
            },
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/project/feature-a/project"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "abc".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/project/feature-a/project"),
        None,
        cx,
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project = project::Project::test(fs.clone(), ["/project".as_ref()], cx).await;
    let worktree_project = project::Project::test(
        fs.clone(),
        ["/worktrees/project/feature-a/project".as_ref()],
        cx,
    )
    .await;

    main_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(main_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project.clone(), window, cx)
    });

    // Save thread metadata using folder_paths that DON'T match the
    // workspace's root paths. This simulates the case where the workspace's
    // paths diverged (e.g. a folder was added after thread creation).
    // This causes workspace_to_remove to be None because
    // workspace_for_paths can't find a workspace with these exact paths.
    let wt_thread_id = acp::SessionId::new(Arc::from("worktree-thread"));
    save_thread_metadata_with_main_paths(
        "worktree-thread",
        "Worktree Thread",
        PathList::new(&[
            PathBuf::from("/worktrees/project/feature-a/project"),
            PathBuf::from("/nonexistent"),
        ]),
        PathList::new(&[PathBuf::from("/project"), PathBuf::from("/nonexistent")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        cx,
    );

    // Also save a main thread so the sidebar has something to show.
    save_thread_metadata(
        acp::SessionId::new(Arc::from("main-thread")),
        Some("Main Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        None,
        None,
        &main_project,
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        2,
        "should start with 2 workspaces (main + linked worktree)"
    );

    // Archive the worktree thread.
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&wt_thread_id, window, cx);
    });

    cx.run_until_parked();

    // The linked worktree workspace should have been removed, even though
    // workspace_to_remove was None (paths didn't match).
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "linked worktree workspace should be removed after archiving, \
         even when folder_paths don't match workspace root paths"
    );

    // The thread should still be archived (not unarchived due to an error).
    let still_archived = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&wt_thread_id)
            .map(|t| t.archived)
    });
    assert_eq!(
        still_archived,
        Some(true),
        "thread should still be archived (not rolled back due to error)"
    );

    // The linked worktree directory should be removed from disk.
    assert!(
        !fs.is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from disk"
    );
}

#[gpui::test]
async fn test_archive_mixed_workspace_closes_only_archived_worktree_items(cx: &mut TestAppContext) {
    // When a workspace contains both a worktree being archived and other
    // worktrees that should remain, only the editor items referencing the
    // archived worktree should be closed — the workspace itself must be
    // preserved.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/main-repo",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-b": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-b",
                    },
                },
            },
            "src": {
                "lib.rs": "pub fn hello() {}",
            },
        }),
    )
    .await;

    fs.insert_tree(
        "/worktrees/main-repo/feature-b/main-repo",
        serde_json::json!({
            ".git": "gitdir: /main-repo/.git/worktrees/feature-b",
            "src": {
                "main.rs": "fn main() { hello(); }",
            },
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/main-repo/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/main-repo/feature-b/main-repo"),
            ref_name: Some("refs/heads/feature-b".into()),
            sha: "def".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/main-repo/feature-b/main-repo"),
        None,
        cx,
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    // Create a single project that contains BOTH the main repo and the
    // linked worktree — this makes it a "mixed" workspace.
    let mixed_project = project::Project::test(
        fs.clone(),
        [
            "/main-repo".as_ref(),
            "/worktrees/main-repo/feature-b/main-repo".as_ref(),
        ],
        cx,
    )
    .await;

    mixed_project
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) = cx
        .add_window_view(|window, cx| MultiWorkspace::test_new(mixed_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Open editor items in both worktrees so we can verify which ones
    // get closed.
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let worktree_ids: Vec<(WorktreeId, Arc<Path>)> = workspace.read_with(cx, |ws, cx| {
        ws.project()
            .read(cx)
            .visible_worktrees(cx)
            .map(|wt| (wt.read(cx).id(), wt.read(cx).abs_path()))
            .collect()
    });

    let main_repo_wt_id = worktree_ids
        .iter()
        .find(|(_, path)| path.as_ref() == Path::new("/main-repo"))
        .map(|(id, _)| *id)
        .expect("should find main-repo worktree");

    let feature_b_wt_id = worktree_ids
        .iter()
        .find(|(_, path)| path.as_ref() == Path::new("/worktrees/main-repo/feature-b/main-repo"))
        .map(|(id, _)| *id)
        .expect("should find feature-b worktree");

    // Open files from both worktrees.
    let main_repo_path = project::ProjectPath {
        worktree_id: main_repo_wt_id,
        path: Arc::from(rel_path("src/lib.rs")),
    };
    let feature_b_path = project::ProjectPath {
        worktree_id: feature_b_wt_id,
        path: Arc::from(rel_path("src/main.rs")),
    };

    workspace
        .update_in(cx, |ws, window, cx| {
            ws.open_path(main_repo_path.clone(), None, true, window, cx)
        })
        .await
        .expect("should open main-repo file");
    workspace
        .update_in(cx, |ws, window, cx| {
            ws.open_path(feature_b_path.clone(), None, true, window, cx)
        })
        .await
        .expect("should open feature-b file");

    cx.run_until_parked();

    // Verify both items are open.
    let open_paths_before: Vec<project::ProjectPath> = workspace.read_with(cx, |ws, cx| {
        ws.panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx)
                    .items()
                    .filter_map(|item| item.project_path(cx))
            })
            .collect()
    });
    assert!(
        open_paths_before
            .iter()
            .any(|pp| pp.worktree_id == main_repo_wt_id),
        "main-repo file should be open"
    );
    assert!(
        open_paths_before
            .iter()
            .any(|pp| pp.worktree_id == feature_b_wt_id),
        "feature-b file should be open"
    );

    // Save thread metadata for the linked worktree with deliberately
    // mismatched folder_paths to trigger the scan-based detection.
    save_thread_metadata_with_main_paths(
        "feature-b-thread",
        "Feature B Thread",
        PathList::new(&[
            PathBuf::from("/worktrees/main-repo/feature-b/main-repo"),
            PathBuf::from("/nonexistent"),
        ]),
        PathList::new(&[PathBuf::from("/main-repo"), PathBuf::from("/nonexistent")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        cx,
    );

    // Save another thread that references only the main repo (not the
    // linked worktree) so archiving the feature-b thread's worktree isn't
    // blocked by another unarchived thread referencing the same path.
    save_thread_metadata_with_main_paths(
        "other-thread",
        "Other Thread",
        PathList::new(&[PathBuf::from("/main-repo")]),
        PathList::new(&[PathBuf::from("/main-repo")]),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        cx,
    );
    cx.run_until_parked();

    multi_workspace.update_in(cx, |_, _window, cx| cx.notify());
    cx.run_until_parked();

    // There should still be exactly 1 workspace.
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "should have 1 workspace (the mixed workspace)"
    );

    // Archive the feature-b thread.
    let fb_session_id = acp::SessionId::new(Arc::from("feature-b-thread"));
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&fb_session_id, window, cx);
    });

    cx.run_until_parked();

    // The workspace should still exist (it's "mixed" — has non-archived worktrees).
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspaces().count()),
        1,
        "mixed workspace should be preserved"
    );

    // Only the feature-b editor item should have been closed.
    let open_paths_after: Vec<project::ProjectPath> = workspace.read_with(cx, |ws, cx| {
        ws.panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx)
                    .items()
                    .filter_map(|item| item.project_path(cx))
            })
            .collect()
    });
    assert!(
        open_paths_after
            .iter()
            .any(|pp| pp.worktree_id == main_repo_wt_id),
        "main-repo file should still be open"
    );
    assert!(
        !open_paths_after
            .iter()
            .any(|pp| pp.worktree_id == feature_b_wt_id),
        "feature-b file should have been closed"
    );
}

#[gpui::test]
async fn test_discard_mixed_workspace_draft_closes_only_archived_worktree_items(
    cx: &mut TestAppContext,
) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/main-repo",
        serde_json::json!({
            ".git": {
                "worktrees": {
                    "feature-b": {
                        "commondir": "../../",
                        "HEAD": "ref: refs/heads/feature-b",
                    },
                },
            },
            "src": {
                "lib.rs": "pub fn hello() {}",
            },
        }),
    )
    .await;

    fs.insert_tree(
        "/worktrees/main-repo/feature-b/main-repo",
        serde_json::json!({
            ".git": "gitdir: /main-repo/.git/worktrees/feature-b",
            "src": {
                "main.rs": "fn main() { hello(); }",
            },
        }),
    )
    .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/main-repo/.git"),
        false,
        git::repository::Worktree {
            path: PathBuf::from("/worktrees/main-repo/feature-b/main-repo"),
            ref_name: Some("refs/heads/feature-b".into()),
            sha: "def".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;
    agent_ui::test_support::record_mav_created_worktree(
        fs.as_ref(),
        Path::new("/worktrees/main-repo/feature-b/main-repo"),
        None,
        cx,
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let mixed_project = project::Project::test(
        fs.clone(),
        [
            "/main-repo".as_ref(),
            "/worktrees/main-repo/feature-b/main-repo".as_ref(),
        ],
        cx,
    )
    .await;

    mixed_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;

    let (multi_workspace, cx) = cx
        .add_window_view(|window, cx| MultiWorkspace::test_new(mixed_project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);
    let workspace =
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());

    let worktree_ids: Vec<(WorktreeId, Arc<Path>)> = workspace.read_with(cx, |workspace, cx| {
        workspace
            .project()
            .read(cx)
            .visible_worktrees(cx)
            .map(|worktree| (worktree.read(cx).id(), worktree.read(cx).abs_path()))
            .collect()
    });

    let main_repo_worktree_id = worktree_ids
        .iter()
        .find(|(_, path)| path.as_ref() == Path::new("/main-repo"))
        .map(|(id, _)| *id)
        .expect("should find main-repo worktree");

    let feature_b_worktree_id = worktree_ids
        .iter()
        .find(|(_, path)| path.as_ref() == Path::new("/worktrees/main-repo/feature-b/main-repo"))
        .map(|(id, _)| *id)
        .expect("should find feature-b worktree");

    let main_repo_path = project::ProjectPath {
        worktree_id: main_repo_worktree_id,
        path: Arc::from(rel_path("src/lib.rs")),
    };
    let feature_b_path = project::ProjectPath {
        worktree_id: feature_b_worktree_id,
        path: Arc::from(rel_path("src/main.rs")),
    };

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(main_repo_path.clone(), None, true, window, cx)
        })
        .await
        .expect("should open main-repo file");
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(feature_b_path.clone(), None, true, window, cx)
        })
        .await
        .expect("should open feature-b file");

    let folder_paths = PathList::new(&[
        PathBuf::from("/main-repo"),
        PathBuf::from("/worktrees/main-repo/feature-b/main-repo"),
    ]);
    let main_worktree_paths =
        PathList::new(&[PathBuf::from("/main-repo"), PathBuf::from("/main-repo")]);
    let draft_id = save_draft_metadata_with_main_paths(
        Some("Mixed Workspace Draft".into()),
        folder_paths,
        main_worktree_paths,
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        cx,
    );
    cx.update(|_, cx| {
        agent_ui::draft_prompt_store::write(
            draft_id,
            &[acp::ContentBlock::Text(acp::TextContent::new(
                "mixed workspace draft",
            ))],
            cx,
        )
    })
    .await
    .expect("draft prompt should persist");

    sidebar.update(cx, |sidebar, cx| sidebar.update_entries(cx));
    cx.run_until_parked();

    let draft_index = sidebar.read_with(cx, |sidebar, _cx| {
        sidebar
            .contents
            .entries
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    ListEntry::Thread(thread) if thread.metadata.thread_id == draft_id
                )
            })
            .expect("mixed workspace draft should be visible")
    });

    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, _window, _cx| {
        sidebar.selection = Some(draft_index);
    });
    cx.dispatch_action(ArchiveSelectedThread);
    for _ in 0..8 {
        cx.run_until_parked();
    }

    assert_eq!(
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace
            .workspaces()
            .count()),
        1,
        "mixed workspace should be preserved"
    );

    let open_paths_after: Vec<project::ProjectPath> = workspace.read_with(cx, |workspace, cx| {
        workspace
            .panes()
            .iter()
            .flat_map(|pane| {
                pane.read(cx)
                    .items()
                    .filter_map(|item| item.project_path(cx))
            })
            .collect()
    });
    assert!(
        open_paths_after
            .iter()
            .any(|project_path| project_path.worktree_id == main_repo_worktree_id),
        "main-repo file should still be open"
    );
    assert!(
        !open_paths_after
            .iter()
            .any(|project_path| project_path.worktree_id == feature_b_worktree_id),
        "feature-b file should have been closed"
    );

    let draft_metadata_deleted = cx.update(|_, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry(draft_id)
            .is_none()
    });
    assert!(
        draft_metadata_deleted,
        "discarded draft metadata should be deleted"
    );
}

#[test]
fn test_worktree_info_branch_names_for_main_worktrees() {
    let folder_paths = PathList::new(&[PathBuf::from("/projects/myapp")]);
    let worktree_paths = WorktreePaths::from_folder_paths(&folder_paths);

    let branch_by_path: HashMap<PathBuf, SharedString> =
        [(PathBuf::from("/projects/myapp"), "feature-x".into())]
            .into_iter()
            .collect();

    let infos = worktree_info_from_thread_paths(&worktree_paths, &branch_by_path);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].kind, ui::WorktreeKind::Main);
    assert_eq!(infos[0].branch_name, Some(SharedString::from("feature-x")));
    assert_eq!(infos[0].worktree_name, Some(SharedString::from("myapp")));
}

#[test]
fn test_worktree_info_branch_names_for_linked_worktrees() {
    let main_paths = PathList::new(&[PathBuf::from("/projects/myapp")]);
    let folder_paths = PathList::new(&[PathBuf::from("/projects/myapp-feature")]);
    let worktree_paths =
        WorktreePaths::from_path_lists(main_paths, folder_paths).expect("same length");

    let branch_by_path: HashMap<PathBuf, SharedString> = [(
        PathBuf::from("/projects/myapp-feature"),
        "feature-branch".into(),
    )]
    .into_iter()
    .collect();

    let infos = worktree_info_from_thread_paths(&worktree_paths, &branch_by_path);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].kind, ui::WorktreeKind::Linked);
    assert_eq!(
        infos[0].branch_name,
        Some(SharedString::from("feature-branch"))
    );
}

#[test]
fn test_worktree_info_missing_branch_returns_none() {
    let folder_paths = PathList::new(&[PathBuf::from("/projects/myapp")]);
    let worktree_paths = WorktreePaths::from_folder_paths(&folder_paths);

    let branch_by_path: HashMap<PathBuf, SharedString> = HashMap::new();

    let infos = worktree_info_from_thread_paths(&worktree_paths, &branch_by_path);
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].kind, ui::WorktreeKind::Main);
    assert_eq!(infos[0].branch_name, None);
    assert_eq!(infos[0].worktree_name, Some(SharedString::from("myapp")));
}

#[gpui::test]
async fn test_remote_archive_thread_with_active_connection(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    // End-to-end test of archiving a remote thread tied to a linked git
    // worktree. Archival should:
    //  1. Persist the worktree's git state via the remote repository RPCs
    //     (head_sha / create_archive_checkpoint / update_ref).
    //  2. Remove the linked worktree directory from the *remote* filesystem
    //     via the GitRemoveWorktree RPC.
    //  3. Mark the thread metadata archived and hide it from the sidebar.
    //
    // The mock remote transport only supports one live `RemoteClient` per
    // connection at a time (each client's `start_proxy` replaces the
    // previous server channel), so we can't split the main repo and the
    // linked worktree across two remote projects the way Mav does in
    // production. Opening both as visible worktrees of a single remote
    // project still exercises every interesting path of the archive flow
    // while staying within the mock's multiplexing limits.
    init_test(cx);

    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        workspace::init(app_state.clone(), cx);
        app_state
    });

    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    // Set up the remote filesystem with a main repo and one linked worktree.
    let server_fs = FakeFs::new(server_cx.executor());
    server_fs
        .insert_tree(
            "/project",
            serde_json::json!({
                ".git": {
                    "worktrees": {
                        "feature-a": {
                            "commondir": "../../",
                            "HEAD": "ref: refs/heads/feature-a",
                        },
                    },
                },
                "src": { "main.rs": "fn main() {}" },
            }),
        )
        .await;
    server_fs
        .insert_tree(
            "/worktrees/project/feature-a/project",
            serde_json::json!({
                ".git": "gitdir: /project/.git/worktrees/feature-a",
                "src": { "lib.rs": "// feature" },
            }),
        )
        .await;
    server_fs
        .add_linked_worktree_for_repo(
            Path::new("/project/.git"),
            false,
            git::repository::Worktree {
                path: PathBuf::from("/worktrees/project/feature-a/project"),
                ref_name: Some("refs/heads/feature-a".into()),
                sha: "abc".into(),
                is_main: false,
                is_bare: false,
            },
        )
        .await;
    server_fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    server_fs.set_head_for_repo(
        Path::new("/project/.git"),
        &[("src/main.rs", "fn main() {}".into())],
        "head-sha",
    );

    // Open a single remote project with both the main repo and the linked
    // worktree as visible worktrees. The mock transport doesn't multiplex
    // multiple `RemoteClient`s over one pooled connection cleanly (each
    // client's `start_proxy` clobbers the previous one's server channel),
    // so we can't build two separate `Project::remote` instances in this
    // test. Folding both worktrees into one project still exercises the
    // archive flow's interesting paths: `build_root_plan` classifies the
    // linked worktree correctly, and `find_or_create_repository` finds
    // the main repo live on that same project — avoiding the temp-project
    // fallback that would also run into the multiplexing limitation.
    let (project, _headless, _opts) = start_remote_project(
        &server_fs,
        Path::new("/project"),
        &app_state,
        None,
        cx,
        server_cx,
    )
    .await;
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(
                Path::new("/worktrees/project/feature-a/project"),
                true,
                cx,
            )
        })
        .await
        .expect("should open linked worktree on remote");
    project.update(cx, |p, cx| p.git_scans_complete(cx)).await;
    cx.run_until_parked();

    cx.update(|cx| <dyn fs::Fs>::set_global(app_state.fs.clone(), cx));

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    // The worktree thread's (main_worktree_path, folder_path) pair points
    // the folder at the linked worktree checkout and the main at the
    // parent repo, so `build_root_plan` targets the linked worktree
    // specifically and knows which main repo owns it.
    let remote_connection = project.read_with(cx, |p, cx| p.remote_connection_options(cx));

    // Record the worktree as Mav-created on the client, keyed by the remote
    // connection identity, with the creation time of the gitdir on the
    // *remote* filesystem (where the archive flow will re-stat it).
    agent_ui::test_support::record_mav_created_worktree(
        server_fs.as_ref(),
        Path::new("/worktrees/project/feature-a/project"),
        remote_connection.as_ref(),
        cx,
    )
    .await;

    let wt_thread_id = acp::SessionId::new(Arc::from("worktree-thread"));
    cx.update(|_window, cx| {
        let metadata = ThreadMetadata {
            thread_id: ThreadId::new(),
            session_id: Some(wt_thread_id.clone()),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title: Some("Worktree Thread".into()),
            title_override: None,
            updated_at: chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0)
                .unwrap(),
            created_at: None,
            interacted_at: None,
            worktree_paths: WorktreePaths::from_path_lists(
                PathList::new(&[PathBuf::from("/project")]),
                PathList::new(&[PathBuf::from("/worktrees/project/feature-a/project")]),
            )
            .unwrap(),
            archived: false,
            remote_connection,
        };
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();

    assert!(
        server_fs
            .is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should exist on remote before archiving"
    );

    sidebar.update_in(cx, |sidebar: &mut Sidebar, window, cx| {
        sidebar.archive_thread(&wt_thread_id, window, cx);
    });
    cx.run_until_parked();
    server_cx.run_until_parked();

    let is_archived = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&wt_thread_id)
            .map(|t| t.archived)
            .unwrap_or(false)
    });
    assert!(is_archived, "worktree thread should be archived");

    assert!(
        !server_fs
            .is_dir(Path::new("/worktrees/project/feature-a/project"))
            .await,
        "linked worktree directory should be removed from remote fs \
         (the GitRemoveWorktree RPC runs `Repository::remove_worktree` \
         on the headless server, which deletes the directory via `Fs::remove_dir` \
         before running `git worktree remove --force`)"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries.iter().any(|e| e.contains("Worktree Thread")),
        "archived worktree thread should be hidden from sidebar: {entries:?}"
    );
}

#[gpui::test]
async fn test_remote_linked_worktree_workspace_to_remove_uses_remote_connection(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    init_test(cx);

    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        workspace::init(app_state.clone(), cx);
        app_state
    });

    let server_fs = FakeFs::new(server_cx.executor());
    server_fs
        .insert_tree(
            "/project",
            serde_json::json!({
                ".git": {},
                "src": {},
            }),
        )
        .await;
    server_fs
        .insert_tree(
            "/external-worktree",
            serde_json::json!({
                ".git": "gitdir: /project/.git/worktrees/feature-a",
                "src": {},
            }),
        )
        .await;
    server_fs.set_branch_name(Path::new("/project/.git"), Some("main"));
    server_fs.insert_branches(Path::new("/project/.git"), &["main", "feature-a"]);
    server_fs
        .add_linked_worktree_for_repo(
            Path::new("/project/.git"),
            false,
            git::repository::Worktree {
                path: PathBuf::from("/external-worktree"),
                ref_name: Some("refs/heads/feature-a".into()),
                sha: "abc".into(),
                is_main: false,
                is_bare: false,
            },
        )
        .await;

    let (worktree_project, _headless, remote_connection) = start_remote_project(
        &server_fs,
        Path::new("/external-worktree"),
        &app_state,
        None,
        cx,
        server_cx,
    )
    .await;
    worktree_project
        .update(cx, |project, cx| project.git_scans_complete(cx))
        .await;
    cx.run_until_parked();

    cx.update(|cx| <dyn fs::Fs>::set_global(app_state.fs.clone(), cx));

    let (multi_workspace, cx) = cx.add_window_view(|window, cx| {
        MultiWorkspace::test_new(worktree_project.clone(), window, cx)
    });
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let worktree_session_id = acp::SessionId::new(Arc::from("remote-worktree-thread"));
    let worktree_folder_paths = PathList::new(&[PathBuf::from("/external-worktree")]);
    let main_folder_paths = PathList::new(&[PathBuf::from("/project")]);
    let worktree_thread_id = ThreadId::new();
    cx.update(|_window, cx| {
        let metadata = ThreadMetadata {
            thread_id: worktree_thread_id,
            session_id: Some(worktree_session_id.clone()),
            agent_id: agent::MAV_AGENT_ID.clone(),
            title: Some("Remote Worktree Thread".into()),
            title_override: None,
            updated_at: chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
            created_at: None,
            interacted_at: None,
            worktree_paths: WorktreePaths::from_path_lists(
                main_folder_paths,
                worktree_folder_paths.clone(),
            )
            .unwrap(),
            archived: false,
            remote_connection: Some(remote_connection.clone()),
        };
        ThreadMetadataStore::global(cx).update(cx, |store, cx| store.save(metadata, cx));
    });
    cx.run_until_parked();

    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(
                    &worktree_folder_paths,
                    Some(&remote_connection),
                    cx,
                )
            })
            .is_some(),
        "remote linked-worktree workspace should be open before archiving"
    );
    assert!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace_for_paths(&worktree_folder_paths, None, cx)
            })
            .is_none(),
        "the test must exercise a remote-only workspace lookup"
    );
    assert_ne!(
        multi_workspace
            .read_with(cx, |multi_workspace, cx| {
                multi_workspace.workspace().read(cx).project_group_key(cx)
            })
            .path_list(),
        &worktree_folder_paths,
        "remote workspace must be classified as a linked worktree under the main project"
    );

    let workspace_to_remove = sidebar.read_with(cx, |sidebar, cx| {
        sidebar
            .linked_worktree_workspace_to_remove(
                &worktree_folder_paths,
                Some(&remote_connection),
                Some(worktree_thread_id),
                None,
                &[],
                cx,
            )
            .map(|workspace| workspace.entity_id())
    });
    let active_workspace_id = multi_workspace.read_with(cx, |multi_workspace, _cx| {
        multi_workspace.workspace().entity_id()
    });
    assert_eq!(
        workspace_to_remove,
        Some(active_workspace_id),
        "archive helper should resolve the remote linked-worktree workspace"
    );
    assert!(
        server_fs.is_dir(Path::new("/external-worktree")).await,
        "direct helper check should not remove the linked worktree from disk"
    );
}

#[gpui::test]
async fn test_remote_archive_thread_with_disconnected_remote(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    // When a remote thread has no linked-worktree state to archive (only
    // a main worktree), archival is a pure metadata operation: no RPCs
    // are issued against the remote server. This must succeed even when
    // the connection has dropped out, because losing connectivity should
    // not block users from cleaning up their thread list.
    //
    // Threads that *do* have linked-worktree state require a live
    // connection to run the git worktree removal on the server; that
    // path is covered by `test_remote_archive_thread_with_active_connection`.
    init_test(cx);

    cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let app_state = cx.update(|cx| {
        let app_state = workspace::AppState::test(cx);
        workspace::init(app_state.clone(), cx);
        app_state
    });

    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let server_fs = FakeFs::new(server_cx.executor());
    server_fs
        .insert_tree(
            "/project",
            serde_json::json!({
                ".git": {},
                "src": { "main.rs": "fn main() {}" },
            }),
        )
        .await;
    server_fs.set_branch_name(Path::new("/project/.git"), Some("main"));

    let (project, _headless, _opts) = start_remote_project(
        &server_fs,
        Path::new("/project"),
        &app_state,
        None,
        cx,
        server_cx,
    )
    .await;
    let remote_client = project
        .read_with(cx, |project, _cx| project.remote_client())
        .expect("remote project should expose its client");

    cx.update(|cx| <dyn fs::Fs>::set_global(app_state.fs.clone(), cx));

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let sidebar = setup_sidebar(&multi_workspace, cx);

    let thread_id = acp::SessionId::new(Arc::from("remote-thread"));
    save_thread_metadata(
        thread_id.clone(),
        Some("Remote Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        None,
        None,
        &project,
        cx,
    );
    cx.run_until_parked();

    // Sanity-check: there is nothing on the remote fs outside the main
    // repo, so archival should not need to touch the server.
    assert!(
        !server_fs.is_dir(Path::new("/worktrees")).await,
        "no linked worktrees on the server before archiving"
    );

    // Disconnect the remote connection before archiving. We don't
    // `run_until_parked` here because the disconnect itself triggers
    // reconnection work that can't complete in the test environment.
    remote_client.update(cx, |client, cx| {
        client.simulate_disconnect(cx).detach();
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.archive_thread(&thread_id, window, cx);
    });
    cx.run_until_parked();

    let is_archived = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&thread_id)
            .map(|t| t.archived)
            .unwrap_or(false)
    });
    assert!(
        is_archived,
        "thread should be archived even when remote is disconnected"
    );

    let entries = visible_entries_as_strings(&sidebar, cx);
    assert!(
        !entries.iter().any(|e| e.contains("Remote Thread")),
        "archived thread should be hidden from sidebar: {entries:?}"
    );
}

#[gpui::test]
async fn test_collab_guest_move_thread_paths_is_noop(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/project-a", serde_json::json!({ "src": {} }))
        .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;
    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));
    let project = project::Project::test(fs, ["/project-a".as_ref()], cx).await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));

    // Set up the sidebar while the project is local. This registers the
    // WorktreePathsChanged subscription for the project.
    let _sidebar = setup_sidebar(&multi_workspace, cx);

    let session_id = acp::SessionId::new(Arc::from("test-thread"));
    save_named_thread_metadata("test-thread", "My Thread", &project, cx).await;

    let thread_id = cx.update(|_window, cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(&session_id)
            .map(|e| e.thread_id)
            .expect("thread must be in the store")
    });

    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx);
        let entry = store.read(cx).entry(thread_id).unwrap();
        assert_eq!(
            entry.folder_paths().paths(),
            &[PathBuf::from("/project-a")],
            "thread must be saved with /project-a before collab"
        );
    });

    // Transition the project into collab mode. The sidebar's subscription is
    // still active from when the project was local.
    project.update(cx, |project, _cx| {
        project.mark_as_collab_for_testing();
    });

    // Adding a worktree fires WorktreePathsChanged with old_paths = {/project-a}.
    // The sidebar's subscription is still active, so move_thread_paths is called.
    // Without the is_via_collab() guard inside move_thread_paths, this would
    // update the stored thread paths from {/project-a} to {/project-a, /project-b}.
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/project-b", true, cx)
        })
        .await
        .expect("should add worktree");
    cx.run_until_parked();

    cx.update(|_window, cx| {
        let store = ThreadMetadataStore::global(cx);
        let entry = store
            .read(cx)
            .entry(thread_id)
            .expect("thread must still exist");
        assert_eq!(
            entry.folder_paths().paths(),
            &[PathBuf::from("/project-a")],
            "thread path must not change when project is via collab"
        );
    });
}

#[gpui::test]
async fn test_cmd_click_project_header_returns_to_last_active_linked_worktree_workspace(
    cx: &mut TestAppContext,
) {
    // Regression test for: cmd-clicking a project group header should return
    // the user to the workspace they most recently had active in that group,
    // including workspaces rooted at a linked worktree.
    init_test(cx);
    let fs = FakeFs::new(cx.executor());

    fs.insert_tree(
        "/project-a",
        serde_json::json!({
            ".git": {},
            "src": {},
        }),
    )
    .await;
    fs.insert_tree("/project-b", serde_json::json!({ "src": {} }))
        .await;

    fs.add_linked_worktree_for_repo(
        Path::new("/project-a/.git"),
        false,
        git::repository::Worktree {
            path: std::path::PathBuf::from("/wt-feature-a"),
            ref_name: Some("refs/heads/feature-a".into()),
            sha: "aaa".into(),
            is_main: false,
            is_bare: false,
        },
    )
    .await;

    cx.update(|cx| <dyn fs::Fs>::set_global(fs.clone(), cx));

    let main_project_a = project::Project::test(fs.clone(), ["/project-a".as_ref()], cx).await;
    let worktree_project_a =
        project::Project::test(fs.clone(), ["/wt-feature-a".as_ref()], cx).await;
    let project_b = project::Project::test(fs.clone(), ["/project-b".as_ref()], cx).await;

    main_project_a
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;
    worktree_project_a
        .update(cx, |p, cx| p.git_scans_complete(cx))
        .await;

    // The multi-workspace starts with the main-paths workspace of group A
    // as the initially active workspace.
    let (multi_workspace, cx) = cx
        .add_window_view(|window, cx| MultiWorkspace::test_new(main_project_a.clone(), window, cx));

    let sidebar = setup_sidebar(&multi_workspace, cx);

    // Capture the initially active workspace (group A's main-paths workspace)
    // *before* registering additional workspaces, since `workspaces()` returns
    // retained workspaces in registration order — not activation order — and
    // the multi-workspace's starting workspace may not be retained yet.
    let main_workspace_a = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    // Register the linked-worktree workspace (group A) and the group-B
    // workspace. Both get retained by the multi-workspace.
    let worktree_workspace_a = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(worktree_project_a.clone(), window, cx)
    });
    let workspace_b = multi_workspace.update_in(cx, |mw, window, cx| {
        mw.test_add_workspace(project_b.clone(), window, cx)
    });

    cx.run_until_parked();

    // Step 1: activate the linked-worktree workspace. The MultiWorkspace
    // records this as the last-active workspace for group A on its
    // ProjectGroupState. (We don't assert on the initial active workspace
    // because `test_add_workspace` may auto-activate newly registered
    // workspaces — what matters for this test is the explicit sequence of
    // activations below.)
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(worktree_workspace_a.clone(), None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        worktree_workspace_a,
        "linked-worktree workspace should be active after step 1"
    );

    // Step 2: switch to the workspace for group B. Group A's last-active
    // workspace remains the linked-worktree one (group B getting activated
    // records *its own* last-active workspace, not group A's).
    multi_workspace.update_in(cx, |mw, window, cx| {
        mw.activate(workspace_b.clone(), None, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        multi_workspace.read_with(cx, |mw, _| mw.workspace().clone()),
        workspace_b,
        "group B's workspace should be active after step 2"
    );

    // Step 3: simulate cmd-click on group A's header. The project group key
    // for group A is derived from the *main-paths* workspace (linked-worktree
    // workspaces share the same key because it normalizes to main-worktree
    // paths).
    let group_a_key = main_workspace_a.read_with(cx, |ws, cx| ws.project_group_key(cx));
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.activate_or_open_workspace_for_group(&group_a_key, window, cx);
    });
    cx.run_until_parked();

    // Expected: we're back in the linked-worktree workspace, not the
    // main-paths one.
    let active_after_cmd_click = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    assert_eq!(
        active_after_cmd_click, worktree_workspace_a,
        "cmd-click on group A's header should return to the last-active \
         linked-worktree workspace, not the main-paths workspace"
    );
    assert_ne!(
        active_after_cmd_click, main_workspace_a,
        "cmd-click must not fall back to the main-paths workspace when a \
         linked-worktree workspace was the last-active one for the group"
    );
}

#[test]
fn test_split_leading_icon_char() {
    // A leading symbol set off by whitespace is pulled out and trimmed from the
    // title.
    let (icon, title, positions) =
        split_leading_icon_char(&"✳ Implement separate config".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "✳");
    assert_eq!(title.as_ref(), "Implement separate config");
    assert_eq!(positions, Vec::<usize>::new());

    // No prefix when the title starts with a letter.
    assert!(split_leading_icon_char(&"Implement separate config".into(), &[]).is_none());

    // Leading whitespace is not treated as a prefix.
    assert!(split_leading_icon_char(&" leading space".into(), &[]).is_none());

    // An alphanumeric prefix such as a version marker is not treated as an icon.
    assert!(split_leading_icon_char(&"v1 Running".into(), &[]).is_none());
    assert!(split_leading_icon_char(&"1 first".into(), &[]).is_none());

    // A title consisting only of a symbol (no whitespace separator) is left
    // untouched.
    assert!(split_leading_icon_char(&"✳".into(), &[]).is_none());
    assert!(split_leading_icon_char(&"✳Thinking".into(), &[]).is_none());

    // A run of the same symbol collapses to a single glyph.
    let (icon, title, _) = split_leading_icon_char(&">>> Thinking".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), ">");
    assert_eq!(title.as_ref(), "Thinking");

    // Surrounding ASCII brackets are stripped so the inner glyph is used.
    let (icon, title, _) = split_leading_icon_char(&"[!] codex waiting".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "!");
    assert_eq!(title.as_ref(), "codex waiting");

    // A run of dots is condensed into an ellipsis.
    let (icon, title, _) = split_leading_icon_char(&"... working".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "\u{2026}");
    assert_eq!(title.as_ref(), "working");

    let (icon, title, _) = split_leading_icon_char(&"[...] working".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "\u{2026}");
    assert_eq!(title.as_ref(), "working");

    let (icon, title, _) = split_leading_icon_char(&"[…] working".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "\u{2026}");
    assert_eq!(title.as_ref(), "working");

    // Multi-codepoint emoji are kept intact rather than sliced mid-cluster.
    let (icon, title, _) = split_leading_icon_char(&"🇺🇸 flag".into(), &[]).unwrap();
    assert_eq!(icon.as_ref(), "🇺🇸");
    assert_eq!(title.as_ref(), "flag");

    // Highlight positions are shifted to account for the stripped prefix, and
    // positions that fall inside the stripped prefix are dropped.
    let title: SharedString = "# abc".into();
    let abc_offset = title.find('a').unwrap();
    let (icon, trimmed, positions) =
        split_leading_icon_char(&title, &[0, abc_offset, abc_offset + 1]).unwrap();
    assert_eq!(icon.as_ref(), "#");
    assert_eq!(trimmed.as_ref(), "abc");
    assert_eq!(positions, vec![0, 1]);
}
