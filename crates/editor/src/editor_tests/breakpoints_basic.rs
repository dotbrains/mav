use super::*;

#[gpui::test]
async fn test_breakpoint_toggling(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text = "First line\nSecond line\nThird line\nFourth line".to_string();
    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let worktree_id = workspace.update_in(cx, |workspace, _window, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

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

    let project_path = editor.update(cx, |editor, cx| editor.active_project_path(cx).unwrap());
    let abs_path = project.read_with(cx, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

    // assert we can add breakpoint on the first line
    editor.update_in(cx, |editor, window, cx| {
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints.len());
    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (3, Breakpoint::new_standard()),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_beginning(&MoveToBeginning, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(1, breakpoints.len());
    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![(3, Breakpoint::new_standard())],
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_eq!(0, breakpoints.len());
    assert_breakpoint(&breakpoints, &abs_path, vec![]);
}

#[gpui::test]
async fn test_breakpoint_after_save_as_existing_path(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "First line\nSecond line\nThird line\nFourth line",
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace =
        multi_workspace.read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone());

    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let first_buffer = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (first_editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(first_buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    first_editor.update_in(cx, |editor, window, cx| {
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let replacement_buffer = project.update(cx, |project, cx| {
        project.create_local_buffer("Alpha\nBeta\nGamma", None, true, cx)
    });
    project
        .update(cx, |project, cx| {
            project.save_buffer_as(
                replacement_buffer.clone(),
                ProjectPath {
                    worktree_id,
                    path: rel_path("main.rs").into(),
                },
                cx,
            )
        })
        .await
        .unwrap();

    let (replacement_editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            MultiBuffer::build_from_buffer(replacement_buffer, cx),
            Some(project.clone()),
            window,
            cx,
        )
    });

    replacement_editor.update_in(cx, |editor, window, cx| {
        editor.move_down(&MoveDown, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
    });

    let project_path =
        first_editor.update(cx, |editor, cx| editor.active_project_path(cx).unwrap());
    let abs_path = project.read_with(cx, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

    let breakpoints = first_editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .source_breakpoints_from_path(&abs_path, cx)
    });

    assert_eq!(
        vec![0, 1],
        breakpoints
            .into_iter()
            .map(|breakpoint| breakpoint.row)
            .collect::<Vec<_>>()
    );
}

#[gpui::test]
async fn test_log_breakpoint_editing(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let sample_text = "First line\nSecond line\nThird line\nFourth line".to_string();

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": sample_text,
        }),
    )
    .await;
    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

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

    let project_path = editor.update(cx, |editor, cx| editor.active_project_path(cx).unwrap());
    let abs_path = project.read_with(cx, |project, cx| {
        project
            .absolute_path(&project_path, cx)
            .map(Arc::from)
            .unwrap()
    });

    editor.update_in(cx, |editor, window, cx| {
        add_log_breakpoint_at_cursor(editor, "hello world", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![(0, Breakpoint::new_log("hello world"))],
    );

    // Removing a log message from a log breakpoint should remove it
    editor.update_in(cx, |editor, window, cx| {
        add_log_breakpoint_at_cursor(editor, "", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(&breakpoints, &abs_path, vec![]);

    editor.update_in(cx, |editor, window, cx| {
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.toggle_breakpoint(&actions::ToggleBreakpoint, window, cx);
        // Not adding a log message to a standard breakpoint shouldn't remove it
        add_log_breakpoint_at_cursor(editor, "", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (3, Breakpoint::new_standard()),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        add_log_breakpoint_at_cursor(editor, "hello world", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (3, Breakpoint::new_log("hello world")),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        add_log_breakpoint_at_cursor(editor, "hello Earth!!", window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, Breakpoint::new_standard()),
            (3, Breakpoint::new_log("hello Earth!!")),
        ],
    );
}
