use super::*;

#[gpui::test(iterations = 10)]
async fn test_ssh_restarting_language_server_replaces_remote_status(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    cx_a.set_name("a");
    server_cx.set_name("server");

    cx_a.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
    });

    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let log_store = cx_a.update(|cx| log_store::init(false, cx));

    let (opts, server_ssh, _) = RemoteClient::fake_server(cx_a, server_cx);
    let remote_fs = FakeFs::new(server_cx.executor());
    remote_fs
        .insert_tree(path!("/project"), json!({ "a.rs": "fn main() {}" }))
        .await;

    client_a.language_registry().add(rust_lang());

    server_cx.update(HeadlessProject::init);
    let languages = Arc::new(LanguageRegistry::new(server_cx.executor()));
    languages.add(rust_lang());
    let mut fake_language_servers = languages.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            ..Default::default()
        },
    );
    let _headless_project = server_cx.new(|cx| {
        HeadlessProject::new(
            HeadlessAppState {
                session: server_ssh,
                fs: remote_fs.clone(),
                http_client: Arc::new(BlockedHttpClient),
                node_runtime: NodeRuntime::unavailable(),
                languages,
                extension_host_proxy: Arc::new(ExtensionHostProxy::new()),
                startup_time: std::time::Instant::now(),
            },
            false,
            cx,
        )
    });

    let client_ssh = RemoteClient::connect_mock(opts, cx_a).await;
    let (project_a, worktree_id) = client_a
        .build_ssh_project(path!("/project"), client_ssh, false, cx_a)
        .await;
    log_store.update(cx_a, |log_store, cx| log_store.add_project(&project_a, cx));

    let (buffer, _handle) = project_a
        .update(cx_a, |project, cx| {
            project.open_buffer_with_lsp((worktree_id, rel_path("a.rs")), cx)
        })
        .await
        .unwrap();

    let first_server = fake_language_servers.next().await.unwrap();
    let first_server_id = first_server.server.server_id();
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, cx| {
        let statuses = project.language_server_statuses(cx).collect::<Vec<_>>();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].0, first_server_id);
        assert_eq!(statuses[0].1.name.0, "the-language-server");
    });
    cx_a.read_global::<GlobalLogStore, _>(|global, cx| {
        let log_store = global.0.read(cx);
        let matching_server_ids = log_store
            .language_servers
            .iter()
            .filter_map(|(server_id, state)| {
                state
                    .name
                    .as_ref()
                    .is_some_and(|name| name.0 == "the-language-server")
                    .then_some(*server_id)
            })
            .collect::<Vec<_>>();
        assert_eq!(matching_server_ids, vec![first_server_id]);
    });

    project_a.update(cx_a, |project, cx| {
        project.restart_language_servers_for_buffers(vec![buffer], HashSet::default(), true, cx);
    });

    let restarted_server = fake_language_servers.next().await.unwrap();
    let restarted_server_id = restarted_server.server.server_id();
    assert_ne!(restarted_server_id, first_server_id);
    executor.run_until_parked();

    project_a.read_with(cx_a, |project, cx| {
        let statuses = project.language_server_statuses(cx).collect::<Vec<_>>();
        assert_eq!(
            statuses.len(),
            1,
            "restarting a remote language server should replace the previous status entry"
        );
        assert_eq!(
            statuses[0].0, restarted_server_id,
            "restarting a remote language server should publish the replacement server id"
        );
        assert_ne!(
            statuses[0].0, first_server_id,
            "restarting a remote language server should remove the previous server id"
        );
        assert_eq!(statuses[0].1.name.0, "the-language-server");
    });
    cx_a.read_global::<GlobalLogStore, _>(|global, cx| {
        let log_store = global.0.read(cx);
        let matching_server_ids = log_store
            .language_servers
            .iter()
            .filter_map(|(server_id, state)| {
                state
                    .name
                    .as_ref()
                    .is_some_and(|name| name.0 == "the-language-server")
                    .then_some(*server_id)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matching_server_ids,
            vec![restarted_server_id],
            "restarting a remote language server should replace the old log store entry"
        );
        assert!(
            !log_store.language_servers.contains_key(&first_server_id),
            "restarting a remote language server should remove the previous log store entry"
        );
    });
}
