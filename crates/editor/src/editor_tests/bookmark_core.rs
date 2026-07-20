use super::bookmark_context::BookmarkTestContext;
use super::*;

#[gpui::test]
async fn test_bookmark_toggling(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.add_bookmark_with_label("");
    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_end(&MoveToEnd, window, cx);
        });
    ctx.add_bookmark_with_label("");

    ctx.assert_bookmarked_file_count(1);
    ctx.assert_bookmark_rows(vec![0, 3]);

    ctx.move_to_row(0);
    ctx.toggle_bookmark();

    ctx.assert_bookmarked_file_count(1);
    ctx.assert_bookmark_rows(vec![3]);

    ctx.move_to_row(3);
    ctx.toggle_bookmark();

    ctx.assert_bookmarked_file_count(0);
    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_bookmark_toggling_with_multiple_selections(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.select_rows(&[0, 1, 2]);
    ctx.toggle_bookmark();

    ctx.assert_prompt_block_count(3);
    ctx.assert_bookmarked_file_count(0);

    ctx.confirm_bookmark_prompt_at_block_index(0, "first label");
    ctx.assert_prompt_block_count(2);
    ctx.confirm_bookmark_prompt_at_block_index(0, "second label");
    ctx.assert_prompt_block_count(1);
    ctx.confirm_bookmark_prompt_at_block_index(0, "third label");
    ctx.assert_prompt_block_count(0);

    ctx.assert_bookmarked_file_count(1);
    ctx.assert_bookmark_labels(vec![
        (0, "first label"),
        (1, "second label"),
        (2, "third label"),
    ]);

    ctx.select_rows(&[0, 1, 2, 3]);
    ctx.toggle_bookmark();

    ctx.assert_prompt_block_count(1);
    ctx.assert_bookmark_labels(vec![
        (0, "first label"),
        (1, "second label"),
        (2, "third label"),
    ]);

    ctx.confirm_bookmark_prompt_at_block_index(0, "fourth label");

    ctx.assert_prompt_block_count(0);
    ctx.assert_bookmark_labels(vec![
        (0, "first label"),
        (1, "second label"),
        (2, "third label"),
        (3, "fourth label"),
    ]);

    ctx.select_rows(&[0, 1, 2, 3]);
    ctx.toggle_bookmark();

    ctx.assert_prompt_block_count(0);
    ctx.assert_bookmarked_file_count(0);
    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_bookmark_toggle_deduplicates_by_row(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
        });
    ctx.add_bookmark_with_label("");

    ctx.assert_bookmark_rows(vec![0]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_end_of_line(
                &MoveToEndOfLine {
                    stop_at_soft_wraps: true,
                },
                window,
                cx,
            );
        });
    ctx.toggle_bookmark();

    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_bookmark_survives_edits(cx: &mut TestAppContext) {
    let mut ctx =
        BookmarkTestContext::new("First line\nSecond line\nThird line\nFourth line", cx).await;

    ctx.move_to_row(2);
    ctx.add_bookmark_with_label("");
    ctx.assert_bookmark_rows(vec![2]);

    ctx.editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.move_to_beginning(&MoveToBeginning, window, cx);
            editor.newline(&Newline, window, cx);
        });

    ctx.assert_bookmark_rows(vec![3]);

    ctx.move_to_row(3);
    ctx.toggle_bookmark();
    ctx.assert_bookmark_rows(vec![]);
}

#[gpui::test]
async fn test_active_bookmarks(cx: &mut TestAppContext) {
    let mut ctx = BookmarkTestContext::new(
        "Line 0\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9",
        cx,
    )
    .await;

    ctx.toggle_bookmarks_at_rows(&[1, 3, 5, 8]);

    let active = ctx
        .editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.active_bookmarks(DisplayRow(0)..DisplayRow(10), window, cx)
        });
    assert!(active.contains(&DisplayRow(1)));
    assert!(active.contains(&DisplayRow(3)));
    assert!(active.contains(&DisplayRow(5)));
    assert!(active.contains(&DisplayRow(8)));
    assert!(!active.contains(&DisplayRow(0)));
    assert!(!active.contains(&DisplayRow(2)));
    assert!(!active.contains(&DisplayRow(9)));

    let active = ctx
        .editor
        .update_in(&mut ctx.cx, |editor: &mut Editor, window, cx| {
            editor.active_bookmarks(DisplayRow(2)..DisplayRow(6), window, cx)
        });
    assert!(active.contains(&DisplayRow(3)));
    assert!(active.contains(&DisplayRow(5)));
    assert!(!active.contains(&DisplayRow(1)));
    assert!(!active.contains(&DisplayRow(8)));
}
