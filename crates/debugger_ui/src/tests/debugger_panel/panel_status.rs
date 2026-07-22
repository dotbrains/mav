use super::*;

#[gpui::test]
async fn test_debug_panel_item_thread_status_reset_on_failure(
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

    let session = start_debug_session(&workspace, cx, |client| {
        client.on_request::<dap::requests::Initialize, _>(move |_, _| {
            Ok(dap::Capabilities {
                supports_step_back: Some(true),
                ..Default::default()
            })
        });
    })
    .unwrap();

    let client = session.update(cx, |session, _| session.adapter_client().unwrap());
    const THREAD_ID_NUM: i64 = 1;

    client.on_request::<dap::requests::Threads, _>(move |_, _| {
        Ok(dap::ThreadsResponse {
            threads: vec![dap::Thread {
                id: THREAD_ID_NUM,
                name: "Thread 1".into(),
            }],
        })
    });

    client.on_request::<Launch, _>(move |_, _| Ok(()));

    client.on_request::<StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: Vec::default(),
            total_frames: None,
        })
    });

    client.on_request::<Next, _>(move |_, _| {
        Err(ErrorResponse {
            error: Some(dap::Message {
                id: 1,
                format: "error".into(),
                variables: None,
                send_telemetry: None,
                show_user: None,
                url: None,
                url_label: None,
            }),
        })
    });

    client.on_request::<StepOut, _>(move |_, _| {
        Err(ErrorResponse {
            error: Some(dap::Message {
                id: 1,
                format: "error".into(),
                variables: None,
                send_telemetry: None,
                show_user: None,
                url: None,
                url_label: None,
            }),
        })
    });

    client.on_request::<StepIn, _>(move |_, _| {
        Err(ErrorResponse {
            error: Some(dap::Message {
                id: 1,
                format: "error".into(),
                variables: None,
                send_telemetry: None,
                show_user: None,
                url: None,
                url_label: None,
            }),
        })
    });

    client.on_request::<StepBack, _>(move |_, _| {
        Err(ErrorResponse {
            error: Some(dap::Message {
                id: 1,
                format: "error".into(),
                variables: None,
                send_telemetry: None,
                show_user: None,
                url: None,
                url_label: None,
            }),
        })
    });

    client.on_request::<Continue, _>(move |_, _| {
        Err(ErrorResponse {
            error: Some(dap::Message {
                id: 1,
                format: "error".into(),
                variables: None,
                send_telemetry: None,
                show_user: None,
                url: None,
                url_label: None,
            }),
        })
    });

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

    let running_state = active_debug_session_panel(workspace, cx)
        .read_with(cx, |item, _| item.running_state().clone());

    cx.run_until_parked();
    let thread_id = ThreadId(1);

    for operation in &[
        "step_over",
        "continue_thread",
        "step_back",
        "step_in",
        "step_out",
    ] {
        running_state.update(cx, |running_state, cx| match *operation {
            "step_over" => running_state.step_over(cx),
            "continue_thread" => running_state.continue_thread(cx),
            "step_back" => running_state.step_back(cx),
            "step_in" => running_state.step_in(cx),
            "step_out" => running_state.step_out(cx),
            _ => unreachable!(),
        });

        // Check that we step the thread status to the correct intermediate state
        running_state.update(cx, |running_state, cx| {
            assert_eq!(
                running_state
                    .thread_status(cx)
                    .expect("There should be an active thread selected"),
                match *operation {
                    "continue_thread" => ThreadStatus::Running,
                    _ => ThreadStatus::Stepping,
                },
                "Thread status was not set to correct intermediate state after {} request",
                operation
            );
        });

        cx.run_until_parked();

        running_state.update(cx, |running_state, cx| {
            assert_eq!(
                running_state
                    .thread_status(cx)
                    .expect("There should be an active thread selected"),
                ThreadStatus::Stopped,
                "Thread status not reset to Stopped after failed {}",
                operation
            );

            // update state to running, so we can test it actually changes the status back to stopped
            running_state
                .session()
                .update(cx, |session, cx| session.continue_thread(thread_id, cx));
        });
    }
}
