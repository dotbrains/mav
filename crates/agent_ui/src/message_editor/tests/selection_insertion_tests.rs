use super::*;

#[gpui::test]
async fn test_autoscroll_after_insert_selections(cx: &mut TestAppContext) {
    init_test(cx);

    let app_state = cx.update(AppState::test);

    cx.update(|cx| {
        editor::init(cx);
        workspace::init(app_state.clone(), cx);
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/dir"),
            json!({
                "test.txt": "line1\nline2\nline3\nline4\nline5\n",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());

    let mut cx = VisualTestContext::from_window(window.into(), cx);

    // Open a regular editor with the created file, and select a portion of
    // the text that will be used for the selections that are meant to be
    // inserted in the agent panel.
    let editor = workspace
        .update_in(&mut cx, |workspace, window, cx| {
            workspace.open_path(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test.txt").into(),
                },
                None,
                false,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges([Point::new(0, 0)..Point::new(0, 5)]);
        });
    });

    let thread_store = Some(cx.new(|cx| ThreadStore::new(cx)));

    // Create a new `MessageEditor`. The `EditorMode::full()` has to be used
    // to ensure we have a fixed viewport, so we can eventually actually
    // place the cursor outside of the visible area.
    let message_editor = workspace.update_in(&mut cx, |workspace, window, cx| {
        let workspace_handle = cx.weak_entity();
        let message_editor = cx.new(|cx| {
            MessageEditor::new(
                workspace_handle,
                project.downgrade(),
                thread_store.clone(),
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::full(),
                window,
                cx,
            )
        });
        workspace.active_pane().update(cx, |pane, cx| {
            pane.add_item(
                Box::new(cx.new(|_| MessageEditorItem(message_editor.clone()))),
                true,
                true,
                None,
                window,
                cx,
            );
        });

        message_editor
    });

    message_editor.update_in(&mut cx, |message_editor, window, cx| {
        message_editor.editor.update(cx, |editor, cx| {
            // Update the Agent Panel's Message Editor text to have 100
            // lines, ensuring that the cursor is set at line 90 and that we
            // then scroll all the way to the top, so the cursor's position
            // remains off screen.
            let mut lines = String::new();
            for _ in 1..=100 {
                lines.push_str(&"Another line in the agent panel's message editor\n");
            }
            editor.set_text(lines.as_str(), window, cx);
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections.select_ranges([Point::new(90, 0)..Point::new(90, 0)]);
            });
            editor.set_scroll_position(gpui::Point::new(0., 0.), window, cx);
        });
    });

    cx.run_until_parked();

    // Before proceeding, let's assert that the cursor is indeed off screen,
    // otherwise the rest of the test doesn't make sense.
    message_editor.update_in(&mut cx, |message_editor, window, cx| {
        message_editor.editor.update(cx, |editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            let cursor_row = editor.selections.newest::<Point>(&snapshot).head().row;
            let scroll_top = snapshot.scroll_position().y as u32;
            let visible_lines = editor.visible_line_count().unwrap() as u32;
            let visible_range = scroll_top..(scroll_top + visible_lines);

            assert!(!visible_range.contains(&cursor_row));
        })
    });

    let text_editor_selection = editor.update(&mut cx, |editor, cx| {
        let multibuffer = editor.buffer().read(cx);
        let buffer = multibuffer.as_singleton().unwrap();
        let buffer_snapshot = buffer.read(cx).snapshot();
        let start = buffer_snapshot.anchor_before(0);
        let end = buffer_snapshot.anchor_after(5);
        AgentContextSelection::Editor(vec![(buffer, start..end)])
    });

    message_editor.update_in(&mut cx, |message_editor, window, cx| {
        message_editor.insert_selections(text_editor_selection, window, cx);
    });

    cx.run_until_parked();

    message_editor.update_in(&mut cx, |message_editor, window, cx| {
        message_editor.editor.update(cx, |editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            let cursor_row = editor.selections.newest::<Point>(&snapshot).head().row;
            let scroll_top = snapshot.scroll_position().y as u32;
            let visible_lines = editor.visible_line_count().unwrap() as u32;
            let visible_range = scroll_top..(scroll_top + visible_lines);

            assert!(visible_range.contains(&cursor_row));
        })
    });
}

#[gpui::test]
async fn test_insert_context_with_multibyte_characters(cx: &mut TestAppContext) {
    init_test(cx);

    let app_state = cx.update(AppState::test);

    cx.update(|cx| {
        editor::init(cx);
        workspace::init(app_state.clone(), cx);
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/dir"), json!({}))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let mut cx = VisualTestContext::from_window(window.into(), cx);

    let thread_store = cx.new(|cx| ThreadStore::new(cx));

    let (message_editor, editor) = workspace.update_in(&mut cx, |workspace, window, cx| {
        let workspace_handle = cx.weak_entity();
        let message_editor = cx.new(|cx| {
            MessageEditor::new(
                workspace_handle,
                project.downgrade(),
                Some(thread_store.clone()),
                Default::default(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    max_lines: None,
                    min_lines: 1,
                },
                window,
                cx,
            )
        });
        workspace.active_pane().update(cx, |pane, cx| {
            pane.add_item(
                Box::new(cx.new(|_| MessageEditorItem(message_editor.clone()))),
                true,
                true,
                None,
                window,
                cx,
            );
        });
        message_editor.read(cx).focus_handle(cx).focus(window, cx);
        let editor = message_editor.read(cx).editor().clone();
        (message_editor, editor)
    });

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.set_text("😄😄", window, cx);
    });

    cx.run_until_parked();

    message_editor.update_in(&mut cx, |message_editor, window, cx| {
        message_editor.insert_context_type("file", window, cx);
    });

    cx.run_until_parked();

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(editor.text(cx), "😄😄@file");
    });
}
