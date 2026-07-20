use super::*;

#[gpui::test]
fn test_orphaned_comments_are_cleaned_up(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Create an editor with some text
    let editor = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("line 1\nline 2\nline 3\n", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    // Add a comment with an anchor on line 2
    editor
        .update(cx, |editor, _window, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let anchor = snapshot.anchor_after(Point::new(1, 0)); // Line 2
            let key = DiffHunkKey {
                file_path: Arc::from(util::rel_path::RelPath::empty()),
                hunk_start_anchor: anchor,
            };
            editor.add_review_comment(key, "Comment on line 2".to_string(), anchor..anchor, cx);
            assert_eq!(editor.total_review_comment_count(), 1);
        })
        .unwrap();

    // Delete all content (this should orphan the comment's anchor)
    editor
        .update(cx, |editor, window, cx| {
            editor.select_all(&SelectAll, window, cx);
            editor.insert("completely new content", window, cx);
        })
        .unwrap();

    // Trigger cleanup
    editor
        .update(cx, |editor, _window, cx| {
            editor.cleanup_orphaned_review_comments(cx);
            // Comment should be removed because its anchor is invalid
            assert_eq!(editor.total_review_comment_count(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn test_orphaned_comments_cleanup_called_on_buffer_edit(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Create an editor with some text
    let editor = cx.add_window(|window, cx| {
        let buffer = cx.new(|cx| Buffer::local("line 1\nline 2\nline 3\n", cx));
        let multi_buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
        Editor::new(EditorMode::full(), multi_buffer, None, window, cx)
    });

    // Add a comment with an anchor on line 2
    editor
        .update(cx, |editor, _window, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let anchor = snapshot.anchor_after(Point::new(1, 0)); // Line 2
            let key = DiffHunkKey {
                file_path: Arc::from(util::rel_path::RelPath::empty()),
                hunk_start_anchor: anchor,
            };
            editor.add_review_comment(key, "Comment on line 2".to_string(), anchor..anchor, cx);
            assert_eq!(editor.total_review_comment_count(), 1);
        })
        .unwrap();

    // Edit the buffer - this should trigger cleanup via on_buffer_event
    // Delete all content which orphans the anchor
    editor
        .update(cx, |editor, window, cx| {
            editor.select_all(&SelectAll, window, cx);
            editor.insert("completely new content", window, cx);
            // The cleanup is called automatically in on_buffer_event when Edited fires
        })
        .unwrap();

    // Verify cleanup happened automatically (not manually triggered)
    editor
        .update(cx, |editor, _window, _cx| {
            // Comment should be removed because its anchor became invalid
            // and cleanup was called automatically on buffer edit
            assert_eq!(editor.total_review_comment_count(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn test_comments_stored_for_multiple_hunks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // This test verifies that comments can be stored for multiple different hunks
    // and that hunk_comment_count correctly identifies comments per hunk.
    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);

        // Create two different hunk keys (simulating two different files)
        let anchor = snapshot.anchor_before(Point::new(0, 0));
        let key1 = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("file1.rs").unwrap()),
            hunk_start_anchor: anchor,
        };
        let key2 = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("file2.rs").unwrap()),
            hunk_start_anchor: anchor,
        };

        // Add comments to first hunk
        editor.add_review_comment(
            key1.clone(),
            "Comment 1 for file1".to_string(),
            anchor..anchor,
            cx,
        );
        editor.add_review_comment(
            key1.clone(),
            "Comment 2 for file1".to_string(),
            anchor..anchor,
            cx,
        );

        // Add comment to second hunk
        editor.add_review_comment(
            key2.clone(),
            "Comment for file2".to_string(),
            anchor..anchor,
            cx,
        );

        // Verify total count
        assert_eq!(editor.total_review_comment_count(), 3);

        // Verify per-hunk counts
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        assert_eq!(
            editor.hunk_comment_count(&key1, &snapshot),
            2,
            "file1 should have 2 comments"
        );
        assert_eq!(
            editor.hunk_comment_count(&key2, &snapshot),
            1,
            "file2 should have 1 comment"
        );

        // Verify comments_for_hunk returns correct comments
        let file1_comments = editor.comments_for_hunk(&key1, &snapshot);
        assert_eq!(file1_comments.len(), 2);
        assert_eq!(file1_comments[0].comment, "Comment 1 for file1");
        assert_eq!(file1_comments[1].comment, "Comment 2 for file1");

        let file2_comments = editor.comments_for_hunk(&key2, &snapshot);
        assert_eq!(file2_comments.len(), 1);
        assert_eq!(file2_comments[0].comment, "Comment for file2");
    });
}

#[gpui::test]
fn test_same_hunk_detected_by_matching_keys(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // This test verifies that hunk_keys_match correctly identifies when two
    // DiffHunkKeys refer to the same hunk (same file path and anchor point).
    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    _ = editor.update(cx, |editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let anchor = snapshot.anchor_before(Point::new(0, 0));

        // Create two keys with the same file path and anchor
        let key1 = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("file.rs").unwrap()),
            hunk_start_anchor: anchor,
        };
        let key2 = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("file.rs").unwrap()),
            hunk_start_anchor: anchor,
        };

        // Add comment to first key
        editor.add_review_comment(key1, "Test comment".to_string(), anchor..anchor, cx);

        // Verify second key (same hunk) finds the comment
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        assert_eq!(
            editor.hunk_comment_count(&key2, &snapshot),
            1,
            "Same hunk should find the comment"
        );

        // Create a key with different file path
        let different_file_key = DiffHunkKey {
            file_path: Arc::from(util::rel_path::RelPath::unix("other.rs").unwrap()),
            hunk_start_anchor: anchor,
        };

        // Different file should not find the comment
        assert_eq!(
            editor.hunk_comment_count(&different_file_key, &snapshot),
            0,
            "Different file should not find the comment"
        );
    });
}

#[gpui::test]
fn test_overlay_comments_expanded_state(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // This test verifies that set_diff_review_comments_expanded correctly
    // updates the expanded state of overlays.
    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(0), window, cx);
        })
        .unwrap();

    // Verify initially expanded (default)
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(
                editor.diff_review_overlays[0].comments_expanded,
                "Should be expanded by default"
            );
        })
        .unwrap();

    // Set to collapsed using the public method
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_diff_review_comments_expanded(false, cx);
        })
        .unwrap();

    // Verify collapsed
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(
                !editor.diff_review_overlays[0].comments_expanded,
                "Should be collapsed after setting to false"
            );
        })
        .unwrap();

    // Set back to expanded
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_diff_review_comments_expanded(true, cx);
        })
        .unwrap();

    // Verify expanded again
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(
                editor.diff_review_overlays[0].comments_expanded,
                "Should be expanded after setting to true"
            );
        })
        .unwrap();
}
