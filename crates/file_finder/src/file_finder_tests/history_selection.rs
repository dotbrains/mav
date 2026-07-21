use super::*;

#[gpui::test]
async fn test_search_preserves_history_items(cx: &mut gpui::TestAppContext) {
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
            path!("/src"),
            json!({
                "test": {
                    "first.rs": "// First Rust file",
                    "second.rs": "// Second Rust file",
                    "third.rs": "// Third Rust file",
                    "fourth.rs": "// Fourth Rust file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1,);

        worktrees[0].read(cx).id()
    });

    // generate some history to select from
    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;
    open_close_queried_buffer("thi", 1, "third.rs", &workspace, cx).await;
    open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;

    let finder = open_file_picker(&workspace, cx);
    let first_query = "f";
    finder
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(first_query.to_string(), window, cx)
        })
        .await;
    finder.update(cx, |picker, _| {
            let matches = collect_search_matches(picker);
            assert_eq!(matches.history.len(), 1, "Only one history item contains {first_query}, it should be present and others should be filtered out");
            let history_match = matches.history_found_paths.first().expect("Should have path matches for history items after querying");
            assert_eq!(history_match, &FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/first.rs").into(),
                },
                PathBuf::from(path!("/src/test/first.rs")),
            ));
            assert_eq!(matches.search.len(), 1, "Only one non-history item contains {first_query}, it should be present");
            assert_eq!(matches.search.first().unwrap().as_ref(), rel_path("test/fourth.rs"));
        });

    let second_query = "fsdasdsa";
    let finder = active_file_picker(&workspace, cx);
    finder
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(second_query.to_string(), window, cx)
        })
        .await;
    finder.update(cx, |picker, _| {
        assert!(
            collect_search_matches(picker)
                .search_paths_only()
                .is_empty(),
            "No search entries should match {second_query}"
        );
    });

    let first_query_again = first_query;

    let finder = active_file_picker(&workspace, cx);
    finder
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(first_query_again.to_string(), window, cx)
        })
        .await;
    finder.update(cx, |picker, _| {
            let matches = collect_search_matches(picker);
            assert_eq!(matches.history.len(), 1, "Only one history item contains {first_query_again}, it should be present and others should be filtered out, even after non-matching query");
            let history_match = matches.history_found_paths.first().expect("Should have path matches for history items after querying");
            assert_eq!(history_match, &FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/first.rs").into(),
                },
                PathBuf::from(path!("/src/test/first.rs"))
            ));
            assert_eq!(matches.search.len(), 1, "Only one non-history item contains {first_query_again}, it should be present, even after non-matching query");
            assert_eq!(matches.search.first().unwrap().as_ref(), rel_path("test/fourth.rs"));
        });
}

#[gpui::test]
async fn test_search_sorts_history_items(cx: &mut gpui::TestAppContext) {
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
                "test": {
                    "1_qw": "// First file that matches the query",
                    "2_second": "// Second file",
                    "3_third": "// Third file",
                    "4_fourth": "// Fourth file",
                    "5_qwqwqw": "// A file with 3 more matches than the first one",
                    "6_qwqwqw": "// Same query matches as above, but closer to the end of the list due to the name",
                    "7_qwqwqw": "// One more, same amount of query matches as above",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    // generate some history to select from
    open_close_queried_buffer("1", 1, "1_qw", &workspace, cx).await;
    open_close_queried_buffer("2", 1, "2_second", &workspace, cx).await;
    open_close_queried_buffer("3", 1, "3_third", &workspace, cx).await;
    open_close_queried_buffer("2", 1, "2_second", &workspace, cx).await;
    open_close_queried_buffer("6", 1, "6_qwqwqw", &workspace, cx).await;

    let finder = open_file_picker(&workspace, cx);
    let query = "qw";
    finder
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(query.to_string(), window, cx)
        })
        .await;
    finder.update(cx, |finder, _| {
        let search_matches = collect_search_matches(finder);
        assert_eq!(
            search_matches.history,
            vec![
                rel_path("test/1_qw").into(),
                rel_path("test/6_qwqwqw").into()
            ],
        );
        assert_eq!(
            search_matches.search,
            vec![
                rel_path("test/5_qwqwqw").into(),
                rel_path("test/7_qwqwqw").into()
            ],
        );
    });
}

