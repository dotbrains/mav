use super::*;

#[gpui::test]
async fn test_basic_show_debug_panel(executor: BackgroundExecutor, cx: &mut TestAppContext) {
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

    client.on_request::<Threads, _>(move |_, _| {
        Ok(dap::ThreadsResponse {
            threads: vec![dap::Thread {
                id: 1,
                name: "Thread 1".into(),
            }],
        })
    });

    client.on_request::<StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: Vec::default(),
            total_frames: None,
        })
    });

    cx.run_until_parked();

    // assert we have a debug panel item before the session has stopped
    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_session =
                debug_panel.update(cx, |debug_panel, _| debug_panel.active_session().unwrap());

            let running_state = active_session.update(cx, |active_session, _| {
                active_session.running_state().clone()
            });

            debug_panel.update(cx, |this, cx| {
                assert!(this.active_session().is_some());
                assert!(running_state.read(cx).selected_thread_id().is_none());
            });
        })
        .unwrap();

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

    cx.run_until_parked();

    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_session = debug_panel
                .update(cx, |this, _| this.active_session())
                .unwrap();

            let running_state = active_session.update(cx, |active_session, _| {
                active_session.running_state().clone()
            });

            assert_eq!(client.id(), running_state.read(cx).session_id());
            assert_eq!(
                ThreadId(1),
                running_state.read(cx).selected_thread_id().unwrap()
            );
        })
        .unwrap();

    let shutdown_session = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(session.read(cx).session_id(), cx)
        })
    });

    shutdown_session.await.unwrap();

    // assert we still have a debug panel item after the client shutdown
    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();

            let active_session = debug_panel
                .update(cx, |this, _| this.active_session())
                .unwrap();

            let running_state = active_session.update(cx, |active_session, _| {
                active_session.running_state().clone()
            });

            debug_panel.update(cx, |this, cx| {
                assert!(this.active_session().is_some());
                assert_eq!(
                    ThreadId(1),
                    running_state.read(cx).selected_thread_id().unwrap()
                );
            });
        })
        .unwrap();
}

#[gpui::test]
async fn test_we_can_only_have_one_panel_per_debug_session(
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

    client.on_request::<Threads, _>(move |_, _| {
        Ok(dap::ThreadsResponse {
            threads: vec![dap::Thread {
                id: 1,
                name: "Thread 1".into(),
            }],
        })
    });

    client.on_request::<StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: Vec::default(),
            total_frames: None,
        })
    });

    cx.run_until_parked();

    // assert we have a debug panel item before the session has stopped
    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();

            debug_panel.update(cx, |this, _| {
                assert!(this.active_session().is_some());
            });
        })
        .unwrap();

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

    cx.run_until_parked();

    // assert we added a debug panel item
    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_session = debug_panel
                .update(cx, |this, _| this.active_session())
                .unwrap();

            let running_state = active_session.update(cx, |active_session, _| {
                active_session.running_state().clone()
            });

            assert_eq!(client.id(), active_session.read(cx).session_id(cx));
            assert_eq!(
                ThreadId(1),
                running_state.read(cx).selected_thread_id().unwrap()
            );
        })
        .unwrap();

    client
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

    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_session = debug_panel
                .update(cx, |this, _| this.active_session())
                .unwrap();

            let running_state = active_session.update(cx, |active_session, _| {
                active_session.running_state().clone()
            });

            assert_eq!(client.id(), active_session.read(cx).session_id(cx));
            assert_eq!(
                ThreadId(1),
                running_state.read(cx).selected_thread_id().unwrap()
            );
        })
        .unwrap();

    let shutdown_session = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(session.read(cx).session_id(), cx)
        })
    });

    shutdown_session.await.unwrap();

    // assert we still have a debug panel item after the client shutdown
    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_session = debug_panel
                .update(cx, |this, _| this.active_session())
                .unwrap();

            let running_state = active_session.update(cx, |active_session, _| {
                active_session.running_state().clone()
            });

            debug_panel.update(cx, |this, cx| {
                assert!(this.active_session().is_some());
                assert_eq!(
                    ThreadId(1),
                    running_state.read(cx).selected_thread_id().unwrap()
                );
            });
        })
        .unwrap();
}
