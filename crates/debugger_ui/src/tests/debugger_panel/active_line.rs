use super::*;

#[gpui::test]
async fn test_active_debug_line_setting(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    fs.insert_tree(
        path!("/project"),
        json!({
            "main.rs": "First line\nSecond line\nThird line\nFourth line",
            "second.rs": "First line\nSecond line\nThird line\nFourth line",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/project").as_ref()], cx).await;
    let workspace = init_test_workspace(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(*workspace, cx);
    let project_path = Path::new(path!("/project"));
    let worktree = project
        .update(cx, |project, cx| project.find_worktree(project_path, cx))
        .expect("This worktree should exist in project")
        .0;

    let worktree_id = workspace
        .update(cx, |_, _, cx| worktree.read(cx).id())
        .unwrap();

    let main_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let second_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("second.rs")), cx)
        })
        .await
        .unwrap();

    let (main_editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(main_buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    let (second_editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(second_buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

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

    client.on_request::<dap::requests::Scopes, _>(move |_, _| {
        Ok(dap::ScopesResponse {
            scopes: Vec::default(),
        })
    });

    client.on_request::<StackTrace, _>(move |_, args| {
        assert_eq!(args.thread_id, 1);

        Ok(dap::StackTraceResponse {
            stack_frames: vec![dap::StackFrame {
                id: 1,
                name: "frame 1".into(),
                source: Some(dap::Source {
                    name: Some("main.rs".into()),
                    path: Some(path!("/project/main.rs").into()),
                    source_reference: None,
                    presentation_hint: None,
                    origin: None,
                    sources: None,
                    adapter_data: None,
                    checksums: None,
                }),
                line: 2,
                column: 0,
                end_line: None,
                end_column: None,
                can_restart: None,
                instruction_pointer_reference: None,
                module_id: None,
                presentation_hint: None,
            }],
            total_frames: None,
        })
    });

    client
        .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
            reason: dap::StoppedEventReason::Breakpoint,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;

    cx.run_until_parked();

    main_editor.update_in(cx, |editor, window, cx| {
        let active_debug_lines: Vec<_> = editor.highlighted_rows::<ActiveDebugLine>(cx).collect();

        assert_eq!(
            active_debug_lines.len(),
            1,
            "There should be only one active debug line"
        );

        let point = editor
            .snapshot(window, cx)
            .buffer_snapshot()
            .summary_for_anchor::<language::Point>(&active_debug_lines.first().unwrap().0.start);

        assert_eq!(point.row, 1);
    });

    second_editor.update(cx, |editor, cx| {
        let active_debug_lines: Vec<_> = editor.highlighted_rows::<ActiveDebugLine>(cx).collect();

        assert!(
            active_debug_lines.is_empty(),
            "There shouldn't be any active debug lines"
        );
    });

    let handled_second_stacktrace = Arc::new(AtomicBool::new(false));
    client.on_request::<StackTrace, _>({
        let handled_second_stacktrace = handled_second_stacktrace.clone();
        move |_, args| {
            handled_second_stacktrace.store(true, Ordering::SeqCst);
            assert_eq!(args.thread_id, 1);

            Ok(dap::StackTraceResponse {
                stack_frames: vec![dap::StackFrame {
                    id: 2,
                    name: "frame 2".into(),
                    source: Some(dap::Source {
                        name: Some("second.rs".into()),
                        path: Some(path!("/project/second.rs").into()),
                        source_reference: None,
                        presentation_hint: None,
                        origin: None,
                        sources: None,
                        adapter_data: None,
                        checksums: None,
                    }),
                    line: 3,
                    column: 0,
                    end_line: None,
                    end_column: None,
                    can_restart: None,
                    instruction_pointer_reference: None,
                    module_id: None,
                    presentation_hint: None,
                }],
                total_frames: None,
            })
        }
    });

    client
        .fake_event(dap::messages::Events::Stopped(dap::StoppedEvent {
            reason: dap::StoppedEventReason::Breakpoint,
            description: None,
            thread_id: Some(1),
            preserve_focus_hint: None,
            text: None,
            all_threads_stopped: None,
            hit_breakpoint_ids: None,
        }))
        .await;

    cx.run_until_parked();

    second_editor.update_in(cx, |editor, window, cx| {
        let active_debug_lines: Vec<_> = editor.highlighted_rows::<ActiveDebugLine>(cx).collect();

        assert_eq!(
            active_debug_lines.len(),
            1,
            "There should be only one active debug line"
        );

        let point = editor
            .snapshot(window, cx)
            .buffer_snapshot()
            .summary_for_anchor::<language::Point>(&active_debug_lines.first().unwrap().0.start);

        assert_eq!(point.row, 2);
    });

    main_editor.update(cx, |editor, cx| {
        let active_debug_lines: Vec<_> = editor.highlighted_rows::<ActiveDebugLine>(cx).collect();

        assert!(
            active_debug_lines.is_empty(),
            "There shouldn't be any active debug lines"
        );
    });

    assert!(
        handled_second_stacktrace.load(Ordering::SeqCst),
        "Second stacktrace request handler was not called"
    );

    client
        .fake_event(dap::messages::Events::Continued(dap::ContinuedEvent {
            thread_id: 0,
            all_threads_continued: Some(true),
        }))
        .await;

    cx.run_until_parked();

    second_editor.update(cx, |editor, cx| {
        let active_debug_lines: Vec<_> = editor.highlighted_rows::<ActiveDebugLine>(cx).collect();

        assert!(
            active_debug_lines.is_empty(),
            "There shouldn't be any active debug lines"
        );
    });

    main_editor.update(cx, |editor, cx| {
        let active_debug_lines: Vec<_> = editor.highlighted_rows::<ActiveDebugLine>(cx).collect();

        assert!(
            active_debug_lines.is_empty(),
            "There shouldn't be any active debug lines"
        );
    });

    // Clean up
    let shutdown_session = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(session.read(cx).session_id(), cx)
        })
    });

    shutdown_session.await.unwrap();

    main_editor.update(cx, |editor, cx| {
        let active_debug_lines: Vec<_> = editor.highlighted_rows::<ActiveDebugLine>(cx).collect();

        assert!(
            active_debug_lines.is_empty(),
            "There shouldn't be any active debug lines after session shutdown"
        );
    });

    second_editor.update(cx, |editor, cx| {
        let active_debug_lines: Vec<_> = editor.highlighted_rows::<ActiveDebugLine>(cx).collect();

        assert!(
            active_debug_lines.is_empty(),
            "There shouldn't be any active debug lines after session shutdown"
        );
    });
}

#[gpui::test]
async fn test_debug_adapters_shutdown_on_app_quit(
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

    let disconnect_request_received = Arc::new(AtomicBool::new(false));
    let disconnect_clone = disconnect_request_received.clone();

    client.on_request::<Disconnect, _>(move |_, _| {
        disconnect_clone.store(true, Ordering::SeqCst);
        Ok(())
    });

    executor.run_until_parked();

    workspace
        .update(cx, |workspace, _, cx| {
            let panel = workspace.panel::<DebugPanel>(cx).unwrap();
            panel.read_with(cx, |panel, _| {
                assert!(
                    panel.sessions().next().is_some(),
                    "Debug session should be active"
                );
            });
        })
        .unwrap();

    cx.update(|_, cx| cx.defer(|cx| cx.shutdown()));

    executor.run_until_parked();

    assert!(
        disconnect_request_received.load(Ordering::SeqCst),
        "Disconnect request should have been sent to the adapter on app shutdown"
    );
}
