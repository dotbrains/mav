use super::*;

#[gpui::test]
async fn test_stack_frame_filter_persistence(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    fs.insert_tree(
        path!("/project"),
        json!({
           "src": {
               "test.js": "function main() { console.log('hello'); }",
           }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);
    workspace
        .update(cx, |workspace, _, cx| {
            workspace.set_random_database_id(cx);
        })
        .unwrap();

    let threads_response = dap::ThreadsResponse {
        threads: vec![dap::Thread {
            id: 1,
            name: "Thread 1".into(),
        }],
    };

    let stack_trace_response = dap::StackTraceResponse {
        stack_frames: vec![StackFrame {
            id: 1,
            name: "main".into(),
            source: Some(dap::Source {
                name: Some("test.js".into()),
                path: Some(path!("/project/src/test.js").into()),
                source_reference: None,
                presentation_hint: None,
                origin: None,
                sources: None,
                adapter_data: None,
                checksums: None,
            }),
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: None,
        }],
        total_frames: None,
    };

    let stopped_event = dap::StoppedEvent {
        reason: dap::StoppedEventReason::Pause,
        description: None,
        thread_id: Some(1),
        preserve_focus_hint: None,
        text: None,
        all_threads_stopped: None,
        hit_breakpoint_ids: None,
    };

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());
    let adapter_name = session.update(cx, |session, _| session.adapter());

    client.on_request::<Threads, _>({
        let threads_response = threads_response.clone();
        move |_, _| Ok(threads_response.clone())
    });

    client.on_request::<Scopes, _>(move |_, _| Ok(dap::ScopesResponse { scopes: vec![] }));

    client.on_request::<StackTrace, _>({
        let stack_trace_response = stack_trace_response.clone();
        move |_, _| Ok(stack_trace_response.clone())
    });

    client
        .fake_event(dap::messages::Events::Stopped(stopped_event.clone()))
        .await;

    cx.run_until_parked();

    let stack_frame_list =
        active_debug_session_panel(workspace, cx).update(cx, |debug_panel_item, cx| {
            debug_panel_item
                .running_state()
                .update(cx, |state, _| state.stack_frame_list().clone())
        });

    stack_frame_list.update(cx, |stack_frame_list, _cx| {
        assert_eq!(
            stack_frame_list.list_filter(),
            StackFrameFilter::All,
            "Initial filter should be All"
        );
    });

    stack_frame_list.update(cx, |stack_frame_list, cx| {
        stack_frame_list
            .toggle_frame_filter(Some(project::debugger::session::ThreadStatus::Stopped), cx);
        assert_eq!(
            stack_frame_list.list_filter(),
            StackFrameFilter::OnlyUserFrames,
            "Filter should be OnlyUserFrames after toggle"
        );
    });

    cx.run_until_parked();

    let workspace_id = workspace
        .update(cx, |workspace, _window, cx| workspace.database_id(cx))
        .ok()
        .flatten()
        .expect("workspace id has to be some for this test to work properly");

    let key = stack_frame_filter_key(&adapter_name, workspace_id);
    let stored_value = cx
        .update(|_, cx| KeyValueStore::global(cx))
        .read_kvp(&key)
        .unwrap();
    assert_eq!(
        stored_value,
        Some(StackFrameFilter::OnlyUserFrames.into()),
        "Filter should be persisted in KVP store with key: {}",
        key
    );

    client
        .fake_event(dap::messages::Events::Terminated(None))
        .await;
    cx.run_until_parked();

    let session2 = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client2 = session2.update(cx, |session, _| session.adapter_client().unwrap());

    client2.on_request::<Threads, _>({
        let threads_response = threads_response.clone();
        move |_, _| Ok(threads_response.clone())
    });

    client2.on_request::<Scopes, _>(move |_, _| Ok(dap::ScopesResponse { scopes: vec![] }));

    client2.on_request::<StackTrace, _>({
        let stack_trace_response = stack_trace_response.clone();
        move |_, _| Ok(stack_trace_response.clone())
    });

    client2
        .fake_event(dap::messages::Events::Stopped(stopped_event.clone()))
        .await;

    cx.run_until_parked();

    let stack_frame_list2 =
        active_debug_session_panel(workspace, cx).update(cx, |debug_panel_item, cx| {
            debug_panel_item
                .running_state()
                .update(cx, |state, _| state.stack_frame_list().clone())
        });

    stack_frame_list2.update(cx, |stack_frame_list, _cx| {
        assert_eq!(
            stack_frame_list.list_filter(),
            StackFrameFilter::OnlyUserFrames,
            "Filter should be restored from KVP store in new session"
        );
    });
}
