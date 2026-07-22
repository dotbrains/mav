use super::*;

#[gpui::test]
async fn test_basic_fetch_initial_scope_and_variables(
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
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

        assert_eq!(stack_frames, stack_frame_list);
        assert_eq!(Some(1), stack_frame_id);

        running_state
            .variable_list()
            .update(cx, |variable_list, _| {
                assert_eq!(scopes, variable_list.scopes());
                assert_eq!(
                    vec![variables[0].clone(), variables[1].clone(),],
                    variable_list.variables()
                );

                variable_list.assert_visual_entries(vec![
                    "v Scope 1",
                    "    > variable1",
                    "    > variable2",
                ]);
            });
    });
}
