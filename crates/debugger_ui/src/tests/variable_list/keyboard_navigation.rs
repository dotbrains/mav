use super::*;

// Tests that toggling a variable will fetch its children and shows it.
macro_rules! assert_entries {
    ($cx:expr, $running_state:expr, [$($entry:expr),* $(,)?]) => {{
        $running_state.update($cx, |debug_panel_item, cx| {
            debug_panel_item
                .variable_list()
                .update(cx, |variable_list, _| {
                    variable_list.assert_visual_entries(vec![$($entry),*]);
                });
        });
    }};
}

macro_rules! assert_entries_after {
    ($cx:expr, $running_state:expr, $action:expr, [$($entry:expr),* $(,)?]) => {{
        $cx.dispatch_action($action);
        $cx.run_until_parked();
        assert_entries!($cx, $running_state, [$($entry),*]);
    }};
}

#[gpui::test]
async fn test_keyboard_navigation(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());

    let test_file_content = r#"
        const variable1 = {
            nested1: "Nested 1",
            nested2: "Nested 2",
        };
        const variable2 = "Value 2";
        const variable3 = "Value 3";
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

    client.on_request::<Initialize, _>(move |_, _| {
        Ok(dap::Capabilities {
            supports_step_back: Some(false),
            ..Default::default()
        })
    });

    client.on_request::<Launch, _>(move |_, _| Ok(()));

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

    let scopes = vec![local_scope("Scope 1", 2), scope("Scope 2", 4)];

    client.on_request::<Scopes, _>({
        let scopes = Arc::new(scopes.clone());
        move |_, args| {
            assert_eq!(1, args.frame_id);

            Ok(dap::ScopesResponse {
                scopes: (*scopes).clone(),
            })
        }
    });

    let scope1_variables = vec![
        Variable {
            name: "variable1".into(),
            value: "{nested1: \"Nested 1\", nested2: \"Nested 2\"}".into(),
            type_: None,
            presentation_hint: None,
            evaluate_name: None,
            variables_reference: 3,
            named_variables: None,
            indexed_variables: None,
            memory_reference: None,
            declaration_location_reference: None,
            value_location_reference: None,
        },
        simple_variable("variable2", "Value 2"),
    ];

    let nested_variables = vec![
        simple_variable("nested1", "Nested 1"),
        simple_variable("nested2", "Nested 2"),
    ];

    let scope2_variables = vec![simple_variable("variable3", "Value 3")];

    client.on_request::<Variables, _>({
        let scope1_variables = Arc::new(scope1_variables.clone());
        let nested_variables = Arc::new(nested_variables.clone());
        let scope2_variables = Arc::new(scope2_variables.clone());
        move |_, args| match args.variables_reference {
            4 => Ok(dap::VariablesResponse {
                variables: (*scope2_variables).clone(),
            }),
            3 => Ok(dap::VariablesResponse {
                variables: (*nested_variables).clone(),
            }),
            2 => Ok(dap::VariablesResponse {
                variables: (*scope1_variables).clone(),
            }),
            id => unreachable!("unexpected variables reference {id}"),
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
                // for testing keyboard navigation
                state.activate_item(DebuggerPaneItem::Variables, window, cx);

                state.variable_list().clone()
            });
            variable_list.update(cx, |_, cx| cx.focus_self(window));
            running
        });
    include!("keyboard_navigation_steps.rs");
}
