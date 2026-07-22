use super::*;

#[gpui::test]
async fn test_it_fetches_scopes_variables_when_you_select_a_stack_frame(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
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
    workspace
        .update(cx, |workspace, window, cx| {
            workspace.focus_panel::<DebugPanel>(window, cx);
        })
        .unwrap();
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

    client.on_request::<Initialize, _>(move |_, _| {
        Ok(dap::Capabilities {
            supports_step_back: Some(false),
            ..Default::default()
        })
    });

    client.on_request::<Launch, _>(move |_, _| Ok(()));

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

    let frame_1_scopes = vec![scope("Frame 1 Scope 1", 2)];

    // add handlers for fetching the second stack frame's scopes and variables
    // after the user clicked the stack frame
    let frame_2_scopes = vec![scope("Frame 2 Scope 1", 3)];

    let called_second_stack_frame = Arc::new(AtomicBool::new(false));
    let called_first_stack_frame = Arc::new(AtomicBool::new(false));

    client.on_request::<Scopes, _>({
        let frame_1_scopes = Arc::new(frame_1_scopes.clone());
        let frame_2_scopes = Arc::new(frame_2_scopes.clone());
        let called_first_stack_frame = called_first_stack_frame.clone();
        let called_second_stack_frame = called_second_stack_frame.clone();
        move |_, args| match args.frame_id {
            1 => {
                called_first_stack_frame.store(true, Ordering::SeqCst);
                Ok(dap::ScopesResponse {
                    scopes: (*frame_1_scopes).clone(),
                })
            }
            2 => {
                called_second_stack_frame.store(true, Ordering::SeqCst);

                Ok(dap::ScopesResponse {
                    scopes: (*frame_2_scopes).clone(),
                })
            }
            _ => panic!("Made a scopes request with an invalid frame id"),
        }
    });

    let frame_1_variables = vec![
        simple_variable("variable1", "value 1"),
        simple_variable("variable2", "value 2"),
    ];

    let frame_2_variables = vec![
        simple_variable("variable3", "old value 1"),
        simple_variable("variable4", "old value 2"),
    ];

    client.on_request::<Variables, _>({
        let frame_1_variables = Arc::new(frame_1_variables.clone());
        move |_, args| {
            assert_eq!(2, args.variables_reference);

            Ok(dap::VariablesResponse {
                variables: (*frame_1_variables).clone(),
            })
        }
    });

    emit_stopped(&client).await;

    cx.run_until_parked();

    let running_state =
        active_debug_session_panel(workspace, cx).update_in(cx, |item, window, cx| {
            cx.focus_self(window);
            item.running_state().clone()
        });

    running_state.update(cx, |running_state, cx| {
        let (stack_frame_list, stack_frame_id) =
            running_state.stack_frame_list().update(cx, |list, _| {
                (
                    list.flatten_entries(true, true),
                    list.opened_stack_frame_id(),
                )
            });

        let variable_list = running_state.variable_list().read(cx);
        let variables = variable_list.variables();

        assert_eq!(Some(1), stack_frame_id);
        assert_eq!(
            running_state
                .stack_frame_list()
                .read(cx)
                .opened_stack_frame_id(),
            Some(1)
        );

        assert!(
            called_first_stack_frame.load(std::sync::atomic::Ordering::SeqCst),
            "Request scopes shouldn't be called before it's needed"
        );
        assert!(
            !called_second_stack_frame.load(std::sync::atomic::Ordering::SeqCst),
            "Request scopes shouldn't be called before it's needed"
        );

        assert_eq!(stack_frames, stack_frame_list);
        assert_eq!(frame_1_variables, variables);
    });

    client.on_request::<Variables, _>({
        let frame_2_variables = Arc::new(frame_2_variables.clone());
        move |_, args| {
            assert_eq!(3, args.variables_reference);

            Ok(dap::VariablesResponse {
                variables: (*frame_2_variables).clone(),
            })
        }
    });

    running_state
        .update_in(cx, |running_state, window, cx| {
            running_state
                .stack_frame_list()
                .update(cx, |stack_frame_list, cx| {
                    stack_frame_list.go_to_stack_frame(stack_frames[1].id, window, cx)
                })
        })
        .await
        .unwrap();

    cx.run_until_parked();

    running_state.update(cx, |running_state, cx| {
        let (stack_frame_list, stack_frame_id) =
            running_state.stack_frame_list().update(cx, |list, _| {
                (
                    list.flatten_entries(true, true),
                    list.opened_stack_frame_id(),
                )
            });

        let variable_list = running_state.variable_list().read(cx);
        let variables = variable_list.variables();

        assert_eq!(Some(2), stack_frame_id);
        assert!(
            called_second_stack_frame.load(std::sync::atomic::Ordering::SeqCst),
            "Request scopes shouldn't be called before it's needed"
        );

        assert_eq!(stack_frames, stack_frame_list);

        assert_eq!(variables, frame_2_variables,);
    });
}
