use super::*;

pub(super) async fn save_n_test_threads(
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

pub(super) async fn save_test_thread_metadata(
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

pub(super) async fn save_named_thread_metadata(
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
pub(super) fn seed_thread_metadata(metadata: ThreadMetadata, cx: &mut TestAppContext) {
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
pub(super) async fn start_remote_project(
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

pub(super) fn save_thread_metadata(
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

pub(super) fn save_thread_metadata_with_main_paths(
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

pub(super) fn save_draft_metadata_with_main_paths(
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
