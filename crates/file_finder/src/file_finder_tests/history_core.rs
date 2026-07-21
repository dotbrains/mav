use super::*;

#[gpui::test]
async fn test_query_history(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);

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
        assert_eq!(worktrees.len(), 1);
        worktrees[0].read(cx).id()
    });

    // Open and close panels, getting their history items afterwards.
    // Ensure history items get populated with opened items, and items are kept in a certain order.
    // The history lags one opened buffer behind, since it's updated in the search panel only on its reopen.
    //
    // TODO: without closing, the opened items do not propagate their history changes for some reason
    // it does work in real app though, only tests do not propagate.
    workspace.update_in(cx, |_workspace, window, cx| window.focused(cx));

    let initial_history = open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    assert!(
        initial_history.is_empty(),
        "Should have no history before opening any files"
    );

    let history_after_first =
        open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;
    assert_eq!(
        history_after_first,
        vec![FoundPath::new(
            ProjectPath {
                worktree_id,
                path: rel_path("test/first.rs").into(),
            },
            PathBuf::from(path!("/src/test/first.rs"))
        )],
        "Should show 1st opened item in the history when opening the 2nd item"
    );

    let history_after_second =
        open_close_queried_buffer("thi", 1, "third.rs", &workspace, cx).await;
    assert_eq!(
        history_after_second,
        vec![
            FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/second.rs").into(),
                },
                PathBuf::from(path!("/src/test/second.rs"))
            ),
            FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/first.rs").into(),
                },
                PathBuf::from(path!("/src/test/first.rs"))
            ),
        ],
        "Should show 1st and 2nd opened items in the history when opening the 3rd item. \
    2nd item should be the first in the history, as the last opened."
    );

    let history_after_third =
        open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;
    assert_eq!(
        history_after_third,
        vec![
            FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/third.rs").into(),
                },
                PathBuf::from(path!("/src/test/third.rs"))
            ),
            FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/second.rs").into(),
                },
                PathBuf::from(path!("/src/test/second.rs"))
            ),
            FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/first.rs").into(),
                },
                PathBuf::from(path!("/src/test/first.rs"))
            ),
        ],
        "Should show 1st, 2nd and 3rd opened items in the history when opening the 2nd item again. \
    3rd item should be the first in the history, as the last opened."
    );

    let history_after_second_again =
        open_close_queried_buffer("thi", 1, "third.rs", &workspace, cx).await;
    assert_eq!(
        history_after_second_again,
        vec![
            FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/second.rs").into(),
                },
                PathBuf::from(path!("/src/test/second.rs"))
            ),
            FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/third.rs").into(),
                },
                PathBuf::from(path!("/src/test/third.rs"))
            ),
            FoundPath::new(
                ProjectPath {
                    worktree_id,
                    path: rel_path("test/first.rs").into(),
                },
                PathBuf::from(path!("/src/test/first.rs"))
            ),
        ],
        "Should show 1st, 2nd and 3rd opened items in the history when opening the 3rd item again. \
    2nd item, as the last opened, 3rd item should go next as it was opened right before."
    );
}

#[gpui::test]
async fn test_history_match_positions(cx: &mut gpui::TestAppContext) {
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
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    workspace.update_in(cx, |_workspace, window, cx| window.focused(cx));

    open_close_queried_buffer("efir", 1, "first.rs", &workspace, cx).await;
    let history = open_close_queried_buffer("second", 1, "second.rs", &workspace, cx).await;
    assert_eq!(history.len(), 1);

    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "fir");
    picker.update_in(cx, |finder, window, cx| {
        let matches = &finder.delegate.matches.matches;
        assert_matches!(
            matches.as_slice(),
            [Match::History { .. }, Match::CreateNew { .. }]
        );
        assert_eq!(
            matches[0].panel_match().unwrap().0.path.as_ref(),
            rel_path("test/first.rs")
        );
        assert_eq!(matches[0].panel_match().unwrap().0.positions, &[5, 6, 7]);

        let (file_label, path_label) =
            finder
                .delegate
                .labels_for_match(&finder.delegate.matches.matches[0], window, cx);
        assert_eq!(file_label.text(), "first.rs");
        assert_eq!(file_label.highlight_indices(), &[0, 1, 2]);
        assert_eq!(
            path_label.text(),
            format!("test{}", PathStyle::local().primary_separator())
        );
        assert_eq!(path_label.highlight_indices(), &[] as &[usize]);
    });
}

