use super::*;

#[gpui::test]
async fn test_history_items_uniqueness_for_multiple_worktree_open_all_files(
    cx: &mut TestAppContext,
) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/repo1"),
            json!({
                "package.json": r#"{"name": "repo1"}"#,
                "src": {
                    "index.js": "// Repo 1 index",
                }
            }),
        )
        .await;

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/repo2"),
            json!({
                "package.json": r#"{"name": "repo2"}"#,
                "src": {
                    "index.js": "// Repo 2 index",
                }
            }),
        )
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        [path!("/repo1").as_ref(), path!("/repo2").as_ref()],
        cx,
    )
    .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let (worktree_id1, worktree_id2) = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        (worktrees[0].read(cx).id(), worktrees[1].read(cx).id())
    });

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                ProjectPath {
                    worktree_id: worktree_id1,
                    path: rel_path("package.json").into(),
                },
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();

    cx.dispatch_action(workspace::CloseActiveItem {
        save_intent: None,
        close_pinned: false,
    });
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                ProjectPath {
                    worktree_id: worktree_id2,
                    path: rel_path("package.json").into(),
                },
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap();

    cx.dispatch_action(workspace::CloseActiveItem {
        save_intent: None,
        close_pinned: false,
    });

    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "package.json");

    picker.update(cx, |finder, _| {
        let matches = &finder.delegate.matches.matches;

        assert_eq!(
            matches.len(),
            2,
            "Expected 1 history match + 1 search matches, but got {} matches: {:?}",
            matches.len(),
            matches
        );

        assert_matches!(matches[0], Match::History { .. });

        let search_matches = collect_search_matches(finder);
        assert_eq!(
            search_matches.history.len(),
            2,
            "Should have exactly 2 history match"
        );
        assert_eq!(
            search_matches.search.len(),
            0,
            "Should have exactly 0 search match (because we already opened the 2 package.json)"
        );

        if let Match::History { path, panel_match } = &matches[0] {
            assert_eq!(path.project.worktree_id, worktree_id2);
            assert_eq!(path.project.path.as_ref(), rel_path("package.json"));
            let panel_match = panel_match.as_ref().unwrap();
            assert_eq!(panel_match.0.path_prefix, rel_path("repo2").into());
            assert_eq!(panel_match.0.path, rel_path("package.json").into());
            assert_eq!(
                panel_match.0.positions,
                vec![6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]
            );
        }

        if let Match::History { path, panel_match } = &matches[1] {
            assert_eq!(path.project.worktree_id, worktree_id1);
            assert_eq!(path.project.path.as_ref(), rel_path("package.json"));
            let panel_match = panel_match.as_ref().unwrap();
            assert_eq!(panel_match.0.path_prefix, rel_path("repo1").into());
            assert_eq!(panel_match.0.path, rel_path("package.json").into());
            assert_eq!(
                panel_match.0.positions,
                vec![6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]
            );
        }
    });
}

#[gpui::test]
async fn test_selected_match_stays_selected_after_matches_refreshed(cx: &mut gpui::TestAppContext) {
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

    app_state.fs.as_fake().insert_tree("/src", json!({})).await;

    app_state
        .fs
        .create_dir("/src/even".as_ref())
        .await
        .expect("unable to create dir");

    let initial_files_num = 5;
    for i in 0..initial_files_num {
        let filename = format!("/src/even/file_{}.txt", 10 + i);
        app_state
            .fs
            .create_file(Path::new(&filename), Default::default())
            .await
            .expect("unable to create file");
    }

    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    // Initial state
    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "file");
    let selected_index = 3;
    // Checking only the filename, not the whole path
    let selected_file = format!("file_{}.txt", 10 + selected_index);
    // Select even/file_13.txt
    for _ in 0..selected_index {
        cx.dispatch_action(SelectNext);
    }

    picker.update(cx, |finder, _| {
        assert_match_selection(finder, selected_index, &selected_file)
    });

    // Add more matches to the search results
    let files_to_add = 10;
    for i in 0..files_to_add {
        let filename = format!("/src/file_{}.txt", 20 + i);
        app_state
            .fs
            .create_file(Path::new(&filename), Default::default())
            .await
            .expect("unable to create file");
        // Wait for each file system event to be fully processed before adding the next
        cx.executor().advance_clock(FS_WATCH_LATENCY);
        cx.run_until_parked();
    }

    // file_13.txt is still selected
    picker.update(cx, |finder, _| {
        let expected_selected_index = selected_index + files_to_add;
        assert_match_selection(finder, expected_selected_index, &selected_file);
    });
}

#[gpui::test]
async fn test_first_match_selected_if_previous_one_is_not_in_the_match_list(
    cx: &mut gpui::TestAppContext,
) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                "file_1.txt": "// file_1",
                "file_2.txt": "// file_2",
                "file_3.txt": "// file_3",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    // Initial state
    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "file");
    // Select even/file_2.txt
    cx.dispatch_action(SelectNext);

    // Remove the selected entry
    app_state
        .fs
        .remove_file("/src/file_2.txt".as_ref(), Default::default())
        .await
        .expect("unable to remove file");
    cx.executor().advance_clock(FS_WATCH_LATENCY);

    // file_1.txt is now selected
    picker.update(cx, |finder, _| {
        assert_match_selection(finder, 0, "file_1.txt");
    });
}
