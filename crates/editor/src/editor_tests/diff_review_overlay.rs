use super::*;

#[gpui::test]
fn test_diff_review_overlay_show_and_dismiss(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(0), window, cx);
        })
        .unwrap();

    // Verify overlay is shown
    editor
        .update(cx, |editor, _window, cx| {
            assert!(!editor.diff_review_overlays.is_empty());
            assert_eq!(editor.diff_review_line_range(cx), Some((0, 0)));
            assert!(editor.diff_review_prompt_editor().is_some());
        })
        .unwrap();

    // Dismiss overlay
    editor
        .update(cx, |editor, _window, cx| {
            editor.dismiss_all_diff_review_overlays(cx);
        })
        .unwrap();

    // Verify overlay is dismissed
    editor
        .update(cx, |editor, _window, cx| {
            assert!(editor.diff_review_overlays.is_empty());
            assert_eq!(editor.diff_review_line_range(cx), None);
            assert!(editor.diff_review_prompt_editor().is_none());
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_overlay_dismiss_via_cancel(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(0), window, cx);
        })
        .unwrap();

    // Verify overlay is shown
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(!editor.diff_review_overlays.is_empty());
        })
        .unwrap();

    // Dismiss via dismiss_menus_and_popups (which is called by cancel action)
    editor
        .update(cx, |editor, window, cx| {
            editor.dismiss_menus_and_popups(true, window, cx);
        })
        .unwrap();

    // Verify overlay is dismissed
    editor
        .update(cx, |editor, _window, _cx| {
            assert!(editor.diff_review_overlays.is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_empty_comment_not_submitted(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Show overlay
    editor
        .update(cx, |editor, window, cx| {
            editor.show_diff_review_overlay(DisplayRow(0)..DisplayRow(0), window, cx);
        })
        .unwrap();

    // Try to submit without typing anything (empty comment)
    editor
        .update(cx, |editor, window, cx| {
            editor.submit_diff_review_comment(window, cx);
        })
        .unwrap();

    // Verify no comment was added
    editor
        .update(cx, |editor, _window, _cx| {
            assert_eq!(editor.total_review_comment_count(), 0);
        })
        .unwrap();

    // Try to submit with whitespace-only comment
    editor
        .update(cx, |editor, window, cx| {
            if let Some(prompt_editor) = editor.diff_review_prompt_editor().cloned() {
                prompt_editor.update(cx, |pe, cx| {
                    pe.insert("   \n\t  ", window, cx);
                });
            }
            editor.submit_diff_review_comment(window, cx);
        })
        .unwrap();

    // Verify still no comment was added
    editor
        .update(cx, |editor, _window, _cx| {
            assert_eq!(editor.total_review_comment_count(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn test_diff_review_inline_edit_flow(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let editor = cx.add_window(|window, cx| Editor::single_line(window, cx));

    // Add a comment directly
    let comment_id = editor
        .update(cx, |editor, _window, cx| {
            let key = test_hunk_key("");
            add_test_comment(editor, key, "Original comment", cx)
        })
        .unwrap();

    // Set comment to editing mode
    editor
        .update(cx, |editor, _window, cx| {
            editor.set_comment_editing(comment_id, true, cx);
        })
        .unwrap();

    // Verify editing flag is set
    editor
        .update(cx, |editor, _window, cx| {
            let key = test_hunk_key("");
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let comments = editor.comments_for_hunk(&key, &snapshot);
            assert_eq!(comments.len(), 1);
            assert!(comments[0].is_editing);
        })
        .unwrap();

    // Update the comment
    editor
        .update(cx, |editor, _window, cx| {
            let updated =
                editor.update_review_comment(comment_id, "Updated comment".to_string(), cx);
            assert!(updated);
        })
        .unwrap();

    // Verify comment was updated and editing flag is cleared
    editor
        .update(cx, |editor, _window, cx| {
            let key = test_hunk_key("");
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let comments = editor.comments_for_hunk(&key, &snapshot);
            assert_eq!(comments[0].comment, "Updated comment");
            assert!(!comments[0].is_editing);
        })
        .unwrap();
}
