use super::*;

#[gpui::test]
async fn test_rust_inline_values(executor: BackgroundExecutor, cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(executor.clone());
    let source_code = include_str!("fixtures/rust/source.rs").unindent();
    fs.insert_tree(path!("/project"), json!({ "main.rs": source_code }))
        .await;

    let project = Project::test(fs.clone(), [path!("/project").as_ref()], cx).await;
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

    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(4)],
            total_frames: None,
        })
    });

    client.on_request::<dap::requests::Evaluate, _>(move |_, args| {
        assert_eq!("GLOBAL", args.expression);
        Ok(dap::EvaluateResponse {
            result: "1".into(),
            type_: None,
            presentation_hint: None,
            variables_reference: 0,
            named_variables: None,
            indexed_variables: None,
            memory_reference: None,
            value_location_reference: None,
        })
    });

    let local_variables = vec![
        variable("x", "10"),
        variable("y", "4"),
        variable("value", "42"),
    ];

    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });

    client.on_request::<dap::requests::Scopes, _>(move |_, _| {
        Ok(dap::ScopesResponse {
            scopes: vec![Scope {
                name: "Locale".into(),
                presentation_hint: None,
                variables_reference: 2,
                named_variables: None,
                indexed_variables: None,
                expensive: false,
                source: None,
                line: None,
                column: None,
                end_line: None,
                end_column: None,
            }],
        })
    });

    emit_stopped(&client).await;

    cx.run_until_parked();

    let project_path = Path::new(path!("/project"));
    let worktree = project
        .update(cx, |project, cx| project.find_worktree(project_path, cx))
        .expect("This worktree should exist in project")
        .0;

    let worktree_id = workspace
        .update(cx, |_, _, cx| worktree.read(cx).id())
        .unwrap();

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    buffer.update(cx, |buffer, cx| {
        buffer.set_language(Some(rust_lang()), cx);
    });

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(buffer, cx),
            Some(project),
            window,
            cx,
        )
    });

    active_debug_session_panel(workspace, cx).update_in(cx, |_, window, cx| {
        cx.focus_self(window);
    });
    cx.run_until_parked();

    editor.update(cx, |editor, cx| editor.refresh_inline_values(cx));

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_1.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(5)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_2.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(6)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_3.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(7)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_4.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(8)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_5.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    let local_variables = vec![
        variable("x", "10"),
        variable("y", "10"),
        variable("value", "42"),
    ];

    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });
    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(9)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_6.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    let local_variables = vec![
        variable("x", "10"),
        variable("y", "5"),
        variable("value", "42"),
    ];

    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });
    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(10)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_7.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    let local_variables = vec![
        variable("x", "10"),
        variable("y", "5"),
        variable("value", "42"),
        variable("b", "3"),
    ];
    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });
    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(11)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_8.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    let local_variables = vec![
        variable("x", "10"),
        variable("y", "4"),
        variable("value", "42"),
        variable("tester", "size=3"),
    ];
    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });
    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(14)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_9.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    let local_variables = vec![
        variable("x", "10"),
        variable("y", "4"),
        variable("value", "42"),
        variable("tester", "size=3"),
        variable("caller", "<not available>"),
    ];
    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });
    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(19)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_10.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(15)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_11.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    let local_variables = vec![variable("x", "3")];
    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });
    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(16)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_12.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    let local_variables = vec![
        variable("x", "10"),
        variable("y", "4"),
        variable("value", "42"),
        variable("tester", "size=3"),
        variable("caller", "<not available>"),
    ];
    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });
    client.on_request::<dap::requests::Evaluate, _>(move |_, args| {
        assert_eq!("GLOBAL", args.expression);
        Ok(dap::EvaluateResponse {
            result: "2".into(),
            type_: None,
            presentation_hint: None,
            variables_reference: 0,
            named_variables: None,
            indexed_variables: None,
            memory_reference: None,
            value_location_reference: None,
        })
    });
    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(25)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_13.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });

    let local_variables = vec![
        variable("x", "10"),
        variable("y", "4"),
        variable("value", "42"),
        variable("tester", "size=3"),
        variable("caller", "<not available>"),
        variable("result", "840"),
    ];
    client.on_request::<Variables, _>({
        let local_variables = Arc::new(local_variables.clone());
        move |_, _| {
            Ok(dap::VariablesResponse {
                variables: (*local_variables).clone(),
            })
        }
    });
    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![main_rs_stack_frame_for_line(26)],
            total_frames: None,
        })
    });
    emit_stopped(&client).await;

    cx.run_until_parked();

    editor.update_in(cx, |editor, window, cx| {
        pretty_assertions::assert_eq!(
            include_str!("fixtures/rust/expected_14.rs").unindent(),
            editor.snapshot(window, cx).text()
        );
    });
}
