use super::*;

#[gpui::test]
async fn test_newline_replacement_in_single_line(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let (editor, cx) = cx.add_window_view(Editor::single_line);
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("oops\n\nwow\n", window, cx)
    });
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.display_text(cx), "oops⋯⋯wow⋯");
    });
    editor.update(cx, |editor, cx| {
        editor.edit([(MultiBufferOffset(3)..MultiBufferOffset(5), "")], cx)
    });
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.display_text(cx), "oop⋯wow⋯");
    });
}

#[gpui::test]
async fn test_non_utf_8_opens(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    cx.update(|cx| {
        register_project_item::<Editor>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root1", json!({})).await;
    fs.insert_file("/root1/one.pdf", vec![0xff, 0xfe, 0xfd])
        .await;

    let project = Project::test(fs, ["/root1".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let worktree_id = project.update(cx, |project, cx| {
        project.worktrees(cx).next().unwrap().read(cx).id()
    });

    let handle = workspace
        .update_in(cx, |workspace, window, cx| {
            let project_path = (worktree_id, rel_path("one.pdf"));
            workspace.open_path(project_path, None, true, window, cx)
        })
        .await
        .unwrap();
    // The test file content `vec![0xff, 0xfe, ...]` starts with a UTF-16 LE BOM.
    // Previously, this fell back to `InvalidItemView` because it wasn't valid UTF-8.
    // With auto-detection enabled, this is now recognized as UTF-16 and opens in the Editor.
    assert_eq!(handle.to_any_view().entity_type(), TypeId::of::<Editor>());
}

#[gpui::test]
async fn test_select_next_prev_syntax_node(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let language = Arc::new(Language::new(
        LanguageConfig::default(),
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));

    // Test hierarchical sibling navigation
    let text = r#"
        fn outer() {
            if condition {
                let a = 1;
            }
            let b = 2;
        }

        fn another() {
            let c = 3;
        }
    "#;

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| build_editor(buffer, window, cx));

    // Wait for parsing to complete
    editor
        .condition::<crate::EditorEvent>(cx, |editor, cx| !editor.buffer.read(cx).is_parsing(cx))
        .await;

    editor.update_in(cx, |editor, window, cx| {
        // Start by selecting "let a = 1;" inside the if block
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(3), 16)..DisplayPoint::new(DisplayRow(3), 26)
            ]);
        });

        let initial_selection = editor
            .selections
            .display_ranges(&editor.display_snapshot(cx));
        assert_eq!(initial_selection.len(), 1, "Should have one selection");

        // Test select next sibling - should move up levels to find the next sibling
        // Since "let a = 1;" has no siblings in the if block, it should move up
        // to find "let b = 2;" which is a sibling of the if block
        editor.select_next_syntax_node(&SelectNextSyntaxNode, window, cx);
        let next_selection = editor
            .selections
            .display_ranges(&editor.display_snapshot(cx));

        // Should have a selection and it should be different from the initial
        assert_eq!(
            next_selection.len(),
            1,
            "Should have one selection after next"
        );
        assert_ne!(
            next_selection[0], initial_selection[0],
            "Next sibling selection should be different"
        );

        // Test hierarchical navigation by going to the end of the current function
        // and trying to navigate to the next function
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_display_ranges([
                DisplayPoint::new(DisplayRow(5), 12)..DisplayPoint::new(DisplayRow(5), 22)
            ]);
        });

        editor.select_next_syntax_node(&SelectNextSyntaxNode, window, cx);
        let function_next_selection = editor
            .selections
            .display_ranges(&editor.display_snapshot(cx));

        // Should move to the next function
        assert_eq!(
            function_next_selection.len(),
            1,
            "Should have one selection after function next"
        );

        // Test select previous sibling navigation
        editor.select_prev_syntax_node(&SelectPreviousSyntaxNode, window, cx);
        let prev_selection = editor
            .selections
            .display_ranges(&editor.display_snapshot(cx));

        // Should have a selection and it should be different
        assert_eq!(
            prev_selection.len(),
            1,
            "Should have one selection after prev"
        );
        assert_ne!(
            prev_selection[0], function_next_selection[0],
            "Previous sibling selection should be different from next"
        );
    });
}

#[gpui::test]
async fn test_next_prev_document_highlight(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    cx.set_state(
        "let ˇvariable = 42;
let another = variable + 1;
let result = variable * 2;",
    );

    // Set up document highlights manually (simulating LSP response)
    cx.update_editor(|editor, _window, cx| {
        let buffer_snapshot = editor.buffer().read(cx).snapshot(cx);

        // Create highlights for "variable" occurrences
        let highlight_ranges = [
            Point::new(0, 4)..Point::new(0, 12),  // First "variable"
            Point::new(1, 14)..Point::new(1, 22), // Second "variable"
            Point::new(2, 13)..Point::new(2, 21), // Third "variable"
        ];

        let anchor_ranges: Vec<_> = highlight_ranges
            .iter()
            .map(|range| range.clone().to_anchors(&buffer_snapshot))
            .collect();

        editor.highlight_background(
            HighlightKey::DocumentHighlightRead,
            &anchor_ranges,
            |_, theme| theme.colors().editor_document_highlight_read_background,
            cx,
        );
    });

    // Go to next highlight - should move to second "variable"
    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_document_highlight(&GoToNextDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let variable = 42;
let another = ˇvariable + 1;
let result = variable * 2;",
    );

    // Go to next highlight - should move to third "variable"
    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_document_highlight(&GoToNextDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let variable = 42;
let another = variable + 1;
let result = ˇvariable * 2;",
    );

    // Go to next highlight - should stay at third "variable" (no wrap-around)
    cx.update_editor(|editor, window, cx| {
        editor.go_to_next_document_highlight(&GoToNextDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let variable = 42;
let another = variable + 1;
let result = ˇvariable * 2;",
    );

    // Now test going backwards from third position
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_document_highlight(&GoToPreviousDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let variable = 42;
let another = ˇvariable + 1;
let result = variable * 2;",
    );

    // Go to previous highlight - should move to first "variable"
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_document_highlight(&GoToPreviousDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let ˇvariable = 42;
let another = variable + 1;
let result = variable * 2;",
    );

    // Go to previous highlight - should stay on first "variable"
    cx.update_editor(|editor, window, cx| {
        editor.go_to_prev_document_highlight(&GoToPreviousDocumentHighlight, window, cx);
    });
    cx.assert_editor_state(
        "let ˇvariable = 42;
let another = variable + 1;
let result = variable * 2;",
    );
}
