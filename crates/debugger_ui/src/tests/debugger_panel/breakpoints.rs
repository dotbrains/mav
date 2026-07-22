use super::*;

#[gpui::test]
async fn test_send_breakpoints_when_editor_has_been_saved(
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
    let project_path = Path::new(path!("/project"));
    let worktree = project
        .update(cx, |project, cx| project.find_worktree(project_path, cx))
        .expect("This worktree should exist in project")
        .0;

    let worktree_id = workspace
        .update(cx, |_, _, cx| worktree.read(cx).id())
        .unwrap();

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    let buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    client.on_request::<Launch, _>(move |_, _| Ok(()));

    client.on_request::<StackTrace, _>(move |_, _| {
        Ok(dap::StackTraceResponse {
            stack_frames: Vec::default(),
            total_frames: None,
        })
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

    let called_set_breakpoints = Arc::new(AtomicBool::new(false));
    client.on_request::<SetBreakpoints, _>({
        let called_set_breakpoints = called_set_breakpoints.clone();
        move |_, args| {
            assert_eq!(path!("/project/main.rs"), args.source.path.unwrap());
            assert_eq!(
                vec![SourceBreakpoint {
                    line: 2,
                    column: None,
                    condition: None,
                    hit_condition: None,
                    log_message: None,
                    mode: None
                }],
                args.breakpoints.unwrap()
            );
            assert!(!args.source_modified.unwrap());

            called_set_breakpoints.store(true, Ordering::SeqCst);

            Ok(dap::SetBreakpointsResponse {
                breakpoints: Vec::default(),
            })
        }
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    cx.run_until_parked();

    assert!(
        called_set_breakpoints.load(std::sync::atomic::Ordering::SeqCst),
        "SetBreakpoint request must be called"
    );

    let called_set_breakpoints = Arc::new(AtomicBool::new(false));
    client.on_request::<SetBreakpoints, _>({
        let called_set_breakpoints = called_set_breakpoints.clone();
        move |_, args| {
            assert_eq!(path!("/project/main.rs"), args.source.path.unwrap());
            assert_eq!(
                vec![SourceBreakpoint {
                    line: 3,
                    column: None,
                    condition: None,
                    hit_condition: None,
                    log_message: None,
                    mode: None
                }],
                args.breakpoints.unwrap()
            );
            assert!(args.source_modified.unwrap());

            called_set_breakpoints.store(true, Ordering::SeqCst);

            Ok(dap::SetBreakpointsResponse {
                breakpoints: Vec::default(),
            })
        }
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.move_up(&mav_actions::editor::MoveUp, window, cx);
        editor.insert("new text\n", window, cx);
    });

    editor
        .update_in(cx, |editor, window, cx| {
            editor.save(
                SaveOptions {
                    format: true,
                    force_format: false,
                    autosave: false,
                },
                project.clone(),
                window,
                cx,
            )
        })
        .await
        .unwrap();

    cx.run_until_parked();

    assert!(
        called_set_breakpoints.load(std::sync::atomic::Ordering::SeqCst),
        "SetBreakpoint request must be called after editor is saved"
    );
}

#[gpui::test]
async fn test_unsetting_breakpoints_on_clear_breakpoint_action(
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
            "no_breakpoints.rs": "Used to ensure that we don't unset breakpoint in files with no breakpoints"
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

    let first = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let second = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("second.rs")), cx)
        })
        .await
        .unwrap();

    let (first_editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(first, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    let (second_editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(second, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    first_editor.update_in(cx, |editor, window, cx| {
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    second_editor.update_in(cx, |editor, window, cx| {
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.move_down(&mav_actions::editor::MoveDown, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let session = start_debug_session(&workspace, cx, |_| {}).unwrap();
    let client = session.update(cx, |session, _| session.adapter_client().unwrap());

    let called_set_breakpoints = Arc::new(AtomicBool::new(false));

    client.on_request::<SetBreakpoints, _>({
        move |_, args| {
            assert!(
                args.breakpoints.is_none_or(|bps| bps.is_empty()),
                "Send empty breakpoint sets to clear them from DAP servers"
            );

            match args
                .source
                .path
                .expect("We should always send a breakpoint's path")
                .as_str()
            {
                path!("/project/main.rs") | path!("/project/second.rs") => {}
                _ => {
                    panic!("Unset breakpoints for path that doesn't have any")
                }
            }

            called_set_breakpoints.store(true, Ordering::SeqCst);

            Ok(dap::SetBreakpointsResponse {
                breakpoints: Vec::default(),
            })
        }
    });

    cx.dispatch_action(crate::ClearAllBreakpoints);
    cx.run_until_parked();
}
