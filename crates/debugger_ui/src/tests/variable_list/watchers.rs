use super::*;

#[gpui::test]
async fn test_add_and_remove_watcher(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let test_file_content = r#"
        const variable1 = "Value 1";
        const variable2 = "Value 2";
    "#
    .unindent();

    fs.insert_tree(
        path!("/project"),
        json!({
           "src": {
               "test.js": test_file_content,
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

    let stack_frames = vec![test_js_stack_frame()];

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

    let scopes = vec![scope("Scope 1", 2)];

    client.on_request::<Scopes, _>({
        let scopes = Arc::new(scopes.clone());
        move |_, args| {
            assert_eq!(1, args.frame_id);

            Ok(dap::ScopesResponse {
                scopes: (*scopes).clone(),
            })
        }
    });

    let variables = vec![
        simple_variable("variable1", "value 1"),
        simple_variable("variable2", "value 2"),
    ];

    client.on_request::<Variables, _>({
        let variables = Arc::new(variables.clone());
        move |_, args| {
            assert_eq!(2, args.variables_reference);

            Ok(dap::VariablesResponse {
                variables: (*variables).clone(),
            })
        }
    });

    client.on_request::<Evaluate, _>({
        move |_, args| {
            assert_eq!("variable1", args.expression);

            Ok(dap::EvaluateResponse {
                result: "value1".to_owned(),
                type_: None,
                presentation_hint: None,
                variables_reference: 2,
                named_variables: None,
                indexed_variables: None,
                memory_reference: None,
                value_location_reference: None,
            })
        }
    });

    emit_stopped(&client).await;

    cx.run_until_parked();

    let running_state =
        active_debug_session_panel(workspace, cx).update_in(cx, |item, window, cx| {
            cx.focus_self(window);
            let running = item.running_state().clone();

            let variable_list = running.update(cx, |state, cx| {
                // have to do this because the variable list pane should be shown/active
                // for testing the variable list
                state.activate_item(DebuggerPaneItem::Variables, window, cx);

                state.variable_list().clone()
            });
            variable_list.update(cx, |_, cx| cx.focus_self(window));
            running
        });
    cx.run_until_parked();

    // select variable 1 from first scope
    running_state.update(cx, |running_state, cx| {
        running_state.variable_list().update(cx, |_, cx| {
            cx.dispatch_action(&SelectFirst);
            cx.dispatch_action(&SelectNext);
        });
    });
    cx.run_until_parked();

    running_state.update(cx, |running_state, cx| {
        running_state.variable_list().update(cx, |_, cx| {
            cx.dispatch_action(&AddWatch);
        });
    });
    cx.run_until_parked();

    // assert watcher for variable1 was added
    running_state.update(cx, |running_state, cx| {
        running_state.variable_list().update(cx, |list, _| {
            list.assert_visual_entries(vec![
                "> variable1",
                "v Scope 1",
                "    > variable1 <=== selected",
                "    > variable2",
            ]);
        });
    });

    session.update(cx, |session, _| {
        let watcher = session
            .watchers()
            .get(&SharedString::from("variable1"))
            .unwrap();

        assert_eq!("value1", watcher.value.to_string());
        assert_eq!("variable1", watcher.expression.to_string());
        assert_eq!(2, watcher.variables_reference);
    });

    // select added watcher for variable1
    running_state.update(cx, |running_state, cx| {
        running_state.variable_list().update(cx, |_, cx| {
            cx.dispatch_action(&SelectFirst);
        });
    });
    cx.run_until_parked();

    running_state.update(cx, |running_state, cx| {
        running_state.variable_list().update(cx, |_, cx| {
            cx.dispatch_action(&RemoveWatch);
        });
    });
    cx.run_until_parked();

    // assert watcher for variable1 was removed
    running_state.update(cx, |running_state, cx| {
        running_state.variable_list().update(cx, |list, _| {
            list.assert_visual_entries(vec!["v Scope 1", "    > variable1", "    > variable2"]);
        });
    });
}

#[gpui::test]
async fn test_refresh_watchers(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let test_file_content = r#"
        const variable1 = "Value 1";
        const variable2 = "Value 2";
    "#
    .unindent();

    fs.insert_tree(
        path!("/project"),
        json!({
           "src": {
               "test.js": test_file_content,
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

    let stack_frames = vec![test_js_stack_frame()];

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

    let scopes = vec![scope("Scope 1", 2)];

    client.on_request::<Scopes, _>({
        let scopes = Arc::new(scopes.clone());
        move |_, args| {
            assert_eq!(1, args.frame_id);

            Ok(dap::ScopesResponse {
                scopes: (*scopes).clone(),
            })
        }
    });

    let variables = vec![
        simple_variable("variable1", "value 1"),
        simple_variable("variable2", "value 2"),
    ];

    client.on_request::<Variables, _>({
        let variables = Arc::new(variables.clone());
        move |_, args| {
            assert_eq!(2, args.variables_reference);

            Ok(dap::VariablesResponse {
                variables: (*variables).clone(),
            })
        }
    });

    client.on_request::<Evaluate, _>({
        move |_, args| {
            assert_eq!("variable1", args.expression);

            Ok(dap::EvaluateResponse {
                result: "value1".to_owned(),
                type_: None,
                presentation_hint: None,
                variables_reference: 2,
                named_variables: None,
                indexed_variables: None,
                memory_reference: None,
                value_location_reference: None,
            })
        }
    });

    emit_stopped(&client).await;

    cx.run_until_parked();

    let running_state =
        active_debug_session_panel(workspace, cx).update_in(cx, |item, window, cx| {
            cx.focus_self(window);
            let running = item.running_state().clone();

            let variable_list = running.update(cx, |state, cx| {
                // have to do this because the variable list pane should be shown/active
                // for testing the variable list
                state.activate_item(DebuggerPaneItem::Variables, window, cx);

                state.variable_list().clone()
            });
            variable_list.update(cx, |_, cx| cx.focus_self(window));
            running
        });
    cx.run_until_parked();

    // select variable 1 from first scope
    running_state.update(cx, |running_state, cx| {
        running_state.variable_list().update(cx, |_, cx| {
            cx.dispatch_action(&SelectFirst);
            cx.dispatch_action(&SelectNext);
        });
    });
    cx.run_until_parked();

    running_state.update(cx, |running_state, cx| {
        running_state.variable_list().update(cx, |_, cx| {
            cx.dispatch_action(&AddWatch);
        });
    });
    cx.run_until_parked();

    session.update(cx, |session, _| {
        let watcher = session
            .watchers()
            .get(&SharedString::from("variable1"))
            .unwrap();

        assert_eq!("value1", watcher.value.to_string());
        assert_eq!("variable1", watcher.expression.to_string());
        assert_eq!(2, watcher.variables_reference);
    });

    client.on_request::<Evaluate, _>({
        move |_, args| {
            assert_eq!("variable1", args.expression);

            Ok(dap::EvaluateResponse {
                result: "value updated".to_owned(),
                type_: None,
                presentation_hint: None,
                variables_reference: 3,
                named_variables: None,
                indexed_variables: None,
                memory_reference: None,
                value_location_reference: None,
            })
        }
    });

    emit_stopped(&client).await;

    cx.run_until_parked();

    session.update(cx, |session, _| {
        let watcher = session
            .watchers()
            .get(&SharedString::from("variable1"))
            .unwrap();

        assert_eq!("value updated", watcher.value.to_string());
        assert_eq!("variable1", watcher.expression.to_string());
        assert_eq!(3, watcher.variables_reference);
    });
}
