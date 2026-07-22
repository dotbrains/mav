use super::*;

pub(super) fn variable(name: &str, value: &str) -> Variable {
    Variable {
        name: name.into(),
        value: value.into(),
        type_: None,
        presentation_hint: None,
        evaluate_name: None,
        variables_reference: 0,
        named_variables: None,
        indexed_variables: None,
        memory_reference: None,
        declaration_location_reference: None,
        value_location_reference: None,
    }
}

pub(super) async fn emit_stopped(client: &dap::client::DebugAdapterClient) {
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
}

pub(super) fn main_rs_stack_frame_for_line(line: u64) -> dap::StackFrame {
    StackFrame {
        id: 1,
        name: "Stack Frame 1".into(),
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
        line,
        column: 1,
        end_line: None,
        end_column: None,
        can_restart: None,
        instruction_pointer_reference: None,
        module_id: None,
        presentation_hint: None,
    }
}

pub(super) fn python_lang() -> Language {
    let debug_variables_query = include_str!("../../../grammars/src/python/debugger.scm");
    Language::new(
        LanguageConfig {
            name: "Python".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["py".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_python::LANGUAGE.into()),
    )
    .with_debug_variables_query(debug_variables_query)
    .unwrap()
}

pub(super) fn go_lang() -> Arc<Language> {
    let debug_variables_query = include_str!("../../../grammars/src/go/debugger.scm");
    Arc::new(
        Language::new(
            LanguageConfig {
                name: "Go".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["go".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_go::LANGUAGE.into()),
        )
        .with_debug_variables_query(debug_variables_query)
        .unwrap(),
    )
}

/// Test utility function for inline values testing
///
/// # Arguments
/// * `variables` - List of tuples containing (variable_name, variable_value)
/// * `before` - Source code before inline values are applied
/// * `after` - Expected source code after inline values are applied
/// * `language` - Language configuration to use for parsing
/// * `executor` - Background executor for async operations
/// * `cx` - Test app context
pub(super) async fn test_inline_values_util(
    local_variables: &[(&str, &str)],
    global_variables: &[(&str, &str)],
    before: &str,
    after: &str,
    active_debug_line: Option<usize>,
    language: Arc<Language>,
    executor: BackgroundExecutor,
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let lines_count = before.lines().count();
    let stop_line =
        active_debug_line.unwrap_or_else(|| if lines_count > 6 { 6 } else { lines_count - 1 });

    let fs = FakeFs::new(executor.clone());
    fs.insert_tree(path!("/project"), json!({ "main.rs": before.to_string() }))
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

    client.on_request::<dap::requests::Threads, _>(|_, _| {
        Ok(dap::ThreadsResponse {
            threads: vec![dap::Thread {
                id: 1,
                name: "main".into(),
            }],
        })
    });

    client.on_request::<dap::requests::StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: vec![dap::StackFrame {
                id: 1,
                name: "main".into(),
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
                line: stop_line as u64,
                column: 1,
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

    let local_vars: Vec<Variable> = local_variables
        .iter()
        .map(|(name, value)| Variable {
            name: (*name).into(),
            value: (*value).into(),
            type_: None,
            presentation_hint: None,
            evaluate_name: None,
            variables_reference: 0,
            named_variables: None,
            indexed_variables: None,
            memory_reference: None,
            declaration_location_reference: None,
            value_location_reference: None,
        })
        .collect();

    let global_vars: Vec<Variable> = global_variables
        .iter()
        .map(|(name, value)| Variable {
            name: (*name).into(),
            value: (*value).into(),
            type_: None,
            presentation_hint: None,
            evaluate_name: None,
            variables_reference: 0,
            named_variables: None,
            indexed_variables: None,
            memory_reference: None,
            declaration_location_reference: None,
            value_location_reference: None,
        })
        .collect();

    client.on_request::<Variables, _>({
        let local_vars = Arc::new(local_vars.clone());
        let global_vars = Arc::new(global_vars.clone());
        move |_, args| {
            let variables = match args.variables_reference {
                2 => (*local_vars).clone(),
                3 => (*global_vars).clone(),
                _ => vec![],
            };
            Ok(dap::VariablesResponse { variables })
        }
    });

    client.on_request::<dap::requests::Scopes, _>(move |_, _| {
        Ok(dap::ScopesResponse {
            scopes: vec![
                Scope {
                    name: "Local".into(),
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
                },
                Scope {
                    name: "Global".into(),
                    presentation_hint: None,
                    variables_reference: 3,
                    named_variables: None,
                    indexed_variables: None,
                    expensive: false,
                    source: None,
                    line: None,
                    column: None,
                    end_line: None,
                    end_column: None,
                },
            ],
        })
    });

    if !global_variables.is_empty() {
        let global_evaluate_map: std::collections::HashMap<String, String> = global_variables
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();

        client.on_request::<dap::requests::Evaluate, _>(move |_, args| {
            let value = global_evaluate_map
                .get(&args.expression)
                .unwrap_or(&"undefined".to_string())
                .clone();

            Ok(dap::EvaluateResponse {
                result: value,
                type_: None,
                presentation_hint: None,
                variables_reference: 0,
                named_variables: None,
                indexed_variables: None,
                memory_reference: None,
                value_location_reference: None,
            })
        });
    }

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
        buffer.set_language(Some(language), cx);
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
        pretty_assertions::assert_eq!(after, editor.snapshot(window, cx).text());
    });
}
