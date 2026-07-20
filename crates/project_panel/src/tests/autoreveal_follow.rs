use super::*;

#[gpui::test]
async fn test_autoreveal_follows_multibuffer_selection(cx: &mut gpui::TestAppContext) {
    use editor::{
        Editor, EditorEvent, EditorMode, MultiBuffer, PathKey, SelectionEffects, ToOffset,
    };
    use language::Point;
    use multibuffer_wrapper::TestMultibufferWrapper;

    init_test_with_editor(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(true);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/project_root"),
        json!({
            "dir_1": { "file_1.py": "alpha 1\nalpha 2\nalpha 3\n" },
            "dir_2": { "file_2.py": "beta 1\nbeta 2\nbeta 3\n" },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/project_root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    let buffer_1 = project
        .update(cx, |project, cx| {
            let project_path = project
                .find_project_path("project_root/dir_1/file_1.py", cx)
                .unwrap();
            project.open_buffer(project_path, cx)
        })
        .await
        .unwrap();
    let buffer_2 = project
        .update(cx, |project, cx| {
            let project_path = project
                .find_project_path("project_root/dir_2/file_2.py", cx)
                .unwrap();
            project.open_buffer(project_path, cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.update(|_, cx| {
        cx.new(|cx| {
            let mut multi_buffer = MultiBuffer::new(language::Capability::ReadWrite);
            multi_buffer.set_excerpts_for_path(
                PathKey::sorted(0),
                buffer_1.clone(),
                [Point::new(0, 0)..Point::new(2, 0)],
                0,
                cx,
            );
            multi_buffer.set_excerpts_for_path(
                PathKey::sorted(1),
                buffer_2.clone(),
                [Point::new(0, 0)..Point::new(2, 0)],
                0,
                cx,
            );
            multi_buffer
        })
    });

    let inner_editor = cx.update(|window, cx| {
        cx.new(|cx| {
            Editor::new(
                EditorMode::full(),
                multi_buffer.clone(),
                Some(project.clone()),
                window,
                cx,
            )
        })
    });

    // Wrap the multibuffer editor in an `Item`, mirroring real multibuffer
    // views (`ProjectDiagnosticsEditor`, `ProjectDiff`, etc.). Auto-reveal
    // should follow the inner editor's active buffer.
    workspace.update_in(cx, |workspace, window, cx| {
        let wrapper = cx.new(|cx| TestMultibufferWrapper::new(inner_editor.clone(), cx));
        workspace.add_item_to_active_pane(Box::new(wrapper), None, true, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    v dir_1",
            "          file_1.py  <== selected  <== marked",
            "    > dir_2",
        ],
        "When a multibuffer becomes active, its first excerpt's file should be revealed"
    );

    let buffer_2_offset = multi_buffer.read_with(cx, |multi_buffer, cx| {
        let snapshot = multi_buffer.snapshot(cx);
        let buffer_2_id = buffer_2.read(cx).remote_id();
        let excerpt = snapshot
            .excerpts_for_buffer(buffer_2_id)
            .next()
            .expect("buffer_2 excerpt must exist");
        snapshot
            .anchor_in_excerpt(excerpt.context.start)
            .expect("excerpt anchor must resolve")
            .to_offset(&snapshot)
    });

    inner_editor.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([buffer_2_offset..buffer_2_offset]);
        });
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_2.py  <== selected  <== marked",
        ],
        "Moving the cursor into a different excerpt buffer should reveal that buffer's entry"
    );

    // Wrappers re-emit inner-editor events through `to_item_events`, so a
    // benign `TitleChanged` (e.g. diagnostic summary updates) ultimately
    // reaches `Workspace::active_item_path_changed`. The active path should be
    // recomputed from the wrapper instead of falling back to a stale selection.
    inner_editor.update(cx, |_, cx| cx.emit(EditorEvent::TitleChanged));
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_2.py  <== selected  <== marked",
        ],
        "Wrapper-level title updates must not clobber the inner editor's reveal"
    );
}

#[gpui::test]
async fn test_reveal_in_project_panel_fallback(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/workspace",
        json!({
            "README.md": ""
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/workspace".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    // Project panel should still be activated and focused, when using `pane:
    // reveal in project panel` without an active item.
    cx.dispatch_action(workspace::RevealInProjectPanel::default());
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel
            .workspace
            .update(cx, |workspace, cx| {
                assert!(
                    workspace.active_item(cx).is_none(),
                    "Workspace should not have an active item."
                );
            })
            .unwrap();

        assert!(
            panel.focus_handle(cx).is_focused(window),
            "Project panel should be focused, even when there's no active item."
        );
    });

    // When working with a file that doesn't belong to an open project, we
    // should still activate the project panel on `pane: reveal in project
    // panel`.
    fs.insert_tree(
        "/external",
        json!({
            "file.txt": "External File",
        }),
    )
    .await;

    let (worktree, _) = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree("/external/file.txt", false, cx)
        })
        .await
        .unwrap();

    workspace
        .update_in(cx, |workspace, window, cx| {
            let worktree_id = worktree.read(cx).id();
            let path = rel_path("").into();
            let project_path = ProjectPath { worktree_id, path };

            workspace.open_path(project_path, None, true, window, cx)
        })
        .await
        .unwrap();
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.focus_handle(cx).is_focused(window),
            "Project panel should not be focused after opening an external file."
        );
    });

    cx.dispatch_action(workspace::RevealInProjectPanel::default());
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel
            .workspace
            .update(cx, |workspace, cx| {
                assert!(
                    workspace.active_item(cx).is_some(),
                    "Workspace should have an active item."
                );
            })
            .unwrap();

        assert!(
            panel.focus_handle(cx).is_focused(window),
            "Project panel should be focused even for invisible worktree entry."
        );
    });

    // Focus again on the center pane so we're sure that the focus doesn't
    // remain on the project panel, otherwise later assertions wouldn't matter.
    panel.update_in(cx, |panel, window, cx| {
        panel
            .workspace
            .update(cx, |workspace, cx| {
                workspace.focus_center_pane(window, cx);
            })
            .log_err();

        assert!(
            !panel.focus_handle(cx).is_focused(window),
            "Project panel should not be focused after focusing on center pane."
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.focus_handle(cx).is_focused(window),
            "Project panel should not be focused after focusing the center pane."
        );
    });

    // Create an unsaved buffer and verify that pane: reveal in project panel`
    // still activates and focuses the panel.
    let pane = workspace.update(cx, |workspace, _| workspace.active_pane().clone());
    pane.update_in(cx, |pane, window, cx| {
        let item = cx.new(|cx| TestItem::new(cx).with_label("Unsaved buffer"));
        pane.add_item(Box::new(item), false, false, None, window, cx);
    });

    cx.dispatch_action(workspace::RevealInProjectPanel::default());
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel
            .workspace
            .update(cx, |workspace, cx| {
                assert!(
                    workspace.active_item(cx).is_some(),
                    "Workspace should have an active item."
                );
            })
            .unwrap();

        assert!(
            panel.focus_handle(cx).is_focused(window),
            "Project panel should be focused even for an unsaved buffer."
        );
    });
}
