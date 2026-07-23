use super::*;

#[gpui::test]
async fn test_slow_adapter_startup_retries(
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

    let count = Arc::new(AtomicUsize::new(0));
    let session = debugger_ui::tests::start_debug_session_with(
        &workspace_window,
        cx_a,
        DebugTaskDefinition {
            adapter: "fake-adapter".into(),
            label: "test".into(),
            config: json!({
                "request": "launch"
            }),
            tcp_connection: Some(TcpArgumentsTemplate {
                port: None,
                host: None,
                timeout: None,
            }),
        },
        move |client| {
            let count = count.clone();
            client.on_request_ext::<dap::requests::Initialize, _>(move |_seq, _request| {
                if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 5 {
                    return RequestHandling::Exit;
                }
                RequestHandling::Respond(Ok(Capabilities::default()))
            });
        },
    )
    .unwrap();
    cx_a.run_until_parked();

    let client = session.update(
        cx_a,
        |session: &mut project::debugger::session::Session, _| session.adapter_client().unwrap(),
    );
    client
        .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
            reason: dap::StoppedEventReason::Pause,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;

    cx_a.run_until_parked();

    let active_session = debug_panel
        .update(cx_a, |this, _| this.active_session())
        .unwrap();

    let running_state = active_session.update(cx_a, |active_session, _| {
        active_session.running_state().clone()
    });

    assert_eq!(
        client.id(),
        running_state.read_with(cx_a, |running_state, _| running_state.session_id())
    );
    assert_eq!(
        ThreadId(1),
        running_state.read_with(cx_a, |running_state, _| running_state
            .selected_thread_id()
            .unwrap())
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
