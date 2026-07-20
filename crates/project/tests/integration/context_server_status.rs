use crate::context_server_store::*;

#[gpui::test]
async fn test_context_server_status(cx: &mut TestAppContext) {
    const SERVER_1_ID: &str = "mcp-1";
    const SERVER_2_ID: &str = "mcp-2";

    let (_fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;

    let registry = cx.new(|_| ContextServerDescriptorRegistry::new());
    let store = cx.new(|cx| {
        ContextServerStore::test(
            registry.clone(),
            project.read(cx).worktree_store(),
            Some(project.downgrade()),
            cx,
        )
    });

    let server_1_id = ContextServerId(SERVER_1_ID.into());
    let server_2_id = ContextServerId(SERVER_2_ID.into());

    let server_1 = Arc::new(ContextServer::new(
        server_1_id.clone(),
        Arc::new(create_fake_transport(SERVER_1_ID, cx.executor())),
    ));
    let server_2 = Arc::new(ContextServer::new(
        server_2_id.clone(),
        Arc::new(create_fake_transport(SERVER_2_ID, cx.executor())),
    ));

    store.update(cx, |store, cx| store.test_start_server(server_1, cx));

    cx.run_until_parked();

    cx.update(|cx| {
        assert_eq!(
            store.read(cx).status_for_server(&server_1_id),
            Some(ContextServerStatus::Running)
        );
        assert_eq!(store.read(cx).status_for_server(&server_2_id), None);
    });

    store.update(cx, |store, cx| {
        store.test_start_server(server_2.clone(), cx)
    });

    cx.run_until_parked();

    cx.update(|cx| {
        assert_eq!(
            store.read(cx).status_for_server(&server_1_id),
            Some(ContextServerStatus::Running)
        );
        assert_eq!(
            store.read(cx).status_for_server(&server_2_id),
            Some(ContextServerStatus::Running)
        );
    });

    store
        .update(cx, |store, cx| store.stop_server(&server_2_id, cx))
        .unwrap();

    cx.update(|cx| {
        assert_eq!(
            store.read(cx).status_for_server(&server_1_id),
            Some(ContextServerStatus::Running)
        );
        assert_eq!(
            store.read(cx).status_for_server(&server_2_id),
            Some(ContextServerStatus::Stopped)
        );
    });
}

#[gpui::test]
async fn test_context_server_status_events(cx: &mut TestAppContext) {
    const SERVER_1_ID: &str = "mcp-1";
    const SERVER_2_ID: &str = "mcp-2";

    let (_fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;

    let registry = cx.new(|_| ContextServerDescriptorRegistry::new());
    let store = cx.new(|cx| {
        ContextServerStore::test(
            registry.clone(),
            project.read(cx).worktree_store(),
            Some(project.downgrade()),
            cx,
        )
    });

    let server_1_id = ContextServerId(SERVER_1_ID.into());
    let server_2_id = ContextServerId(SERVER_2_ID.into());

    let server_1 = Arc::new(ContextServer::new(
        server_1_id.clone(),
        Arc::new(create_fake_transport(SERVER_1_ID, cx.executor())),
    ));
    let server_2 = Arc::new(ContextServer::new(
        server_2_id.clone(),
        Arc::new(create_fake_transport(SERVER_2_ID, cx.executor())),
    ));

    let _server_events = assert_server_events(
        &store,
        vec![
            (server_1_id.clone(), ContextServerStatus::Starting),
            (server_1_id, ContextServerStatus::Running),
            (server_2_id.clone(), ContextServerStatus::Starting),
            (server_2_id.clone(), ContextServerStatus::Running),
            (server_2_id.clone(), ContextServerStatus::Stopped),
        ],
        cx,
    );

    store.update(cx, |store, cx| store.test_start_server(server_1, cx));

    cx.run_until_parked();

    store.update(cx, |store, cx| {
        store.test_start_server(server_2.clone(), cx)
    });

    cx.run_until_parked();

    store
        .update(cx, |store, cx| store.stop_server(&server_2_id, cx))
        .unwrap();
}

#[gpui::test(iterations = 25)]
async fn test_context_server_concurrent_starts(cx: &mut TestAppContext) {
    const SERVER_1_ID: &str = "mcp-1";

    let (_fs, project) = setup_context_server_test(cx, json!({"code.rs": ""}), vec![]).await;

    let registry = cx.new(|_| ContextServerDescriptorRegistry::new());
    let store = cx.new(|cx| {
        ContextServerStore::test(
            registry.clone(),
            project.read(cx).worktree_store(),
            Some(project.downgrade()),
            cx,
        )
    });

    let server_id = ContextServerId(SERVER_1_ID.into());

    let server_with_same_id_1 = Arc::new(ContextServer::new(
        server_id.clone(),
        Arc::new(create_fake_transport(SERVER_1_ID, cx.executor())),
    ));
    let server_with_same_id_2 = Arc::new(ContextServer::new(
        server_id.clone(),
        Arc::new(create_fake_transport(SERVER_1_ID, cx.executor())),
    ));

    // If we start another server with the same id, we should report that we stopped the previous one
    let _server_events = assert_server_events(
        &store,
        vec![
            (server_id.clone(), ContextServerStatus::Starting),
            (server_id.clone(), ContextServerStatus::Stopped),
            (server_id.clone(), ContextServerStatus::Starting),
            (server_id.clone(), ContextServerStatus::Running),
        ],
        cx,
    );

    store.update(cx, |store, cx| {
        store.test_start_server(server_with_same_id_1.clone(), cx)
    });
    store.update(cx, |store, cx| {
        store.test_start_server(server_with_same_id_2.clone(), cx)
    });

    cx.run_until_parked();

    cx.update(|cx| {
        assert_eq!(
            store.read(cx).status_for_server(&server_id),
            Some(ContextServerStatus::Running)
        );
    });
}
