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
#[path = "sidebar_tests/archive_diverged_worktree_paths.rs"]
mod archive_diverged_worktree_paths;
#[path = "sidebar_tests/archive_linked_worktree_threads.rs"]
mod archive_linked_worktree_threads;
#[path = "sidebar_tests/archive_mixed_workspace_items.rs"]
mod archive_mixed_workspace_items;
#[path = "sidebar_tests/archive_path_resolution.rs"]
mod archive_path_resolution;
#[path = "sidebar_tests/archive_terminal_worktrees.rs"]
mod archive_terminal_worktrees;
#[path = "sidebar_tests/archive_thread_worktrees.rs"]
mod archive_thread_worktrees;
#[path = "sidebar_tests/archive_visibility.rs"]
mod archive_visibility;
#[path = "sidebar_tests/archive_worktree_cleanup.rs"]
mod archive_worktree_cleanup;
#[path = "sidebar_tests/collab_and_header_activation.rs"]
mod collab_and_header_activation;
#[path = "sidebar_tests/discard_mixed_workspace_draft.rs"]
mod discard_mixed_workspace_draft;
#[path = "sidebar_tests/draft_activation_and_path_migration.rs"]
mod draft_activation_and_path_migration;
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
#[path = "sidebar_tests/icon_parsing.rs"]
mod icon_parsing;
#[path = "sidebar_tests/keyboard_navigation.rs"]
mod keyboard_navigation;
#[path = "sidebar_tests/linked_worktree_archive.rs"]
mod linked_worktree_archive;
#[path = "sidebar_tests/linked_worktree_terminal_close.rs"]
mod linked_worktree_terminal_close;
#[path = "sidebar_tests/linked_worktree_thread_visibility.rs"]
mod linked_worktree_thread_visibility;
#[path = "sidebar_tests/remote_archive_active.rs"]
mod remote_archive_active;
#[path = "sidebar_tests/remote_archive_edge_cases.rs"]
mod remote_archive_edge_cases;
#[path = "sidebar_tests/remote_project_integration.rs"]
mod remote_project_integration;
#[path = "sidebar_tests/search.rs"]
mod search;
#[path = "sidebar_tests/sidebar_basic_entries.rs"]
mod sidebar_basic_entries;
#[path = "sidebar_tests/sidebar_measurement_serialization.rs"]
mod sidebar_measurement_serialization;
#[path = "sidebar_tests/startup_restoration.rs"]
mod startup_restoration;
#[path = "sidebar_tests/thread_rename.rs"]
mod thread_rename;
#[path = "sidebar_tests/thread_status_selection.rs"]
mod thread_status_selection;
#[path = "sidebar_tests/thread_switcher_ordering.rs"]
mod thread_switcher_ordering;
#[path = "sidebar_tests/thread_switcher_terminal_rows.rs"]
mod thread_switcher_terminal_rows;
#[path = "sidebar_tests/unarchive_existing_workspace.rs"]
mod unarchive_existing_workspace;
#[path = "sidebar_tests/unarchive_workspace_drafts.rs"]
mod unarchive_workspace_drafts;
#[path = "sidebar_tests/visible_entries_snapshot.rs"]
mod visible_entries_snapshot;
#[path = "sidebar_tests/workspace_lifecycle.rs"]
mod workspace_lifecycle;
#[path = "sidebar_tests/worktree_activation.rs"]
mod worktree_activation;
#[path = "sidebar_tests/worktree_chips.rs"]
mod worktree_chips;
#[path = "sidebar_tests/worktree_discovery.rs"]
mod worktree_discovery;
#[path = "sidebar_tests/worktree_info_unit.rs"]
mod worktree_info_unit;
#[path = "sidebar_tests/worktree_live_open.rs"]
mod worktree_live_open;
#[path = "sidebar_tests/worktree_reachability.rs"]
mod worktree_reachability;
#[path = "sidebar_tests/worktree_restore_git.rs"]
mod worktree_restore_git;
#[path = "sidebar_tests/worktree_restore_sidebar.rs"]
mod worktree_restore_sidebar;

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

#[path = "sidebar_tests/property_test.rs"]
mod property_test;