#[gpui::test]
async fn test_select_current_open_file_when_no_history(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "test": {
                    "1_qw": "",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    // Open new buffer
    open_queried_buffer("1", 1, "1_qw", &workspace, cx).await;

    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_match_selection(finder, 0, "1_qw");
    });
}

#[gpui::test]
async fn test_keep_opened_file_on_top_of_search_results_and_select_next_one(
    cx: &mut TestAppContext,
) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/src"),
            json!({
                "test": {
                    "bar.rs": "// Bar file",
                    "lib.rs": "// Lib file",
                    "maaa.rs": "// Maaaaaaa",
                    "main.rs": "// Main file",
                    "moo.rs": "// Moooooo",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_close_queried_buffer("bar", 1, "bar.rs", &workspace, cx).await;
    open_close_queried_buffer("lib", 1, "lib.rs", &workspace, cx).await;
    open_queried_buffer("main", 1, "main.rs", &workspace, cx).await;

    // main.rs is on top, previously used is selected
    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_selection(finder, 0, "main.rs");
        assert_match_at_position(finder, 1, "lib.rs");
        assert_match_at_position(finder, 2, "bar.rs");
    });

    // all files match, main.rs is still on top, but the second item is selected
    picker
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(".rs".to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 6);
        assert_match_at_position(finder, 0, "main.rs");
        assert_match_selection(finder, 1, "bar.rs");
        assert_match_at_position(finder, 2, "lib.rs");
        assert_match_at_position(finder, 3, "moo.rs");
        assert_match_at_position(finder, 4, "maaa.rs");
        assert_match_at_position(finder, 5, ".rs");
    });

    // main.rs is not among matches, select top item
    picker
        .update_in(cx, |finder, window, cx| {
            finder.delegate.update_matches("b".to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_at_position(finder, 0, "bar.rs");
        assert_match_at_position(finder, 1, "lib.rs");
        assert_match_at_position(finder, 2, "b");
    });

    // main.rs is back, put it on top and select next item
    picker
        .update_in(cx, |finder, window, cx| {
            finder.delegate.update_matches("m".to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 4);
        assert_match_at_position(finder, 0, "main.rs");
        assert_match_selection(finder, 1, "moo.rs");
        assert_match_at_position(finder, 2, "maaa.rs");
        assert_match_at_position(finder, 3, "m");
    });

    // get back to the initial state
    picker
        .update_in(cx, |finder, window, cx| {
            finder.delegate.update_matches("".to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_selection(finder, 0, "main.rs");
        assert_match_at_position(finder, 1, "lib.rs");
        assert_match_at_position(finder, 2, "bar.rs");
    });
}

#[gpui::test]
async fn test_setting_auto_select_first_and_select_active_file(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    cx.update(|cx| {
        let settings = *FileFinderSettings::get_global(cx);

        FileFinderSettings::override_global(
            FileFinderSettings {
                skip_focus_for_active_in_search: false,
                ..settings
            },
            cx,
        );
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/src"),
            json!({
                "test": {
                    "bar.rs": "// Bar file",
                    "lib.rs": "// Lib file",
                    "maaa.rs": "// Maaaaaaa",
                    "main.rs": "// Main file",
                    "moo.rs": "// Moooooo",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_close_queried_buffer("bar", 1, "bar.rs", &workspace, cx).await;
    open_close_queried_buffer("lib", 1, "lib.rs", &workspace, cx).await;
    open_queried_buffer("main", 1, "main.rs", &workspace, cx).await;

    // main.rs is on top, previously used is selected
    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_selection(finder, 0, "main.rs");
        assert_match_at_position(finder, 1, "lib.rs");
        assert_match_at_position(finder, 2, "bar.rs");
    });

    // all files match, main.rs is on top, and is selected
    picker
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches(".rs".to_string(), window, cx)
        })
        .await;
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 6);
        assert_match_selection(finder, 0, "main.rs");
        assert_match_at_position(finder, 1, "bar.rs");
        assert_match_at_position(finder, 2, "lib.rs");
        assert_match_at_position(finder, 3, "moo.rs");
        assert_match_at_position(finder, 4, "maaa.rs");
        assert_match_at_position(finder, 5, ".rs");
    });
}
