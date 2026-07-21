use super::*;

#[gpui::test]
async fn test_external_files_history(cx: &mut gpui::TestAppContext) {
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
                }
            }),
        )
        .await;

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/external-src"),
            json!({
                "test": {
                    "third.rs": "// Third Rust file",
                    "fourth.rs": "// Fourth Rust file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    cx.update(|cx| {
        project.update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/external-src"), false, cx)
        })
    })
    .detach();
    cx.background_executor.run_until_parked();

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1,);

        worktrees[0].read(cx).id()
    });
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/external-src/test/third.rs")),
                OpenOptions {
                    visible: Some(OpenVisible::None),
                    ..Default::default()
                },
                window,
                cx,
            )
        })
        .detach();
    cx.background_executor.run_until_parked();
    let external_worktree_id = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        assert_eq!(
            worktrees.len(),
            2,
            "External file should get opened in a new worktree"
        );

        worktrees
            .into_iter()
            .find(|worktree| worktree.read(cx).id() != worktree_id)
            .expect("New worktree should have a different id")
            .read(cx)
            .id()
    });
    cx.dispatch_action(workspace::CloseActiveItem {
        save_intent: None,
        close_pinned: false,
    });

    let initial_history_items =
        open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;
    assert_eq!(
        initial_history_items,
        vec![FoundPath::new(
            ProjectPath {
                worktree_id: external_worktree_id,
                path: rel_path("").into(),
            },
            PathBuf::from(path!("/external-src/test/third.rs"))
        )],
        "Should show external file with its full path in the history after it was open"
    );

    let updated_history_items =
        open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    assert_eq!(
        updated_history_items,
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
                    worktree_id: external_worktree_id,
                    path: rel_path("").into(),
                },
                PathBuf::from(path!("/external-src/test/third.rs"))
            ),
        ],
        "Should keep external file with history updates",
    );
}

#[gpui::test]
async fn test_non_project_file_open_with_filter(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/project"),
            json!({
                "src": {
                    "main.rs": "fn main() {}",
                }
            }),
        )
        .await;

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/external"), json!({ "notes.txt": "some notes" }))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    // Open the external file so it gets a single-file worktree and enters history.
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/external/notes.txt")),
                OpenOptions {
                    visible: Some(OpenVisible::None),
                    ..Default::default()
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

    cx.run_until_parked();

    let finder = open_file_picker(&workspace, cx);
    finder
        .update_in(cx, |f, window, cx| {
            f.delegate
                .spawn_search(test_path_position("notes"), window, cx)
        })
        .await;
    cx.run_until_parked();

    finder.update(cx, |f, _| {
        let entries = collect_search_matches(f);
        assert_eq!(
            entries.search.len(),
            0,
            "External file should appear as a history match, not a search match"
        );
        assert_eq!(
            entries.history.len(),
            1,
            "Expected the external file in history matches"
        );
    });

    // Confirming should open /external/notes.txt without a path-duplication error.
    // Explicitly select index 0: skip_focus_for_active_in_search would otherwise
    // auto-advance past the currently-open file to the CreateNew entry.
    finder.update_in(cx, |f, window, cx| {
        f.delegate.set_selected_index(0, window, cx);
        f.delegate.confirm(false, window, cx);
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let active_editor = workspace
            .read(cx)
            .active_item_as::<Editor>(cx)
            .expect("Should have an active editor after confirming");
        let abs_path = active_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .and_then(|b| b.read(cx).file())
            .map(|f| f.full_path(cx));
        assert_eq!(
            abs_path.as_deref(),
            Some(Path::new(path!("/external/notes.txt"))),
            "Should open /external/notes.txt, not a duplicated path"
        );
    });
}

#[gpui::test]
async fn test_non_project_file_matches_history_with_hidden_root(cx: &mut gpui::TestAppContext) {
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
            path!("/project"),
            json!({
                "src": {
                    "main.rs": "fn main() {}",
                }
            }),
        )
        .await;

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/external"), json!({ "notes.txt": "some notes" }))
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/project").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/external/notes.txt")),
                OpenOptions {
                    visible: Some(OpenVisible::None),
                    ..Default::default()
                },
                window,
                cx,
            )
        })
        .await
        .unwrap();

    cx.run_until_parked();

    let finder = open_file_picker(&workspace, cx);

    finder
        .update_in(cx, |f, window, cx| {
            f.delegate
                .spawn_search(test_path_position("notes"), window, cx)
        })
        .await;
    cx.run_until_parked();

    finder.update(cx, |f, _| {
        let entries = collect_search_matches(f);
        assert_eq!(
            entries.search.len(),
            0,
            "External file should appear as a history match, not a search match"
        );
        assert_eq!(
            entries.history.len(),
            1,
            "Expected the external file in history matches"
        );
    });
}

#[gpui::test]
async fn test_single_file_search_result_split_open(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({ "the-parent-dir": { "the-file": "" } }),
        )
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        [path!("/root/the-parent-dir/the-file").as_ref()],
        cx,
    )
    .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = cx.read(|cx| {
        workspace
            .read(cx)
            .worktrees(cx)
            .next()
            .expect("Expected a single-file worktree")
            .read(cx)
            .id()
    });
    let finder = open_file_picker(&workspace, cx);

    finder
        .update_in(cx, |finder, window, cx| {
            finder
                .delegate
                .spawn_search(test_path_position("thf"), window, cx)
        })
        .await;
    cx.run_until_parked();

    finder.update(cx, |finder, _| {
        let matches = collect_search_matches(finder);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(matches.search.len(), 1);
    });

    cx.dispatch_action(pane::SplitRight::default());
    cx.run_until_parked();

    cx.read(|cx| {
        let active_editor = workspace
            .read(cx)
            .active_item_as::<Editor>(cx)
            .expect("Should have an active editor after splitting the search result");
        assert_eq!(
            active_editor.read(cx).active_project_path(cx),
            Some(ProjectPath {
                worktree_id,
                path: RelPath::empty_arc(),
            }),
            "Should split-open the single-file worktree root with an empty relative path"
        );
    });
}

#[gpui::test]
async fn test_toggle_panel_new_selections(cx: &mut gpui::TestAppContext) {
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

    // generate some history to select from
    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;
    open_close_queried_buffer("thi", 1, "third.rs", &workspace, cx).await;
    let current_history = open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;

    for expected_selected_index in 0..current_history.len() {
        cx.dispatch_action(ToggleFileFinder::default());
        let picker = active_file_picker(&workspace, cx);
        let selected_index = picker.update(cx, |picker, _| picker.delegate.selected_index());
        assert_eq!(
            selected_index, expected_selected_index,
            "Should select the next item in the history"
        );
    }

    cx.dispatch_action(ToggleFileFinder::default());
    let selected_index = workspace.update(cx, |workspace, cx| {
        workspace
            .active_modal::<FileFinder>(cx)
            .unwrap()
            .read(cx)
            .picker
            .read(cx)
            .delegate
            .selected_index()
    });
    assert_eq!(
        selected_index, 0,
        "Should wrap around the history and start all over"
    );
}
