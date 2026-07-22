use super::*;

/// This tests fetching multiple scopes and variables for them with a single stackframe
#[gpui::test]
async fn test_fetch_variables_for_multiple_scopes(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
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

    let scopes = vec![local_scope("Scope 1", 2), scope("Scope 2", 3)];

    client.on_request::<Scopes, _>({
        let scopes = Arc::new(scopes.clone());
        move |_, args| {
            assert_eq!(1, args.frame_id);

            Ok(dap::ScopesResponse {
                scopes: (*scopes).clone(),
            })
        }
    });

    let mut variables = HashMap::default();
    variables.insert(
        2,
        vec![
            Variable {
                name: "variable1".into(),
                value: "{nested1: \"Nested 1\", nested2: \"Nested 2\"}".into(),
                type_: None,
                presentation_hint: None,
                evaluate_name: None,
                variables_reference: 0,
                named_variables: None,
                indexed_variables: None,
                memory_reference: None,
                declaration_location_reference: None,
                value_location_reference: None,
            },
            simple_variable("variable2", "Value 2"),
        ],
    );
    variables.insert(3, vec![simple_variable("variable3", "Value 3")]);

    client.on_request::<Variables, _>({
        let variables = Arc::new(variables.clone());
        move |_, args| {
            Ok(dap::VariablesResponse {
                variables: variables.get(&args.variables_reference).unwrap().clone(),
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
    cx.run_until_parked();

    running_state.update(cx, |running_state, cx| {
        let (stack_frame_list, stack_frame_id) =
            running_state.stack_frame_list().update(cx, |list, _| {
                (
                    list.flatten_entries(true, true),
                    list.opened_stack_frame_id(),
                )
            });

        assert_eq!(Some(1), stack_frame_id);
        assert_eq!(stack_frames, stack_frame_list);

        running_state
            .variable_list()
            .update(cx, |variable_list, _| {
                assert_eq!(2, variable_list.scopes().len());
                assert_eq!(scopes, variable_list.scopes());
                let variables_by_scope = variable_list.variables_per_scope();

                // scope 1
                assert_eq!(
                    vec![
                        variables.get(&2).unwrap()[0].clone(),
                        variables.get(&2).unwrap()[1].clone(),
                    ],
                    variables_by_scope[0].1
                );

                // scope 2
                let empty_vec: Vec<dap::Variable> = vec![];
                assert_eq!(empty_vec, variables_by_scope[1].1);

                variable_list.assert_visual_entries(vec![
                    "v Scope 1",
                    "    > variable1",
                    "    > variable2",
                    "> Scope 2",
                ]);
            });
    });
}
