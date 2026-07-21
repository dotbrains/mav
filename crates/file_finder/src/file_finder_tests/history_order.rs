use super::*;

#[gpui::test]
async fn test_non_separate_history_items(cx: &mut TestAppContext) {
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

    cx.dispatch_action(ToggleFileFinder::default());
    let picker = active_file_picker(&workspace, cx);
    // main.rs is on top, previously used is selected
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
        assert_match_selection(finder, 1, "moo.rs");
        assert_match_at_position(finder, 2, "bar.rs");
        assert_match_at_position(finder, 3, "lib.rs");
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
async fn test_history_items_shown_in_order_of_open(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/test"),
            json!({
                "test": {
                    "1.txt": "// One",
                    "2.txt": "// Two",
                    "3.txt": "// Three",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_queried_buffer("1", 1, "1.txt", &workspace, cx).await;
    open_queried_buffer("2", 1, "2.txt", &workspace, cx).await;
    open_queried_buffer("3", 1, "3.txt", &workspace, cx).await;

    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_selection(finder, 0, "3.txt");
        assert_match_at_position(finder, 1, "2.txt");
        assert_match_at_position(finder, 2, "1.txt");
    });

    cx.dispatch_action(SelectNext);
    cx.dispatch_action(Confirm); // Open 2.txt

    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_selection(finder, 0, "2.txt");
        assert_match_at_position(finder, 1, "3.txt");
        assert_match_at_position(finder, 2, "1.txt");
    });

    cx.dispatch_action(SelectNext);
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(Confirm); // Open 1.txt

    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_selection(finder, 0, "1.txt");
        assert_match_at_position(finder, 1, "2.txt");
        assert_match_at_position(finder, 2, "3.txt");
    });
}

#[gpui::test]
async fn test_selected_history_item_stays_selected_on_worktree_updated(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/test"),
            json!({
                "test": {
                    "1.txt": "// One",
                    "2.txt": "// Two",
                    "3.txt": "// Three",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/test").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_close_queried_buffer("1", 1, "1.txt", &workspace, cx).await;
    open_close_queried_buffer("2", 1, "2.txt", &workspace, cx).await;
    open_close_queried_buffer("3", 1, "3.txt", &workspace, cx).await;

    let picker = open_file_picker(&workspace, cx);
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_selection(finder, 0, "3.txt");
        assert_match_at_position(finder, 1, "2.txt");
        assert_match_at_position(finder, 2, "1.txt");
    });

    cx.dispatch_action(SelectNext);

    // Add more files to the worktree to trigger update matches
    for i in 0..5 {
        let filename = if cfg!(windows) {
            format!("C:/test/{}.txt", 4 + i)
        } else {
            format!("/test/{}.txt", 4 + i)
        };
        app_state
            .fs
            .create_file(Path::new(&filename), Default::default())
            .await
            .expect("unable to create file");
    }

    cx.executor().advance_clock(FS_WATCH_LATENCY);

    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_at_position(finder, 0, "3.txt");
        assert_match_selection(finder, 1, "2.txt");
        assert_match_at_position(finder, 2, "1.txt");
    });
}
