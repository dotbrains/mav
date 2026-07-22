use super::*;

#[gpui::test]
async fn test_adapter_shutdown_with_child_sessions_on_app_quit(
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
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);

    let parent_session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let parent_session_id = cx.read(|cx| parent_session.read(cx).session_id());
    let parent_client = parent_session.update(cx, |session, _| session.adapter_client().unwrap());

    let disconnect_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let parent_disconnect_called = Arc::new(AtomicBool::new(false));
    let parent_disconnect_clone = parent_disconnect_called.clone();
    let disconnect_count_clone = disconnect_count.clone();

    parent_client.on_request::<Disconnect, _>(move |_, _| {
        parent_disconnect_clone.store(true, Ordering::SeqCst);
        disconnect_count_clone.fetch_add(1, Ordering::SeqCst);

        for _ in 0..50 {
            if disconnect_count_clone.load(Ordering::SeqCst) >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        Ok(())
    });

    parent_client
        .on_response::<StartDebugging, _>(move |_| {})
        .await;
    let _subscription = project::debugger::test::intercept_debug_sessions(cx, |_| {});

    parent_client
        .fake_reverse_request::<StartDebugging>(StartDebuggingRequestArguments {
            configuration: json!({}),
            request: StartDebuggingRequestArgumentsRequest::Launch,
        })
        .await;

    cx.run_until_parked();

    let child_session = project.update(cx, |project, cx| {
        project
            .dap_store()
            .read(cx)
            .session_by_id(SessionId(1))
            .unwrap()
    });
    let child_session_id = cx.read(|cx| child_session.read(cx).session_id());
    let child_client = child_session.update(cx, |session, _| session.adapter_client().unwrap());

    let child_disconnect_called = Arc::new(AtomicBool::new(false));
    let child_disconnect_clone = child_disconnect_called.clone();
    let disconnect_count_clone = disconnect_count.clone();

    child_client.on_request::<Disconnect, _>(move |_, _| {
        child_disconnect_clone.store(true, Ordering::SeqCst);
        disconnect_count_clone.fetch_add(1, Ordering::SeqCst);

        for _ in 0..50 {
            if disconnect_count_clone.load(Ordering::SeqCst) >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        Ok(())
    });

    executor.run_until_parked();

    project.update(cx, |project, cx| {
        let store = project.dap_store().read(cx);
        assert!(store.session_by_id(parent_session_id).is_some());
        assert!(store.session_by_id(child_session_id).is_some());
    });

    cx.update(|_, cx| cx.defer(|cx| cx.shutdown()));

    executor.run_until_parked();

    let parent_disconnect_check = parent_disconnect_called.clone();
    let child_disconnect_check = child_disconnect_called.clone();
    let executor_clone = executor.clone();
    let both_disconnected = executor
        .spawn(async move {
            let parent_disconnect = parent_disconnect_check;
            let child_disconnect = child_disconnect_check;

            // We only have 100ms to shutdown the app
            for _ in 0..100 {
                if parent_disconnect.load(Ordering::SeqCst)
                    && child_disconnect.load(Ordering::SeqCst)
                {
                    return true;
                }

                executor_clone
                    .timer(std::time::Duration::from_millis(1))
                    .await;
            }

            false
        })
        .await;

    assert!(
        both_disconnected,
        "Both parent and child sessions should receive disconnect requests"
    );

    assert!(
        parent_disconnect_called.load(Ordering::SeqCst),
        "Parent session should have received disconnect request"
    );
    assert!(
        child_disconnect_called.load(Ordering::SeqCst),
        "Child session should have received disconnect request"
    );
}
