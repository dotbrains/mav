use super::*;

#[gpui::test]
async fn test_shutdown_children_when_parent_session_shutdown(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    fs.insert_tree(
        path!("/project"),
        json!({
            "main.rs": "First line\nSecond line\nThird line\nFourth line",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let dap_store = project.update(cx, |project, _| project.dap_store());
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let parent_session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = parent_session.update(cx, |session, _| session.adapter_client().unwrap());

    client.on_request::<dap::requests::Threads, _>(move |_, _| {
        Ok(dap::ThreadsResponse {
            threads: vec![dap::Thread {
                id: 1,
                name: "Thread 1".into(),
            }],
        })
    });

    client.on_response::<StartDebugging, _>(move |_| {}).await;
    // Set up handlers for sessions spawned with reverse request too.
    let _reverse_request_subscription =
        project::debugger::test::intercept_debug_sessions(cx, |_| {});
    // start first child session
    client
        .fake_reverse_request::<StartDebugging>(StartDebuggingRequestArguments {
            configuration: json!({}),
            request: StartDebuggingRequestArgumentsRequest::Launch,
        })
        .await;

    cx.run_until_parked();

    // start second child session
    client
        .fake_reverse_request::<StartDebugging>(StartDebuggingRequestArguments {
            configuration: json!({}),
            request: StartDebuggingRequestArgumentsRequest::Launch,
        })
        .await;

    cx.run_until_parked();

    // configure first child session
    let first_child_session = dap_store.read_with(cx, |dap_store, _| {
        dap_store.session_by_id(SessionId(1)).unwrap()
    });
    let first_child_client =
        first_child_session.update(cx, |session, _| session.adapter_client().unwrap());

    first_child_client.on_request::<Disconnect, _>(move |_, _| Ok(()));

    // configure second child session
    let second_child_session = dap_store.read_with(cx, |dap_store, _| {
        dap_store.session_by_id(SessionId(2)).unwrap()
    });
    let second_child_client =
        second_child_session.update(cx, |session, _| session.adapter_client().unwrap());

    second_child_client.on_request::<Disconnect, _>(move |_, _| Ok(()));

    cx.run_until_parked();

    // shutdown parent session
    dap_store
        .update(cx, |dap_store, cx| {
            dap_store.shutdown_session(parent_session.read(cx).session_id(), cx)
        })
        .await
        .unwrap();

    // assert parent session and all children sessions are shutdown
    dap_store.update(cx, |dap_store, cx| {
        assert!(
            dap_store
                .session_by_id(parent_session.read(cx).session_id())
                .is_none()
        );
        assert!(
            dap_store
                .session_by_id(first_child_session.read(cx).session_id())
                .is_none()
        );
        assert!(
            dap_store
                .session_by_id(second_child_session.read(cx).session_id())
                .is_none()
        );
    });
}

#[gpui::test]
async fn test_shutdown_parent_session_if_all_children_are_shutdown(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    fs.insert_tree(
        path!("/project"),
        json!({
            "main.rs": "First line\nSecond line\nThird line\nFourth line",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let dap_store = project.update(cx, |project, _| project.dap_store());
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let parent_session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = parent_session.update(cx, |session, _| session.adapter_client().unwrap());

    client.on_response::<StartDebugging, _>(move |_| {}).await;
    // Set up handlers for sessions spawned with reverse request too.
    let _reverse_request_subscription =
        project::debugger::test::intercept_debug_sessions(cx, |_| {});
    // start first child session
    client
        .fake_reverse_request::<StartDebugging>(StartDebuggingRequestArguments {
            configuration: json!({}),
            request: StartDebuggingRequestArgumentsRequest::Launch,
        })
        .await;

    cx.run_until_parked();

    // start second child session
    client
        .fake_reverse_request::<StartDebugging>(StartDebuggingRequestArguments {
            configuration: json!({}),
            request: StartDebuggingRequestArgumentsRequest::Launch,
        })
        .await;

    cx.run_until_parked();

    // configure first child session
    let first_child_session = dap_store.read_with(cx, |dap_store, _| {
        dap_store.session_by_id(SessionId(1)).unwrap()
    });
    let first_child_client =
        first_child_session.update(cx, |session, _| session.adapter_client().unwrap());

    first_child_client.on_request::<Disconnect, _>(move |_, _| Ok(()));

    // configure second child session
    let second_child_session = dap_store.read_with(cx, |dap_store, _| {
        dap_store.session_by_id(SessionId(2)).unwrap()
    });
    let second_child_client =
        second_child_session.update(cx, |session, _| session.adapter_client().unwrap());

    second_child_client.on_request::<Disconnect, _>(move |_, _| Ok(()));

    cx.run_until_parked();

    // shutdown first child session
    dap_store
        .update(cx, |dap_store, cx| {
            dap_store.shutdown_session(first_child_session.read(cx).session_id(), cx)
        })
        .await
        .unwrap();

    // assert parent session and second child session still exist
    dap_store.update(cx, |dap_store, cx| {
        assert!(
            dap_store
                .session_by_id(parent_session.read(cx).session_id())
                .is_some()
        );
        assert!(
            dap_store
                .session_by_id(first_child_session.read(cx).session_id())
                .is_none()
        );
        assert!(
            dap_store
                .session_by_id(second_child_session.read(cx).session_id())
                .is_some()
        );
    });

    // shutdown first child session
    dap_store
        .update(cx, |dap_store, cx| {
            dap_store.shutdown_session(second_child_session.read(cx).session_id(), cx)
        })
        .await
        .unwrap();

    // assert parent session got shutdown by second child session
    // because it was the last child
    dap_store.update(cx, |dap_store, cx| {
        assert!(
            dap_store
                .session_by_id(parent_session.read(cx).session_id())
                .is_none()
        );
        assert!(
            dap_store
                .session_by_id(second_child_session.read(cx).session_id())
                .is_none()
        );
    });
}
