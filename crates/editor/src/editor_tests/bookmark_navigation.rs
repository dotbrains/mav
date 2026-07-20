use super::bookmark_context::BookmarkTestContext;
use super::*;

#[gpui::test]
async fn test_bookmark_not_available_in_single_line_editor(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let (editor, _cx) = cx.add_window_view(|window, cx| Editor::single_line(window, cx));

    editor.update(cx, |editor, _cx| {
        assert!(
            editor.bookmark_store.is_none(),
            "Single-line editors should not have a bookmark store"
        );
    });
}

#[gpui::test]
async fn test_edit_bookmark_does_not_open_prompt_without_existing_bookmark(
    cx: &mut TestAppContext,
) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    assert!(!ctx.confirm_action_available());

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.edit_bookmark(&actions::EditBookmark, window, cx);
        });

    assert!(!ctx.confirm_action_available());
    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_edit_bookmark_updates_label_after_confirmation(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.add_bookmark_with_label("old label");
    ctx.assert_bookmark_labels(vec![(0, "old label")]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.edit_bookmark(&actions::EditBookmark, window, cx);
        });

    assert!(ctx.confirm_action_available());
    ctx.cx.dispatch_action(SelectAll);
    ctx.cx.simulate_input("new label");
    ctx.cx.dispatch_action(menu::Confirm);

    ctx.assert_bookmark_labels(vec![(0, "new label")]);
}

#[gpui::test]
async fn test_bookmark_navigation_lands_at_column_zero(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
            editor.move_down(&MoveDown, window, cx);
            editor.move_to_end_of_line(
                &MoveToEndOfLine {
                    stop_at_soft_wraps: true,
                },
                window,
                cx,
            );
        });

    let column_before_toggle = ctx.cursor_point().column;
    assert_eq!(
        column_before_toggle, 11,
        "Cursor should be at the 11th column before toggling bookmark, got column {column_before_toggle}"
    );

    ctx.add_bookmark_with_label("");

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
        });

    ctx.go_to_next_bookmark();

    let cursor = ctx.cursor_point();
    assert_eq!(cursor.row, 1, "Should navigate to the bookmarked row");
    assert_eq!(
        cursor.column, 0,
        "Bookmark navigation should always land at column 0"
    );
}

#[gpui::test]
async fn test_bookmark_set_from_nonzero_column_toggles_off_from_column_zero(
    cx: &mut TestAppContext,
) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
            editor.move_down(&MoveDown, window, cx);
            editor.move_to_end_of_line(
                &MoveToEndOfLine {
                    stop_at_soft_wraps: true,
                },
                window,
                cx,
            );
        });
    ctx.add_bookmark_with_label("");

    ctx.assert_bookmark_rows(vec![1]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning_of_line(
                &MoveToBeginningOfLine {
                    stop_at_soft_wraps: true,
                    stop_at_indent: false,
                },
                window,
                cx,
            );
        });
    ctx.toggle_bookmark();

    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_go_to_next_bookmark(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[2, 5, 8]);

    ctx.move_to_row(0);

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        2,
        "First next-bookmark should go to row 2"
    );

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        5,
        "Second next-bookmark should go to row 5"
    );

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        8,
        "Third next-bookmark should go to row 8"
    );

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        2,
        "Next-bookmark should wrap around to row 2"
    );
}

#[gpui::test]
async fn test_go_to_previous_bookmark(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[2, 5, 8]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_end(&MoveToEnd, window, cx);
        });

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        8,
        "First prev-bookmark should go to row 8"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        5,
        "Second prev-bookmark should go to row 5"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        2,
        "Third prev-bookmark should go to row 2"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        8,
        "Prev-bookmark should wrap around to row 8"
    );
}

#[gpui::test]
async fn test_go_to_bookmark_when_cursor_on_bookmarked_line(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[3, 7]);

    ctx.move_to_row(3);

    ctx.go_to_next_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        7,
        "Next from bookmarked row 3 should go to row 7"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        3,
        "Previous from bookmarked row 7 should go to row 3"
    );

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 7, "Next from row 3 should go to row 7");

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 3, "Next from row 7 should wrap to row 3");
}

#[gpui::test]
async fn test_go_to_bookmark_with_out_of_order_bookmarks(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[8, 1, 5]);

    ctx.move_to_row(0);

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 1, "First next should go to row 1");

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 5, "Second next should go to row 5");

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 8, "Third next should go to row 8");

    ctx.go_to_next_bookmark();
    assert_eq!(ctx.cursor_row(), 1, "Fourth next should wrap to row 1");

    ctx.go_to_previous_bookmark();
    assert_eq!(
        ctx.cursor_row(),
        8,
        "Prev from row 1 should wrap around to row 8"
    );

    ctx.go_to_previous_bookmark();
    assert_eq!(ctx.cursor_row(), 5, "Prev from row 8 should go to row 5");

    ctx.go_to_previous_bookmark();
    assert_eq!(ctx.cursor_row(), 1, "Prev from row 5 should go to row 1");
}
