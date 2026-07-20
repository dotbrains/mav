use super::*;

/// This also tests that Editor::breakpoint_at_cursor_head is working properly
/// we had some issues where we wouldn't find a breakpoint at Point {row: 0, col: 0}
/// or when breakpoints were placed out of order. This tests for a regression too
#[gpui::test]
async fn test_breakpoint_enabling_and_disabling(cx: &mut TestAppContext) {
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
        editor.move_up(&MoveUp, window, cx);
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
            (2, Breakpoint::new_standard()),
            (3, Breakpoint::new_standard()),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_beginning(&MoveToBeginning, window, cx);
        editor.disable_breakpoint(&actions::DisableBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.disable_breakpoint(&actions::DisableBreakpoint, window, cx);
        // Disabling a breakpoint that doesn't exist should do nothing
        editor.move_up(&MoveUp, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.disable_breakpoint(&actions::DisableBreakpoint, window, cx);
    });

    let breakpoints = editor.update(cx, |editor, cx| {
        editor
            .breakpoint_store()
            .as_ref()
            .unwrap()
            .read(cx)
            .all_source_breakpoints(cx)
    });

    let disable_breakpoint = {
        let mut bp = Breakpoint::new_standard();
        bp.state = BreakpointState::Disabled;
        bp
    };

    assert_eq!(1, breakpoints.len());
    assert_breakpoint(
        &breakpoints,
        &abs_path,
        vec![
            (0, disable_breakpoint.clone()),
            (2, Breakpoint::new_standard()),
            (3, disable_breakpoint.clone()),
        ],
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.move_to_beginning(&MoveToBeginning, window, cx);
        editor.enable_breakpoint(&actions::EnableBreakpoint, window, cx);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.enable_breakpoint(&actions::EnableBreakpoint, window, cx);
        editor.move_up(&MoveUp, window, cx);
        editor.disable_breakpoint(&actions::DisableBreakpoint, window, cx);
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
            (2, disable_breakpoint),
            (3, Breakpoint::new_standard()),
        ],
    );
}
