use super::*;

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
