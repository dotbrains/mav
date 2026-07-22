use super::*;

#[gpui::test]
async fn test_collapsed_entries(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let test_file_content = r#"
        import { SOME_VALUE } './module.js';

        console.log(SOME_VALUE);
    "#
    .unindent();

    let module_file_content = r#"
        export SOME_VALUE = 'some value';
    "#
    .unindent();

    fs.insert_tree(
        path!("/project"),
        json!({
           "src": {
               "test.js": test_file_content,
               "module.js": module_file_content,
           }
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

    client.on_request::<Scopes, _>(move |_, _| Ok(dap::ScopesResponse { scopes: vec![] }));

    let stack_frames = vec![
        StackFrame {
            id: 1,
            name: "Stack Frame 1".into(),
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
            line: 3,
            column: 1,
            end_line: None,
            end_column: None,
            can_restart: None,
            instruction_pointer_reference: None,
            module_id: None,
            presentation_hint: None,
        },
        StackFrame {
            id: 2,
            name: "Stack Frame 2".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some(path!("/project/src/module.js").into()),
                source_reference: None,
                presentation_hint: None,
                origin: Some("ignored".into()),
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
            presentation_hint: Some(dap::StackFramePresentationHint::Deemphasize),
        },
        StackFrame {
            id: 3,
            name: "Stack Frame 3".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some(path!("/project/src/module.js").into()),
                source_reference: None,
                presentation_hint: None,
                origin: Some("ignored".into()),
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
            presentation_hint: Some(dap::StackFramePresentationHint::Deemphasize),
        },
        StackFrame {
            id: 4,
            name: "Stack Frame 4".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some(path!("/project/src/module.js").into()),
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
        },
        StackFrame {
            id: 5,
            name: "Stack Frame 5".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some(path!("/project/src/module.js").into()),
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
            presentation_hint: Some(dap::StackFramePresentationHint::Deemphasize),
        },
        StackFrame {
            id: 6,
            name: "Stack Frame 6".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some(path!("/project/src/module.js").into()),
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
            presentation_hint: Some(dap::StackFramePresentationHint::Deemphasize),
        },
        StackFrame {
            id: 7,
            name: "Stack Frame 7".into(),
            source: Some(dap::Source {
                name: Some("module.js".into()),
                path: Some(path!("/project/src/module.js").into()),
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
        },
    ];

    client.on_request::<StackTrace, _>({
        let stack_frames = Arc::new(stack_frames.clone());
        move |_, args| {
            assert_eq!(1, args.thread_id);

            Ok(dap::StackTraceResponse {
                stack_frames: (*stack_frames).clone(),
                total_frames: None,
            })
        }
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

    // trigger threads to load
    active_debug_session_panel(workspace, cx).update(cx, |session, cx| {
        session.running_state().update(cx, |running_state, cx| {
            running_state
                .session()
                .update(cx, |session, cx| session.threads(cx));
        });
    });

    cx.run_until_parked();

    // select first thread
    active_debug_session_panel(workspace, cx).update_in(cx, |session, window, cx| {
        session.running_state().update(cx, |running_state, cx| {
            running_state.select_current_thread(
                &running_state
                    .session()
                    .update(cx, |session, cx| session.threads(cx)),
                window,
                cx,
            );
        });
    });

    cx.run_until_parked();

    // trigger stack frames to loaded
    active_debug_session_panel(workspace, cx).update(cx, |debug_panel_item, cx| {
        let stack_frame_list = debug_panel_item
            .running_state()
            .update(cx, |state, _| state.stack_frame_list().clone());

        stack_frame_list.update(cx, |stack_frame_list, cx| {
            stack_frame_list.dap_stack_frames(cx);
        });
    });

    cx.run_until_parked();

    active_debug_session_panel(workspace, cx).update_in(cx, |debug_panel_item, window, cx| {
        let stack_frame_list = debug_panel_item
            .running_state()
            .update(cx, |state, _| state.stack_frame_list().clone());

        stack_frame_list.update(cx, |stack_frame_list, cx| {
            stack_frame_list.build_entries(true, window, cx);

            assert_eq!(
                &vec![
                    StackFrameEntry::Normal(stack_frames[0].clone()),
                    StackFrameEntry::Collapsed(vec![
                        stack_frames[1].clone(),
                        stack_frames[2].clone()
                    ]),
                    StackFrameEntry::Normal(stack_frames[3].clone()),
                    StackFrameEntry::Collapsed(vec![
                        stack_frames[4].clone(),
                        stack_frames[5].clone()
                    ]),
                    StackFrameEntry::Normal(stack_frames[6].clone()),
                ],
                stack_frame_list.entries()
            );

            stack_frame_list.expand_collapsed_entry(1, cx);

            assert_eq!(
                &vec![
                    StackFrameEntry::Normal(stack_frames[0].clone()),
                    StackFrameEntry::Normal(stack_frames[1].clone()),
                    StackFrameEntry::Normal(stack_frames[2].clone()),
                    StackFrameEntry::Normal(stack_frames[3].clone()),
                    StackFrameEntry::Collapsed(vec![
                        stack_frames[4].clone(),
                        stack_frames[5].clone()
                    ]),
                    StackFrameEntry::Normal(stack_frames[6].clone()),
                ],
                stack_frame_list.entries()
            );

            stack_frame_list.expand_collapsed_entry(4, cx);

            assert_eq!(
                &vec![
                    StackFrameEntry::Normal(stack_frames[0].clone()),
                    StackFrameEntry::Normal(stack_frames[1].clone()),
                    StackFrameEntry::Normal(stack_frames[2].clone()),
                    StackFrameEntry::Normal(stack_frames[3].clone()),
                    StackFrameEntry::Normal(stack_frames[4].clone()),
                    StackFrameEntry::Normal(stack_frames[5].clone()),
                    StackFrameEntry::Normal(stack_frames[6].clone()),
                ],
                stack_frame_list.entries()
            );
        });
    });
}
