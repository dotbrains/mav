use super::*;

#[gpui::test]
async fn test_remote_server_debugger(
    cx_a: &mut TestAppContext,
    server_cx: &mut TestAppContext,
    executor: BackgroundExecutor,
) {
    cx_a.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        command_palette_hooks::init(cx);
        zlog::init_test();
        dap_adapters::init(cx);
    });
    server_cx.update(|cx| {
        release_channel::init(semver::Version::new(0, 0, 0), cx);
        dap_adapters::init(cx);
    });
    let (opts, server_ssh, _) = RemoteClient::fake_server(cx_a, server_cx);
    let remote_fs = FakeFs::new(server_cx.executor());
    remote_fs
        .insert_tree(
            path!("/code"),
            json!({
                "lib.rs": "fn one() -> usize { 1 }"
            }),
        )
        .await;

    // User A connects to the remote project via SSH.
    server_cx.update(HeadlessProject::init);
    let remote_http_client = Arc::new(BlockedHttpClient);
    let node = NodeRuntime::unavailable();
    let languages = Arc::new(LanguageRegistry::new(server_cx.executor()));
    let _headless_project = server_cx.new(|cx| {
        HeadlessProject::new(
            HeadlessAppState {
                session: server_ssh,
                fs: remote_fs.clone(),
                http_client: remote_http_client,
                node_runtime: node,
                languages,
                extension_host_proxy: Arc::new(ExtensionHostProxy::new()),
                startup_time: std::time::Instant::now(),
            },
            false,
            cx,
        )
    });

    let client_ssh = RemoteClient::connect_mock(opts, cx_a).await;
    let mut server = TestServer::start(server_cx.executor()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    cx_a.update(|cx| {
        debugger_ui::init(cx);
        command_palette_hooks::init(cx);
    });
    let (project_a, _) = client_a
        .build_ssh_project(path!("/code"), client_ssh.clone(), false, cx_a)
        .await;

    let (workspace, cx_a) = client_a.build_workspace(&project_a, cx_a);

    let debugger_panel = workspace
        .update_in(cx_a, |_workspace, window, cx| {
            cx.spawn_in(window, DebugPanel::load)
        })
        .await
        .unwrap();

    workspace.update_in(cx_a, |workspace, window, cx| {
        workspace.add_panel(debugger_panel, window, cx);
    });

    cx_a.run_until_parked();
    let debug_panel = workspace
        .update(cx_a, |workspace, cx| workspace.panel::<DebugPanel>(cx))
        .unwrap();

    let workspace_window = cx_a
        .window_handle()
        .downcast::<workspace::MultiWorkspace>()
        .unwrap();

    let session = debugger_ui::tests::start_debug_session(&workspace_window, cx_a, |_| {}).unwrap();
    cx_a.run_until_parked();
    debug_panel.update(cx_a, |debug_panel, cx| {
        assert_eq!(
            debug_panel.active_session().unwrap().read(cx).session(cx),
            session.clone()
        )
    });

    session.update(
        cx_a,
        |session: &mut project::debugger::session::Session, _| {
            assert_eq!(session.binary().unwrap().command.as_deref(), Some("mock"));
        },
    );

    let shutdown_session = workspace.update(cx_a, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.dap_store().update(cx, |dap_store, cx| {
                dap_store.shutdown_session(session.read(cx).session_id(), cx)
            })
        })
    });

    client_ssh.update(cx_a, |a, _| {
        a.shutdown_processes(Some(proto::ShutdownRemoteServer {}), executor)
    });

    shutdown_session.await.unwrap();
}
