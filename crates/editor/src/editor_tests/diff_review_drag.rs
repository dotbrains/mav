use super::*;

#[gpui::test]
fn test_diff_review_multiline_selection(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Create an editor with multiple lines of text
    let editor = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("line 1\nline 2\nline 3\nline 4\nline 5\n", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    // Test showing overlay with a multi-line selection (lines 1-3, which are rows 0-2)
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(2), window, cx);
        })
        .unwrap();

    // Verify line range
    editor
        .update(cx, |editor, _window, cx| {
            assert!(!editor.diff_review_overlays.is_empty());
            assert_eq!(editor.diff_review_line_range(cx), Some((0, 2)));
        })
        .unwrap();

    // Dismiss and test with reversed range (end < start)
    editor
        .update(cx, |editor, _window, cx| {
            editor.dismiss_all_diff_review_overlays(cx);
        })
        .unwrap();

    // Show overlay with reversed range - should normalize it
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(3)..DisplayRow(1), window, cx);
        })
        .unwrap();

    // Verify range is normalized (start <= end)
    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(editor.diff_review_line_range(cx), Some((1, 3)));
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_drag_state(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("line 1\nline 2\nline 3\n", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    // Initially no drag state
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(editor.diff_review_drag_state.is_none());
        })
        .unwrap();

    // Start drag at row 1
    editor
        .update(cx, |editor, window, cx| {
            editor.start_diff_review_drag(DisplayRow(1), window, cx);
        })
        .unwrap();

    // Verify drag state is set
    editor
        .update(cx, |editor, window, cx| {
            assert!(editor.diff_review_drag_state.is_some());
            let snapshot = editor.snapshot(window, cx);
            let range = editor
                .diff_review_drag_state
                .as_ref()
                .unwrap()
                .row_range(&snapshot.display_snapshot);
            assert_eq!(*range.start(), DisplayRow(1));
            assert_eq!(*range.end(), DisplayRow(1));
        })
        .unwrap();

    // Update drag to row 3
    editor
        .update(cx, |editor, window, cx| {
            editor.update_diff_review_drag(DisplayRow(3), window, cx);
        })
        .unwrap();

    // Verify drag state is updated
    editor
        .update(cx, |editor, window, cx| {
            assert!(editor.diff_review_drag_state.is_some());
            let snapshot = editor.snapshot(window, cx);
            let range = editor
                .diff_review_drag_state
                .as_ref()
                .unwrap()
                .row_range(&snapshot.display_snapshot);
            assert_eq!(*range.start(), DisplayRow(1));
            assert_eq!(*range.end(), DisplayRow(3));
        })
        .unwrap();

    // End drag - should show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.end_diff_review_drag(window, cx);
        })
        .unwrap();

    // Verify drag state is cleared and overlay is shown
    editor
        .update(cx, |editor, _window, cx| {
            assert!(editor.diff_review_drag_state.is_none());
            assert!(!editor.diff_review_overlays.is_empty());
            assert_eq!(editor.diff_review_line_range(cx), Some((1, 3)));
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_drag_cancel(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Start drag
    editor
        .update(cx, |editor, window, cx| {
            editor.start_diff_review_drag(DisplayRow(0), window, cx);
        })
        .unwrap();

    // Verify drag state is set
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(editor.diff_review_drag_state.is_some());
        })
        .unwrap();

    // Cancel drag
    editor
        .update(cx, |editor, _window, cx| {
            editor.cancel_diff_review_drag(cx);
        })
        .unwrap();

    // Verify drag state is cleared and no overlay was created
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(editor.diff_review_drag_state.is_none());
            assert!(editor.diff_review_overlays.is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn test_calculate_overlay_height(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // This test verifies that calculate_overlay_height returns correct heights
    // based on comment count and expanded state.
    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let anchor = snapshot.anchor_before(Point::new(0, 0));
        let key = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::empty()),
            hunk_start_anchor: anchor,
        };

        // No comments: base height of 2
        let height_no_comments = editor.calculate_overlay_height(&key, true, &snapshot);
        assert_eq!(
            height_no_comments, 2,
            "Base height should be 2 with no comments"
        );

        // Add one comment
        editor.add_review_comment(key.clone(), "Comment 1".to_string(), anchor..anchor, cx);

        let snapshot = editor.buffer().read(cx).snapshot(cx);

        // With comments expanded: base (2) + header (1) + 2 per comment
        let height_expanded = editor.calculate_overlay_height(&key, true, &snapshot);
        assert_eq!(
            height_expanded,
            2 + 1 + 2, // base + header + 1 comment * 2
            "Height with 1 comment expanded"
        );

        // With comments collapsed: base (2) + header (1)
        let height_collapsed = editor.calculate_overlay_height(&key, false, &snapshot);
        assert_eq!(
            height_collapsed,
            2 + 1, // base + header only
            "Height with comments collapsed"
        );

        // Add more comments
        editor.add_review_comment(key.clone(), "Comment 2".to_string(), anchor..anchor, cx);
        editor.add_review_comment(key.clone(), "Comment 3".to_string(), anchor..anchor, cx);

        let snapshot = editor.buffer().read(cx).snapshot(cx);

        // With 3 comments expanded
        let height_3_expanded = editor.calculate_overlay_height(&key, true, &snapshot);
        assert_eq!(
            height_3_expanded,
            2 + 1 + (3 * 2), // base + header + 3 comments * 2
            "Height with 3 comments expanded"
        );

        // Collapsed height stays the same regardless of comment count
        let height_3_collapsed = editor.calculate_overlay_height(&key, false, &snapshot);
        assert_eq!(
            height_3_collapsed,
            2 + 1, // base + header only
            "Height with 3 comments collapsed should be same as 1 comment collapsed"
        );
    });
}
