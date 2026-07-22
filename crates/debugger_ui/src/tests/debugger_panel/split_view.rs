use super::*;

#[gpui::test]
async fn test_breakpoint_jumps_only_in_proper_split_view(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
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
    let pane_a = workspace
        .update(cx, |multi, _window, cx| {
            multi.workspace().read(cx).active_pane().clone()
        })
        .unwrap();

    let open_main = workspace
        .update(cx, |multi, window, cx| {
            multi.workspace().update(cx, |workspace, cx| {
                workspace.open_path((worktree_id, rel_path("main.rs")), None, true, window, cx)
            })
        })
        .unwrap();
    open_main.await.unwrap();

    cx.run_until_parked();
    let pane_b = workspace
        .update(cx, |multi, window, cx| {
            multi.workspace().update(cx, |workspace, cx| {
                workspace.split_pane(pane_a.clone(), SplitDirection::Right, window, cx)
            })
        })
        .unwrap();

    cx.run_until_parked();
    let weak_pane_b = pane_b.downgrade();
    let open_main_in_b = workspace
        .update(cx, |multi, window, cx| {
            multi.workspace().update(cx, |workspace, cx| {
                workspace.open_path(
                    (worktree_id, rel_path("main.rs")),
                    Some(weak_pane_b),
                    true,
                    window,
                    cx,
                )
            })
        })
        .unwrap();
    open_main_in_b.await.unwrap();

    cx.run_until_parked();
    let weak_pane_b = pane_b.downgrade();
    let open_second_in_b = workspace
        .update(cx, |multi, window, cx| {
            multi.workspace().update(cx, |workspace, cx| {
                workspace.open_path(
                    (worktree_id, rel_path("second.rs")),
                    Some(weak_pane_b),
                    true,
                    window,
                    cx,
                )
            })
        })
        .unwrap();
    open_second_in_b.await.unwrap();

    cx.run_until_parked();
    let weak_pane_b = pane_b.downgrade();
    let reactivate_main_in_b = workspace
        .update(cx, |multi, window, cx| {
            multi.workspace().update(cx, |workspace, cx| {
                workspace.open_path(
                    (worktree_id, rel_path("main.rs")),
                    Some(weak_pane_b),
                    true,
                    window,
                    cx,
                )
            })
        })
        .unwrap();
    reactivate_main_in_b.await.unwrap();

    cx.run_until_parked();
    let weak_pane_a = pane_a.downgrade();
    let open_second = workspace
        .update(cx, |multi, window, cx| {
            multi.workspace().update(cx, |workspace, cx| {
                workspace.open_path(
                    (worktree_id, rel_path("second.rs")),
                    Some(weak_pane_a),
                    true,
                    window,
                    cx,
                )
            })
        })
        .unwrap();
    open_second.await.unwrap();

    cx.run_until_parked();
    workspace
        .read_with(cx, |_multi, cx| {
            let active = pane_a.read(cx).active_item().unwrap();
            let editor = active.to_any_view().downcast::<Editor>().unwrap();
            let path = editor.read(cx).active_project_path(cx).unwrap();
            assert_eq!(
                path.path.file_name().unwrap(),
                "second.rs",
                "Pane A should have second.rs active",
            );
        })
        .unwrap();
    workspace
        .read_with(cx, |_multi, cx| {
            let active = pane_b.read(cx).active_item().unwrap();
            let editor = active.to_any_view().downcast::<Editor>().unwrap();
            let path = editor.read(cx).active_project_path(cx).unwrap();
            assert_eq!(
                path.path.file_name().unwrap(),
                "main.rs",
                "Pane B should have main.rs active",
            );
        })
        .unwrap();
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
    workspace
        .read_with(cx, |_multi, cx| {
            let pane_a_active = pane_a.read(cx).active_item().unwrap();
            let pane_a_editor = pane_a_active.to_any_view().downcast::<Editor>().unwrap();
            let pane_a_path = pane_a_editor.read(cx).active_project_path(cx).unwrap();
            assert_eq!(
                pane_a_path.path.file_name().unwrap(),
                "second.rs",
                "Pane A should still have second.rs as active item. \
                 The debugger should not switch active tabs in panes where the \
                 breakpoint file is not the active tab (issue #40602)",
            );
        })
        .unwrap();
    workspace
        .read_with(cx, |_multi, cx| {
            let mut total_active_debug_lines = 0;
            for pane in [&pane_a, &pane_b] {
                for item in pane.read(cx).items() {
                    if let Some(editor) = item.to_any_view().downcast::<Editor>().ok() {
                        total_active_debug_lines += editor
                            .read(cx)
                            .highlighted_rows::<ActiveDebugLine>(cx)
                            .count();
                    }
                }
            }
            assert_eq!(
                total_active_debug_lines, 1,
                "There should be exactly one active debug line across all editors in all panes"
            );
        })
        .unwrap();
    workspace
        .read_with(cx, |_multi, cx| {
            let pane_b_active = pane_b.read(cx).active_item().unwrap();
            let pane_b_editor = pane_b_active.to_any_view().downcast::<Editor>().unwrap();

            let active_debug_lines: Vec<_> = pane_b_editor
                .read(cx)
                .highlighted_rows::<ActiveDebugLine>(cx)
                .collect();

            assert_eq!(
                active_debug_lines.len(),
                1,
                "Pane B's main.rs editor should have the active debug line"
            );
        })
        .unwrap();
    client.on_request::<StackTrace, _>(move |_, args| {
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
    workspace
        .read_with(cx, |_multi, cx| {
            let pane_b_active = pane_b.read(cx).active_item().unwrap();
            let pane_b_editor = pane_b_active.to_any_view().downcast::<Editor>().unwrap();
            let pane_b_path = pane_b_editor.read(cx).active_project_path(cx).unwrap();
            assert_eq!(
                pane_b_path.path.file_name().unwrap(),
                "second.rs",
                "Pane B should have switched to second.rs because it is the persistent debug pane",
            );

            let active_debug_lines: Vec<_> = pane_b_editor
                .read(cx)
                .highlighted_rows::<ActiveDebugLine>(cx)
                .collect();

            assert_eq!(
                active_debug_lines.len(),
                1,
                "Pane B's second.rs editor should have the active debug line"
            );
        })
        .unwrap();
    workspace
        .read_with(cx, |_multi, cx| {
            let mut total_active_debug_lines = 0;
            for pane in [&pane_a, &pane_b] {
                for item in pane.read(cx).items() {
                    if let Some(editor) = item.to_any_view().downcast::<Editor>().ok() {
                        total_active_debug_lines += editor
                            .read(cx)
                            .highlighted_rows::<ActiveDebugLine>(cx)
                            .count();
                    }
                }
            }
            assert_eq!(
                total_active_debug_lines, 1,
                "There should be exactly one active debug line across all editors after second stop"
            );
        })
        .unwrap();
    let pane_c = workspace
        .update(cx, |multi, window, cx| {
            multi.workspace().update(cx, |workspace, cx| {
                workspace.split_pane(pane_b.clone(), SplitDirection::Right, window, cx)
            })
        })
        .unwrap();

    cx.run_until_parked();
    workspace
        .update(cx, |_multi, window, cx| {
            move_active_item(&pane_b, &pane_c, true, false, window, cx);
        })
        .unwrap();

    cx.run_until_parked();
    workspace
        .read_with(cx, |_multi, cx| {
            let pane_c_active = pane_c.read(cx).active_item().unwrap();
            let pane_c_editor = pane_c_active.to_any_view().downcast::<Editor>().unwrap();
            let pane_c_path = pane_c_editor.read(cx).active_project_path(cx).unwrap();
            assert_eq!(
                pane_c_path.path.file_name().unwrap(),
                "second.rs",
                "Pane C should have second.rs after moving it from pane B",
            );
        })
        .unwrap();
    client.on_request::<StackTrace, _>(move |_, args| {
        assert_eq!(args.thread_id, 1);

        Ok(dap::StackTraceResponse {
            stack_frames: vec![dap::StackFrame {
                id: 3,
                name: "frame 3".into(),
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
    workspace
        .read_with(cx, |_multi, cx| {
            let pane_c_active = pane_c.read(cx).active_item().unwrap();
            let pane_c_editor = pane_c_active.to_any_view().downcast::<Editor>().unwrap();
            let pane_c_path = pane_c_editor.read(cx).active_project_path(cx).unwrap();
            assert_eq!(
                pane_c_path.path.file_name().unwrap(),
                "main.rs",
                "Pane C should have switched to main.rs because it is now the persistent debug pane \
                 (the debug line was moved here from pane B)",
            );

            let active_debug_lines: Vec<_> = pane_c_editor
                .read(cx)
                .highlighted_rows::<ActiveDebugLine>(cx)
                .collect();

            assert_eq!(
                active_debug_lines.len(),
                1,
                "Pane C's main.rs editor should have the active debug line"
            );
        })
        .unwrap();
    workspace
        .read_with(cx, |_multi, cx| {
            let mut total_active_debug_lines = 0;
            for pane in [&pane_a, &pane_b, &pane_c] {
                for item in pane.read(cx).items() {
                    if let Some(editor) = item.to_any_view().downcast::<Editor>().ok() {
                        total_active_debug_lines += editor
                            .read(cx)
                            .highlighted_rows::<ActiveDebugLine>(cx)
                            .count();
                    }
                }
            }
            assert_eq!(
                total_active_debug_lines, 1,
                "There should be exactly one active debug line across all editors after third stop"
            );
        })
        .unwrap();
    let shutdown_session = project.update(cx, |project, cx| {
        project.dap_store().update(cx, |dap_store, cx| {
            dap_store.shutdown_session(session.read(cx).session_id(), cx)
        })
    });

    shutdown_session.await.unwrap();
}
