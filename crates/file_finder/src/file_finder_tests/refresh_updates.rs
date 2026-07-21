use super::*;

#[gpui::test]
async fn test_history_items_vs_very_good_external_match(cx: &mut gpui::TestAppContext) {
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
                "collab_ui": {
                    "first.rs": "// First Rust file",
                    "second.rs": "// Second Rust file",
                    "third.rs": "// Third Rust file",
                    "collab_ui.rs": "// Fourth Rust file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    // generate some history to select from
    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;
    open_close_queried_buffer("thi", 1, "third.rs", &workspace, cx).await;
    open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;

    let finder = open_file_picker(&workspace, cx);
    let query = "collab_ui";
    simulate_input(cx, query);
    finder.update(cx, |picker, _| {
            let search_entries = collect_search_matches(picker).search_paths_only();
            assert_eq!(
                search_entries,
                vec![
                    rel_path("collab_ui/collab_ui.rs").into(),
                    rel_path("collab_ui/first.rs").into(),
                    rel_path("collab_ui/third.rs").into(),
                    rel_path("collab_ui/second.rs").into(),
                ],
                "Despite all search results having the same directory name, the most matching one should be on top"
            );
        });
}

#[gpui::test]
async fn test_nonexistent_history_items_not_shown(cx: &mut gpui::TestAppContext) {
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
                    "nonexistent.rs": "// Second Rust file",
                    "third.rs": "// Third Rust file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx)); // generate some history to select from
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    open_close_queried_buffer("non", 1, "nonexistent.rs", &workspace, cx).await;
    open_close_queried_buffer("thi", 1, "third.rs", &workspace, cx).await;
    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    app_state
        .fs
        .remove_file(
            Path::new(path!("/src/test/nonexistent.rs")),
            RemoveOptions::default(),
        )
        .await
        .unwrap();
    cx.run_until_parked();

    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "rs");

    picker.update(cx, |picker, _| {
        assert_eq!(
            collect_search_matches(picker).history,
            vec![
                rel_path("test/first.rs").into(),
                rel_path("test/third.rs").into()
            ],
            "Should have all opened files in the history, except the ones that do not exist on disk"
        );
    });
}

#[gpui::test]
async fn test_search_results_refreshed_on_worktree_updates(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                "lib.rs": "// Lib file",
                "main.rs": "// Bar file",
                "read.me": "// Readme file",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    // Initial state
    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "rs");
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_at_position(finder, 0, "lib.rs");
        assert_match_at_position(finder, 1, "main.rs");
        assert_match_at_position(finder, 2, "rs");
    });
    // Delete main.rs
    app_state
        .fs
        .remove_file("/src/main.rs".as_ref(), Default::default())
        .await
        .expect("unable to remove file");
    cx.executor().advance_clock(FS_WATCH_LATENCY);

    // main.rs is in not among search results anymore
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 2);
        assert_match_at_position(finder, 0, "lib.rs");
        assert_match_at_position(finder, 1, "rs");
    });

    // Create util.rs
    app_state
        .fs
        .create_file("/src/util.rs".as_ref(), Default::default())
        .await
        .expect("unable to create file");
    cx.executor().advance_clock(FS_WATCH_LATENCY);

    // util.rs is among search results
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_at_position(finder, 0, "lib.rs");
        assert_match_at_position(finder, 1, "util.rs");
        assert_match_at_position(finder, 2, "rs");
    });
}

#[gpui::test]
async fn test_search_results_refreshed_on_standalone_file_creation(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                "lib.rs": "// Lib file",
                "main.rs": "// Bar file",
                "read.me": "// Readme file",
            }),
        )
        .await;
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/test",
            json!({
                "new.rs": "// New file",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let window = cx.add_window({
        let project = project.clone();
        |window, cx| MultiWorkspace::test_new(project, window, cx)
    });
    let cx = VisualTestContext::from_window(*window, cx).into_mut();
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    cx.update(|_, cx| {
        open_paths(
            &[PathBuf::from(path!("/test/new.rs"))],
            app_state,
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|_, cx| cx.windows().len()), 1);

    // Verify the standalone file appears as a history match when filtered. Because new.rs IS the
    // currently-open file and skip_focus_for_active_in_search is enabled, confirming would skip
    // it. Close the finder without confirming and use CloseActiveItem to close the file instead.
    let initial_history = {
        let picker = open_file_picker(&workspace, cx);
        simulate_input(cx, "new");
        let history_items = picker.update(cx, |finder, _| {
            assert_eq!(
                finder.delegate.matches.len(),
                2, // 1 history match + 1 CreateNew
                "Unexpected number of matches found for query `new`, matches: {:?}",
                finder.delegate.matches
            );
            let entries = collect_search_matches(finder);
            assert_eq!(entries.history.len(), 1, "new.rs should be a history match");
            assert_eq!(
                entries.search.len(),
                0,
                "new.rs should not be a plain search match"
            );
            finder.delegate.history_items.clone()
        });
        cx.dispatch_action(Cancel);
        history_items
    };
    assert_eq!(
        initial_history.first().unwrap().absolute,
        PathBuf::from(path!("/test/new.rs")),
        "Should show 1st opened item in the history when opening the 2nd item"
    );

    cx.dispatch_action(CloseActiveItem {
        save_intent: None,
        close_pinned: false,
    });
    cx.run_until_parked();

    let history_after_first = open_close_queried_buffer("lib", 1, "lib.rs", &workspace, cx).await;
    assert_eq!(
        history_after_first.first().unwrap().absolute,
        PathBuf::from(path!("/test/new.rs")),
        "Should show 1st opened item in the history when opening the 2nd item"
    );
}

#[gpui::test]
async fn test_search_results_refreshed_on_adding_and_removing_worktrees(
    cx: &mut gpui::TestAppContext,
) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/test",
            json!({
                "project_1": {
                    "bar.rs": "// Bar file",
                    "lib.rs": "// Lib file",
                },
                "project_2": {
                    "Cargo.toml": "// Cargo file",
                    "main.rs": "// Main file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), ["/test/project_1".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_1_id = project.update(cx, |project, cx| {
        let worktree = project.worktrees(cx).last().expect("worktree not found");
        worktree.read(cx).id()
    });

    // Initial state
    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "rs");
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 3);
        assert_match_at_position(finder, 0, "bar.rs");
        assert_match_at_position(finder, 1, "lib.rs");
        assert_match_at_position(finder, 2, "rs");
    });

    // Add new worktree
    project
        .update(cx, |project, cx| {
            project
                .find_or_create_worktree("/test/project_2", true, cx)
                .into_future()
        })
        .await
        .expect("unable to create workdir");
    cx.executor().advance_clock(FS_WATCH_LATENCY);

    // main.rs is among search results
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 4);
        assert_match_at_position(finder, 0, "bar.rs");
        assert_match_at_position(finder, 1, "lib.rs");
        assert_match_at_position(finder, 2, "main.rs");
        assert_match_at_position(finder, 3, "rs");
    });

    // Remove the first worktree
    project.update(cx, |project, cx| {
        project.remove_worktree(worktree_1_id, cx);
    });
    cx.executor().advance_clock(FS_WATCH_LATENCY);

    // Files from the first worktree are not in the search results anymore
    picker.update(cx, |finder, _| {
        assert_eq!(finder.delegate.matches.len(), 2);
        assert_match_at_position(finder, 0, "main.rs");
        assert_match_at_position(finder, 1, "rs");
    });
}
