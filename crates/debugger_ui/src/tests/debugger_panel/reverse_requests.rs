use super::*;

#[gpui::test]
async fn test_handle_successful_run_in_terminal_reverse_request(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    // needed because the debugger launches a terminal which starts a background PTY
    cx.executor().allow_parking();
    init_test(cx);

    let send_response = Arc::new(AtomicBool::new(false));

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

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    client
        .on_response::<RunInTerminal, _>({
            let send_response = send_response.clone();
            move |response| {
                send_response.store(true, Ordering::SeqCst);

                assert!(response.success);
                assert!(response.body.is_some());
            }
        })
        .await;

    client
        .fake_reverse_request::<RunInTerminal>(RunInTerminalRequestArguments {
            kind: None,
            title: None,
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            args: vec![],
            env: None,
            args_can_be_interpreted_by_shell: None,
        })
        .await;

    cx.run_until_parked();

    assert!(
        send_response.load(std::sync::atomic::Ordering::SeqCst),
        "Expected to receive response from reverse request"
    );

    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let session = debug_panel.read(cx).active_session().unwrap();
            let running = session.read(cx).running_state();
            assert_eq!(
                running
                    .read(cx)
                    .pane_items_status(cx)
                    .get(&DebuggerPaneItem::Terminal),
                Some(&true)
            );
            assert!(running.read(cx).debug_terminal.read(cx).terminal.is_some());
        })
        .unwrap();
}

#[gpui::test]
async fn test_handle_start_debugging_request(
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

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    let fake_config = json!({"one": "two"});
    let launched_with = Arc::new(parking_lot::Mutex::new(None));

    let _subscription = project::debugger::test::intercept_debug_sessions(cx, {
        let launched_with = launched_with.clone();
        move |client| {
            let launched_with = launched_with.clone();
            client.on_request::<dap::requests::Launch, _>(move |_, args| {
                launched_with.lock().replace(args.raw);
                Ok(())
            });
            client.on_request::<dap::requests::Attach, _>(move |_, _| {
                assert!(false, "should not get attach request");
                Ok(())
            });
        }
    });

    let sessions = workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            debug_panel.read(cx).sessions().collect::<Vec<_>>()
        })
        .unwrap();
    assert_eq!(sessions.len(), 1);
    client
        .fake_reverse_request::<StartDebugging>(StartDebuggingRequestArguments {
            request: StartDebuggingRequestArgumentsRequest::Launch,
            configuration: fake_config.clone(),
        })
        .await;

    cx.run_until_parked();

    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();

            // Active session changes on spawn, as the parent has never stopped.
            let active_session = debug_panel
                .read(cx)
                .active_session()
                .unwrap()
                .read(cx)
                .session(cx);
            let current_sessions = debug_panel.read(cx).sessions().collect::<Vec<_>>();
            assert_eq!(active_session, current_sessions[1].read(cx).session(cx));
            assert_eq!(
                active_session.read(cx).parent_session(),
                Some(&current_sessions[0].read(cx).session(cx))
            );

            assert_eq!(current_sessions.len(), 2);
            assert_eq!(current_sessions[0], sessions[0]);

            let parent_session = current_sessions[1]
                .read(cx)
                .session(cx)
                .read(cx)
                .parent_session()
                .unwrap();
            assert_eq!(parent_session, &sessions[0].read(cx).session(cx));

            // We should preserve the original binary (params to spawn process etc.) except for launch params
            // (as they come from reverse spawn request).
            let mut original_binary = parent_session.read(cx).binary().cloned().unwrap();
            original_binary.request_args = StartDebuggingRequestArguments {
                request: StartDebuggingRequestArgumentsRequest::Launch,
                configuration: fake_config.clone(),
            };

            assert_eq!(
                current_sessions[1]
                    .read(cx)
                    .session(cx)
                    .read(cx)
                    .binary()
                    .unwrap(),
                &original_binary
            );
        })
        .unwrap();

    assert_eq!(&fake_config, launched_with.lock().as_ref().unwrap());
}

// // covers that we always send a response back, if something when wrong,
// // while spawning the terminal
#[gpui::test]
async fn test_handle_error_run_in_terminal_reverse_request(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let send_response = Arc::new(AtomicBool::new(false));

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

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    client
        .on_response::<RunInTerminal, _>({
            let send_response = send_response.clone();
            move |response| {
                send_response.store(true, Ordering::SeqCst);

                assert!(!response.success);
                assert!(response.body.is_some());
            }
        })
        .await;

    client
        .fake_reverse_request::<RunInTerminal>(RunInTerminalRequestArguments {
            kind: None,
            title: None,
            cwd: "".into(),
            args: vec!["oops".into(), "oops".into()],
            env: None,
            args_can_be_interpreted_by_shell: None,
        })
        .await;

    cx.run_until_parked();

    assert!(
        send_response.load(std::sync::atomic::Ordering::SeqCst),
        "Expected to receive response from reverse request"
    );

    workspace
        .update(cx, |workspace, _window, cx| {
            let terminal_panel = workspace.panel::<TerminalPanel>(cx).unwrap();

            assert_eq!(
                0,
                terminal_panel.read(cx).pane().unwrap().read(cx).items_len()
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_handle_start_debugging_reverse_request(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    init_test(cx);

    let send_response = Arc::new(AtomicBool::new(false));

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

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    client.on_request::<dap::requests::Threads, _>(move |_, _| {
        Ok(dap::ThreadsResponse {
            threads: vec![dap::Thread {
                id: 1,
                name: "Thread 1".into(),
            }],
        })
    });

    client
        .on_response::<StartDebugging, _>({
            let send_response = send_response.clone();
            move |response| {
                send_response.store(true, Ordering::SeqCst);

                assert!(response.success);
                assert!(response.body.is_some());
            }
        })
        .await;
    // Set up handlers for sessions spawned with reverse request too.
    let _reverse_request_subscription =
        project::debugger::test::intercept_debug_sessions(cx, |_| {});
    client
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
    let child_client = child_session.update(cx, |session, _| session.adapter_client().unwrap());

    child_client.on_request::<dap::requests::Threads, _>(move |_, _| {
        Ok(dap::ThreadsResponse {
            threads: vec![dap::Thread {
                id: 1,
                name: "Thread 1".into(),
            }],
        })
    });

    child_client.on_request::<Disconnect, _>(move |_, _| Ok(()));

    child_client
        .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
            reason: dap::StoppedEventReason::Pause,
            description: None,
            thread_id: Some(2),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;

    cx.run_until_parked();

    assert!(
        send_response.load(std::sync::atomic::Ordering::SeqCst),
        "Expected to receive response from reverse request"
    );
}
