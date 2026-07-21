use super::*;

#[gpui::test]
async fn test_matching_paths(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "banana": "",
                    "bandana": "",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;

    let (picker, workspace, cx) = build_find_picker(project, cx);

    simulate_input(cx, "bna");
    picker.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 3);
    });
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(Confirm);
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        assert_eq!(active_editor.read(cx).title(cx), "bandana");
    });

    for bandana_query in [
        "bandana",
        "./bandana",
        ".\\bandana",
        util::path!("a/bandana"),
        "b/bandana",
        "b\\bandana",
        " bandana",
        "bandana ",
        " bandana ",
        " ndan ",
        " band ",
        "a bandana",
        "bandana:",
    ] {
        picker
            .update_in(cx, |picker, window, cx| {
                picker
                    .delegate
                    .update_matches(bandana_query.to_string(), window, cx)
            })
            .await;
        picker.update(cx, |picker, _| {
            assert_eq!(
                picker.delegate.matches.len(),
                // existence of CreateNew option depends on whether path already exists
                if bandana_query == util::path!("a/bandana") {
                    1
                } else {
                    2
                },
                "Wrong number of matches for bandana query '{bandana_query}'. Matches: {:?}",
                picker.delegate.matches
            );
        });
        cx.dispatch_action(SelectNext);
        cx.dispatch_action(Confirm);
        cx.read(|cx| {
            let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
            assert_eq!(
                active_editor.read(cx).title(cx),
                "bandana",
                "Wrong match for bandana query '{bandana_query}'"
            );
        });
    }
}

#[gpui::test]
async fn test_matching_paths_with_colon(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "foo:bar.rs": "",
                    "foo.rs": "",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;

    let (picker, _, cx) = build_find_picker(project, cx);

    // 'foo:' matches both files
    simulate_input(cx, "foo:");
    picker.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 3);
        assert_match_at_position(picker, 0, "foo.rs");
        assert_match_at_position(picker, 1, "foo:bar.rs");
    });

    // 'foo:b' matches one of the files
    simulate_input(cx, "b");
    picker.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 2);
        assert_match_at_position(picker, 0, "foo:bar.rs");
    });

    cx.dispatch_action(editor::actions::Backspace);

    // 'foo:1' matches both files, specifying which row to jump to
    simulate_input(cx, "1");
    picker.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 3);
        assert_match_at_position(picker, 0, "foo.rs");
        assert_match_at_position(picker, 1, "foo:bar.rs");
    });
}

#[gpui::test]
async fn test_unicode_paths(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "İg": " ",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;

    let (picker, workspace, cx) = build_find_picker(project, cx);

    simulate_input(cx, "g");
    picker.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 2);
        assert_match_at_position(picker, 1, "g");
    });
    cx.dispatch_action(Confirm);
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        assert_eq!(active_editor.read(cx).title(cx), "İg");
    });
}

#[gpui::test]
async fn test_absolute_paths(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "file1.txt": "",
                    "b": {
                        "file2.txt": "",
                    },
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;

    let (picker, workspace, cx) = build_find_picker(project, cx);

    let matching_abs_path = path!("/root/a/b/file2.txt").to_string();
    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .update_matches(matching_abs_path, window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        assert_eq!(
            collect_search_matches(picker).search_paths_only(),
            vec![rel_path("a/b/file2.txt").into()],
            "Matching abs path should be the only match"
        )
    });
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(Confirm);
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        assert_eq!(active_editor.read(cx).title(cx), "file2.txt");
    });

    let mismatching_abs_path = path!("/root/a/b/file1.txt").to_string();
    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .update_matches(mismatching_abs_path, window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        assert_eq!(
            collect_search_matches(picker).search_paths_only(),
            Vec::new(),
            "Mismatching abs path should produce no matches"
        )
    });
}

#[gpui::test]
async fn test_complex_path(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    cx.update(|cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_root: true,
                ..settings
            },
            cx,
        );
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "其他": {
                    "S数据表格": {
                        "task.xlsx": "some content",
                    },
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;

    let (picker, workspace, cx) = build_find_picker(project, cx);

    simulate_input(cx, "t");
    picker.update(cx, |picker, _| {
        assert_eq!(picker.delegate.matches.len(), 2);
        assert_eq!(
            collect_search_matches(picker).search_paths_only(),
            vec![rel_path("其他/S数据表格/task.xlsx").into()],
        )
    });
    cx.dispatch_action(Confirm);
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        assert_eq!(active_editor.read(cx).title(cx), "task.xlsx");
    });
}

#[gpui::test]
async fn test_row_column_numbers_query_inside_file(cx: &mut TestAppContext) {
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
    let file_row = 1;
    let file_column = 3;
    assert!(file_column <= first_file_contents.len());
    let query_inside_file = format!("{file_query}:{file_row}:{file_column}");
    picker
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(query_inside_file.to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_match_at_position(finder, 1, &query_inside_file.to_string());
        let finder = &finder.delegate;
        assert_eq!(finder.matches.len(), 2);
        let latest_search_query = finder
            .latest_search_query
            .as_ref()
            .expect("Finder should have a query after the update_matches call");
        assert_eq!(latest_search_query.raw_query, query_inside_file);
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
            assert_eq!(file_row, caret_selection.start.row + 1,
                "Query inside file should get caret with the same focus row");
            assert_eq!(file_column, caret_selection.start.column as usize + 1,
                "Query inside file should get caret with the same focus column");
        });
}

#[gpui::test]
async fn test_row_column_numbers_query_inside_unicode_file(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    let first_file_name = "first.rs";
    let first_file_contents = "aéøbcdef";
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
    let file_row = 1;
    let file_column = 5;
    let query_inside_file = format!("{file_query}:{file_row}:{file_column}");
    picker
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(query_inside_file.to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_match_at_position(finder, 1, &query_inside_file.to_string());
        let finder = &finder.delegate;
        assert_eq!(finder.matches.len(), 2);
        let latest_search_query = finder
            .latest_search_query
            .as_ref()
            .expect("Finder should have a query after the update_matches call");
        assert_eq!(latest_search_query.raw_query, query_inside_file);
        assert_eq!(latest_search_query.file_query_end, Some(file_query.len()));
        assert_eq!(latest_search_query.path_position.row, Some(file_row));
        assert_eq!(latest_search_query.path_position.column, Some(file_column));
    });

    cx.dispatch_action(Confirm);

    let editor = cx.update(|_, cx| workspace.read(cx).active_item_as::<Editor>(cx).unwrap());
    cx.executor().advance_clock(Duration::from_secs(2));

    let expected_column = first_file_contents
        .chars()
        .take(file_column as usize - 1)
        .map(|character| character.len_utf8())
        .sum::<usize>();

    editor.update(cx, |editor, cx| {
        let all_selections = editor.selections.all_adjusted(&editor.display_snapshot(cx));
        assert_eq!(
            all_selections.len(),
            1,
            "Expected to have 1 selection (caret) after file finder confirm, but got: {all_selections:?}"
        );
        let caret_selection = all_selections.into_iter().next().unwrap();
        assert_eq!(
            caret_selection.start, caret_selection.end,
            "Caret selection should have its start and end at the same position"
        );
        assert_eq!(
            file_row,
            caret_selection.start.row + 1,
            "Query inside file should get caret with the same focus row"
        );
        assert_eq!(
            expected_column,
            caret_selection.start.column as usize,
            "Query inside file should map user-visible columns to byte offsets for Unicode text"
        );
    });
}
