use super::*;

#[gpui::test]
async fn test_handle_output_event(executor: BackgroundExecutor, cx: &mut TestAppContext) {
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
    workspace
        .update(cx, |workspace, window, cx| {
            workspace.focus_panel::<DebugPanel>(window, cx);
        })
        .unwrap();

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.read_with(cx, |session, _| session.adapter_client().unwrap());

    client.on_request::<StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: Vec::default(),
            total_frames: None,
        })
    });

    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: None,
            output: "First console output line before thread stopped!".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;

    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: Some(dap::OutputEventCategory::Stdout),
            output: "First output line before thread stopped!".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;

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

    let running_state =
        active_debug_session_panel(workspace, cx).update_in(cx, |item, window, cx| {
            cx.focus_self(window);
            item.running_state().clone()
        });

    cx.run_until_parked();

    // assert we have output from before the thread stopped
    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_debug_session_panel = debug_panel
                .update(cx, |this, _| this.active_session())
                .unwrap();

            assert_eq!(
                "First console output line before thread stopped!\nFirst output line before thread stopped!\n",
                active_debug_session_panel.read(cx).running_state().read(cx).console().read(cx).editor().read(cx).text(cx).as_str()
            );
        })
        .unwrap();

    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: Some(dap::OutputEventCategory::Stdout),
            output: "\tSecond output line after thread stopped!".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;

    client
        .fake_event(dap::messages::Events::Output(dap::OutputEvent {
            category: Some(dap::OutputEventCategory::Console),
            output: "\tSecond console output line after thread stopped!".to_string(),
            data: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            group: None,
            location_reference: None,
        }))
        .await;

    cx.run_until_parked();
    running_state.update(cx, |_, cx| {
        cx.refresh_windows();
    });
    cx.run_until_parked();

    // assert we have output from before and after the thread stopped
    workspace
        .update(cx, |workspace, _window, cx| {
            let debug_panel = workspace.panel::<DebugPanel>(cx).unwrap();
            let active_session_panel = debug_panel
                .update(cx, |this, _| this.active_session())
                .unwrap();

            assert_eq!(
                "First console output line before thread stopped!\nFirst output line before thread stopped!\n\tSecond output line after thread stopped!\n\tSecond console output line after thread stopped!\n",
                active_session_panel.read(cx).running_state().read(cx).console().read(cx).editor().read(cx).text(cx).as_str()
            );
        })
        .unwrap();
}
