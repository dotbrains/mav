use super::*;

#[gpui::test]
async fn test_remote_telemetry_event_forwarding(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    // This mirrors `init_test`, but retains the server-side session so the test
    // can drive a forwarded telemetry event over the proto channel (as
    // `init_telemetry_forwarding` does on a real remote server).
    let server_fs = FakeFs::new(server_cx.executor());
    server_fs
        .insert_tree(
            path!("/code"),
            json!({ "project1": { "README.md": "# project 1" } }),
        )
        .await;

    cx.update(|cx| release_channel::init(semver::Version::new(0, 0, 0), cx));
    server_cx.update(|cx| release_channel::init(semver::Version::new(0, 0, 0), cx));
    init_logger();

    let (opts, server_session, _) = RemoteClient::fake_server(cx, server_cx);
    let http_client = Arc::new(BlockedHttpClient);
    let node_runtime = NodeRuntime::unavailable();
    let languages = Arc::new(LanguageRegistry::new(cx.executor()));
    let proxy = Arc::new(ExtensionHostProxy::new());
    server_cx.update(HeadlessProject::init);
    let headless = server_cx.new(|cx| {
        HeadlessProject::new(
            crate::HeadlessAppState {
                session: server_session.clone(),
                fs: server_fs.clone(),
                http_client,
                node_runtime,
                languages,
                extension_host_proxy: proxy,
                startup_time: std::time::Instant::now(),
            },
            false,
            cx,
        )
    });

    let ssh = RemoteClient::connect_mock(opts, cx).await;
    let project = build_project(ssh, cx);
    project
        .update(cx, {
            let headless = headless.clone();
            |_, cx| cx.on_release(|_, _| drop(headless))
        })
        .detach();

    // The remote server forwards a bare `FlexibleEvent` as JSON; mirror that
    // here by sending the proto message the forwarding task would send.
    let event_json = json!({
        "event_type": "fs_watcher_poll",
        "event_properties": { "path": "/code/project1" },
    })
    .to_string();
    server_session
        .send(proto::TelemetryEvent {
            project_id: proto::REMOTE_SERVER_PROJECT_ID,
            event_json,
        })
        .unwrap();
    cx.executor().run_until_parked();

    let events = project.read_with(cx, |project, _| {
        project.client().telemetry().queued_events()
    });
    assert_eq!(
        events.len(),
        1,
        "the forwarded event should be reported once"
    );
    let event = &events[0];
    assert_eq!(event.event_type, "fs_watcher_poll");
    // The event's original properties survive the round-trip.
    assert_eq!(
        event.event_properties.get("path"),
        Some(&serde_json::Value::String("/code/project1".to_string()))
    );
    // The client stamps the remote host metadata it learned at connection time.
    // The mock connection reports a Linux/x86_64 host over a "mock" connection.
    assert_eq!(
        event.event_properties.get("remote"),
        Some(&serde_json::Value::Bool(true))
    );
    assert_eq!(
        event.event_properties.get("remote_connection_type"),
        Some(&serde_json::Value::String("mock".to_string()))
    );
    assert_eq!(
        event.event_properties.get("remote_os_name"),
        Some(&serde_json::Value::String("Linux".to_string()))
    );
    assert_eq!(
        event.event_properties.get("remote_architecture"),
        Some(&serde_json::Value::String("x86_64".to_string()))
    );
}