#[gpui::test]
async fn test_history_labels_do_not_include_worktree_root_name(cx: &mut gpui::TestAppContext) {
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
            path!("/my_project"),
            json!({
                "src": {
                    "first.rs": "// First Rust file",
                    "second.rs": "// Second Rust file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/my_project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;

    let picker = open_file_picker(&workspace, cx);
    picker.update_in(cx, |finder, window, cx| {
        let matches = &finder.delegate.matches.matches;
        assert!(matches.len() >= 2);

        for m in matches.iter() {
            if let Match::History { panel_match, .. } = m {
                assert!(
                    panel_match.is_none(),
                    "History items with no query should not have a panel match"
                );
            }
        }

        let separator = PathStyle::local().primary_separator();

        let (file_label, path_label) = finder.delegate.labels_for_match(&matches[0], window, cx);
        assert_eq!(file_label.text(), "second.rs");
        assert_eq!(
            path_label.text(),
            format!("src{separator}"),
            "History path label must not contain root name 'my_project'"
        );

        let (file_label, path_label) = finder.delegate.labels_for_match(&matches[1], window, cx);
        assert_eq!(file_label.text(), "first.rs");
        assert_eq!(
            path_label.text(),
            format!("src{separator}"),
            "History path label must not contain root name 'my_project'"
        );
    });

    // Now type a query so history items get panel_match populated,
    // and verify labels stay consistent with the no-query case.
    let picker = active_file_picker(&workspace, cx);
    picker
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .update_matches("first".to_string(), window, cx)
        })
        .await;
    picker.update_in(cx, |finder, window, cx| {
        let matches = &finder.delegate.matches.matches;
        let history_match = matches
            .iter()
            .find(|m| matches!(m, Match::History { .. }))
            .expect("Should have a history match for 'first'");

        let (file_label, path_label) = finder.delegate.labels_for_match(history_match, window, cx);
        assert_eq!(file_label.text(), "first.rs");
        let separator = PathStyle::local().primary_separator();
        assert_eq!(
            path_label.text(),
            format!("src{separator}"),
            "Queried history path label must not contain root name 'my_project'"
        );
    });
}

#[gpui::test]
async fn test_history_labels_include_worktree_root_name_when_hide_root_false(
    cx: &mut gpui::TestAppContext,
) {
    let app_state = init_test(cx);

    cx.update(|cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_root: false,
                ..settings
            },
            cx,
        );
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/my_project"),
            json!({
                "src": {
                    "first.rs": "// First Rust file",
                    "second.rs": "// Second Rust file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/my_project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;

    let picker = open_file_picker(&workspace, cx);
    picker.update_in(cx, |finder, window, cx| {
        let matches = &finder.delegate.matches.matches;
        let separator = PathStyle::local().primary_separator();

        let (_file_label, path_label) = finder.delegate.labels_for_match(&matches[0], window, cx);
        assert_eq!(
            path_label.text(),
            format!("my_project{separator}src{separator}"),
            "With hide_root=false, history path label should include root name 'my_project'"
        );
    });
}

#[gpui::test]
async fn test_history_labels_include_worktree_root_name_when_hide_root_true_and_multiple_folders(
    cx: &mut gpui::TestAppContext,
) {
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
            path!("/my_project"),
            json!({
                "src": {
                    "first.rs": "// First Rust file",
                    "second.rs": "// Second Rust file",
                }
            }),
        )
        .await;

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/my_second_project"),
            json!({
                "src": {
                    "third.rs": "// Third Rust file",
                    "fourth.rs": "// Fourth Rust file",
                }
            }),
        )
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        [
            path!("/my_project").as_ref(),
            path!("/my_second_project").as_ref(),
        ],
        cx,
    )
    .await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    open_close_queried_buffer("thi", 1, "third.rs", &workspace, cx).await;

    let picker = open_file_picker(&workspace, cx);
    picker.update_in(cx, |finder, window, cx| {
        let matches = &finder.delegate.matches.matches;
        assert!(matches.len() >= 2, "Should have at least 2 history matches");

        let separator = PathStyle::local().primary_separator();

        let first_match = matches
            .iter()
            .find(|m| {
                if let Match::History { path, .. } = m {
                    path.project.path.file_name()
                        .map(|n| n.to_string())
                        .map_or(false, |name| name == "first.rs")
                } else {
                    false
                }
            })
            .expect("Should have history match for first.rs");

        let third_match = matches
            .iter()
            .find(|m| {
                if let Match::History { path, .. } = m {
                    path.project.path.file_name()
                        .map(|n| n.to_string())
                        .map_or(false, |name| name == "third.rs")
                } else {
                    false
                }
            })
            .expect("Should have history match for third.rs");

        let (_file_label, path_label) =
            finder.delegate.labels_for_match(first_match, window, cx);
        assert_eq!(
            path_label.text(),
            format!("my_project{separator}src{separator}"),
            "With hide_root=true and multiple folders, history path label should include root name 'my_project'"
        );

        let (_file_label, path_label) =
            finder.delegate.labels_for_match(third_match, window, cx);
        assert_eq!(
            path_label.text(),
            format!("my_second_project{separator}src{separator}"),
            "With hide_root=true and multiple folders, history path label should include root name 'my_second_project'"
        );
    });
}
