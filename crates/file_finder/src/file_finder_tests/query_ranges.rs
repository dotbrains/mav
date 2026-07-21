use super::*;

#[gpui::test]
async fn test_row_column_numbers_query_outside_file(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    let first_file_name = "first.rs";
    let first_file_contents = "// First Rust file";
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/src"),
            json!({
                "test": {
                    first_file_name: first_file_contents,
                    "second.rs": "// Second Rust file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;

    let (picker, workspace, cx) = build_find_picker(project, cx);

    let file_query = &first_file_name[..3];
    let file_row = 200;
    let file_column = 300;
    assert!(file_column > first_file_contents.len());
    let query_outside_file = format!("{file_query}:{file_row}:{file_column}");
    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .update_matches(query_outside_file.to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_match_at_position(finder, 1, &query_outside_file.to_string());
        let delegate = &finder.delegate;
        assert_eq!(delegate.matches.len(), 2);
        let latest_search_query = delegate
            .latest_search_query
            .as_ref()
            .expect("Finder should have a query after the update_matches call");
        assert_eq!(latest_search_query.raw_query, query_outside_file);
        assert_eq!(latest_search_query.file_query_end, Some(file_query.len()));
        assert_eq!(latest_search_query.path_position.row, Some(file_row));
        assert_eq!(
            latest_search_query.path_position.column,
            Some(file_column as u32)
        );
    });

    cx.dispatch_action(Confirm);

    let editor = cx.update(|_, cx| workspace.read(cx).active_item_as::<Editor>(cx).unwrap());
    cx.executor().advance_clock(Duration::from_secs(2));

    editor.update(cx, |editor, cx| {
            let all_selections = editor.selections.all_adjusted(&editor.display_snapshot(cx));
            assert_eq!(
                all_selections.len(),
                1,
                "Expected to have 1 selection (caret) after file finder confirm, but got: {all_selections:?}"
            );
            let caret_selection = all_selections.into_iter().next().unwrap();
            assert_eq!(caret_selection.start, caret_selection.end,
                "Caret selection should have its start and end at the same position");
            assert_eq!(0, caret_selection.start.row,
                "Excessive rows (as in query outside file borders) should get trimmed to last file row");
            assert_eq!(first_file_contents.len(), caret_selection.start.column as usize,
                "Excessive columns (as in query outside file borders) should get trimmed to selected row's last column");
        });
}

#[test]
fn test_line_range_query_parsing() {
    let query = parse_file_search_query("fs/smb/server/connection.c:428-440");

    assert_eq!(query.raw_query, "fs/smb/server/connection.c:428-440");
    assert_eq!(
        query.file_query_end,
        Some("fs/smb/server/connection.c".len())
    );
    assert_eq!(query.path_query(), "fs/smb/server/connection.c");
    assert_eq!(query.path_position.row, Some(428));
    assert_eq!(query.path_position.column, None);
    assert_eq!(query.line_range, Some(428..=440));
}

#[test]
fn test_parse_search_query() {
    // Test trailing colon stripping.
    let query = parse_file_search_query("content.rs:2:");
    assert_eq!(query.raw_query, "content.rs:2");
    assert_eq!(query.path_query(), "content.rs");
    assert_eq!(query.path_position.row, Some(2));
    assert_eq!(query.path_position.column, None);
    assert_eq!(query.line_range, None);

    // Test multiple trailing colons are also stripped.
    let query = parse_file_search_query("content.rs:2:::");
    assert_eq!(query.raw_query, "content.rs:2");
    assert_eq!(query.path_query(), "content.rs");
    assert_eq!(query.path_position.row, Some(2));
    assert_eq!(query.path_position.column, None);
    assert_eq!(query.line_range, None);

    // Test trailing colon after an incomplete range is stripped.
    let query = parse_file_search_query("content.rs:2-:");
    assert_eq!(query.raw_query, "content.rs:2-");
    assert_eq!(query.path_query(), "content.rs");
    assert_eq!(query.path_position.row, Some(2));
    assert_eq!(query.path_position.column, None);
    assert_eq!(query.line_range, None);

    // Test trailing colon after a complete range is stripped, range is preserved.
    let query = parse_file_search_query("content.rs:2-4:");
    assert_eq!(query.raw_query, "content.rs:2-4");
    assert_eq!(query.path_query(), "content.rs");
    assert_eq!(query.path_position.row, Some(2));
    assert_eq!(query.path_position.column, None);
    assert_eq!(query.line_range, Some(2..=4));

    // Test multiple trailing colons after a complete range are all stripped.
    let query = parse_file_search_query("content.rs:2-4:::");
    assert_eq!(query.raw_query, "content.rs:2-4");
    assert_eq!(query.path_query(), "content.rs");
    assert_eq!(query.path_position.row, Some(2));
    assert_eq!(query.path_position.column, None);
    assert_eq!(query.line_range, Some(2..=4));

    // Test invalid end should fall back to using the start as a single row.
    let query = parse_file_search_query("content.rs:5-x");
    assert_eq!(query.raw_query, "content.rs:5-x");
    assert_eq!(query.path_query(), "content.rs");
    assert_eq!(query.path_position.row, Some(5));
    assert_eq!(query.path_position.column, None);
    assert_eq!(query.line_range, None);

    // Test reversed range (end < start) should fall back to using the start as a single row.
    let query = parse_file_search_query("content.rs:10-5");
    assert_eq!(query.raw_query, "content.rs:10-5");
    assert_eq!(query.path_query(), "content.rs");
    assert_eq!(query.path_position.row, Some(10));
    assert_eq!(query.path_position.column, None);
    assert_eq!(query.line_range, None);
}

#[gpui::test]
async fn test_line_range_query_selects_lines(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    let first_file_contents = "line 1\nline 2\nline 3\nline 4\nline 5";
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/src"),
            json!({
                "test": {
                    "first.rs": first_file_contents,
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;

    let (picker, workspace, cx) = build_find_picker(project, cx);

    let query = "test/first.rs:2-4";
    picker
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(query.to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 2);
        assert_match_at_position(finder, 0, "first.rs");
        assert_match_at_position(finder, 1, "first.rs:2-4");

        let latest_search_query = finder
            .delegate
            .latest_search_query
            .as_ref()
            .expect("Finder should have a query after the update_matches call");
        assert_eq!(latest_search_query.raw_query, query);
        assert_eq!(
            latest_search_query.file_query_end,
            Some("test/first.rs".len())
        );
        assert_eq!(latest_search_query.path_position.row, Some(2));
        assert_eq!(latest_search_query.path_position.column, None);
        assert_eq!(latest_search_query.line_range, Some(2..=4));
    });

    cx.dispatch_action(Confirm);

    let editor = cx.update(|_, cx| workspace.read(cx).active_item_as::<Editor>(cx).unwrap());
    cx.executor().advance_clock(Duration::from_secs(2));

    editor.update(cx, |editor, cx| {
        let all_selections = editor.selections.all_adjusted(&editor.display_snapshot(cx));
        assert_eq!(
            all_selections.len(),
            1,
            "Expected to have 1 selection after file finder confirm, but got: {all_selections:?}"
        );
        let selection = all_selections.into_iter().next().unwrap();
        assert_eq!(selection.start.row, 1);
        assert_eq!(selection.start.column, 0);
        assert_eq!(selection.end.row, 4);
        assert_eq!(selection.end.column, 0);
    });
}

#[gpui::test]
async fn test_line_range_query_outside_file_clamps_to_eof(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    let first_file_contents = "line 1\nline 2";
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/src"),
            json!({
                "test": {
                    "first.rs": first_file_contents,
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;

    let (picker, workspace, cx) = build_find_picker(project, cx);

    let query = "test/first.rs:200-300";
    picker
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(query.to_string(), window, cx)
        })
        .await;

    cx.dispatch_action(Confirm);

    let editor = cx.update(|_, cx| workspace.read(cx).active_item_as::<Editor>(cx).unwrap());
    cx.executor().advance_clock(Duration::from_secs(2));

    editor.update(cx, |editor, cx| {
        let all_selections = editor.selections.all_adjusted(&editor.display_snapshot(cx));
        assert_eq!(
            all_selections.len(),
            1,
            "Expected to have 1 selection after file finder confirm, but got: {all_selections:?}"
        );
        let selection = all_selections.into_iter().next().unwrap();
        assert_eq!(selection.start, selection.end);
        assert_eq!(selection.start.row, 1);
        assert_eq!(selection.start.column, "line 2".len() as u32);
    });
}
