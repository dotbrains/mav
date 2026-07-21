use super::*;

#[gpui::test]
async fn test_single_file_worktrees(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree("/root", json!({ "the-parent-dir": { "the-file": "" } }))
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        ["/root/the-parent-dir/the-file".as_ref()],
        cx,
    )
    .await;

    let (picker, _, cx) = build_find_picker(project, cx);

    // Even though there is only one worktree, that worktree's filename
    // is included in the matching, because the worktree is a single file.
    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("thf"), window, cx)
        })
        .await;
    cx.read(|cx| {
        let picker = picker.read(cx);
        let delegate = &picker.delegate;
        let matches = collect_search_matches(picker).search_matches_only();
        assert_eq!(matches.len(), 1);

        let (file_name, file_name_positions, full_path, full_path_positions) =
            delegate.labels_for_path_match(&matches[0], PathStyle::local());
        assert_eq!(file_name, "the-file");
        assert_eq!(file_name_positions, &[0, 1, 4]);
        assert_eq!(full_path, "");
        assert_eq!(full_path_positions, &[0; 0]);
    });

    // Since the worktree root is a file, searching for its name followed by a slash does
    // not match anything.
    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("thf/"), window, cx)
        })
        .await;
    picker.update(cx, |f, _| assert_eq!(f.delegate.matches.len(), 0));
}

#[gpui::test]
async fn test_history_items_uniqueness_for_multiple_worktree(cx: &mut TestAppContext) {
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
            1,
            "Should have exactly 1 history match"
        );
        assert_eq!(
            search_matches.search.len(),
            1,
            "Should have exactly 1 search match (the other package.json)"
        );

        if let Match::History { path, .. } = &matches[0] {
            assert_eq!(path.project.worktree_id, worktree_id1);
            assert_eq!(path.project.path.as_ref(), rel_path("package.json"));
        }

        if let Match::Search(path_match) = &matches[1] {
            assert_eq!(
                WorktreeId::from_usize(path_match.0.worktree_id),
                worktree_id2
            );
            assert_eq!(path_match.0.path.as_ref(), rel_path("package.json"));
        }
    });
}

#[gpui::test]
async fn test_create_file_for_multiple_worktrees(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/roota"),
            json!({ "the-parent-dira": { "filea": "" } }),
        )
        .await;

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/rootb"),
            json!({ "the-parent-dirb": { "fileb": "" } }),
        )
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        [path!("/roota").as_ref(), path!("/rootb").as_ref()],
        cx,
    )
    .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let (_worktree_id1, worktree_id2) = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        (worktrees[0].read(cx).id(), worktrees[1].read(cx).id())
    });

    let b_path = ProjectPath {
        worktree_id: worktree_id2,
        path: rel_path("the-parent-dirb/fileb").into(),
    };
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(b_path, None, true, window, cx)
        })
        .await
        .unwrap();

    let finder = open_file_picker(&workspace, cx);

    finder
        .update_in(cx, |f, window, cx| {
            f.delegate.spawn_search(
                test_path_position(path!("the-parent-dirb/filec")),
                window,
                cx,
            )
        })
        .await;
    cx.run_until_parked();
    finder.update_in(cx, |picker, window, cx| {
        assert_eq!(picker.delegate.matches.len(), 1);
        picker.delegate.confirm(false, window, cx)
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        let project_path = active_editor.read(cx).active_project_path(cx);
        assert_eq!(
            project_path,
            Some(ProjectPath {
                worktree_id: worktree_id2,
                path: rel_path("the-parent-dirb/filec").into()
            })
        );
    });
}

#[gpui::test]
async fn test_create_file_focused_file_does_not_belong_to_available_worktrees(
    cx: &mut TestAppContext,
) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/roota"), json!({ "the-parent-dira": { "filea": ""}}))
        .await;

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/rootb"), json!({"the-parent-dirb":{ "fileb": ""}}))
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        [path!("/roota").as_ref(), path!("/rootb").as_ref()],
        cx,
    )
    .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let (worktree_id_a, worktree_id_b) = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        (worktrees[0].read(cx).id(), worktrees[1].read(cx).id())
    });
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/external/external-file.txt")),
                OpenOptions {
                    visible: Some(OpenVisible::None),
                    ..OpenOptions::default()
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
                .spawn_search(test_path_position("new-file.txt"), window, cx)
        })
        .await;

    cx.run_until_parked();
    finder.update_in(cx, |f, window, cx| {
        assert_eq!(f.delegate.matches.len(), 1);
        f.delegate.confirm(false, window, cx); // ✓ works
    });
    cx.run_until_parked();

    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();

        let project_path = active_editor.read(cx).active_project_path(cx);

        assert!(
            project_path.is_some(),
            "Active editor should have a project path"
        );

        let project_path = project_path.unwrap();

        assert!(
            project_path.worktree_id == worktree_id_a || project_path.worktree_id == worktree_id_b,
            "New file should be created in one of the available worktrees (A or B), \
                not in a directory derived from the external file. Got worktree_id: {:?}",
            project_path.worktree_id
        );

        assert_eq!(project_path.path.as_ref(), rel_path("new-file.txt"));
    });
}

#[gpui::test]
async fn test_create_file_no_focused_with_multiple_worktrees(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/roota"),
            json!({ "the-parent-dira": { "filea": "" } }),
        )
        .await;

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/rootb"),
            json!({ "the-parent-dirb": { "fileb": "" } }),
        )
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        [path!("/roota").as_ref(), path!("/rootb").as_ref()],
        cx,
    )
    .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let (_worktree_id1, worktree_id2) = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        (worktrees[0].read(cx).id(), worktrees[1].read(cx).id())
    });

    let finder = open_file_picker(&workspace, cx);

    finder
        .update_in(cx, |f, window, cx| {
            f.delegate
                .spawn_search(test_path_position(path!("rootb/filec")), window, cx)
        })
        .await;
    cx.run_until_parked();
    finder.update_in(cx, |picker, window, cx| {
        assert_eq!(picker.delegate.matches.len(), 1);
        picker.delegate.confirm(false, window, cx)
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        let project_path = active_editor.read(cx).active_project_path(cx);
        assert_eq!(
            project_path,
            Some(ProjectPath {
                worktree_id: worktree_id2,
                path: rel_path("filec").into()
            })
        );
    });
}

#[gpui::test]
async fn test_path_distance_ordering(cx: &mut TestAppContext) {
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
                "dir1": { "a.txt": "" },
                "dir2": {
                    "a.txt": "",
                    "b.txt": ""
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let worktree_id = cx.read(|cx| {
        let worktrees = workspace.read(cx).worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees[0].read(cx).id()
    });

    // When workspace has an active item, sort items which are closer to that item
    // first when they have the same name. In this case, b.txt is closer to dir2's a.txt
    // so that one should be sorted earlier
    let b_path = ProjectPath {
        worktree_id,
        path: rel_path("dir2/b.txt").into(),
    };
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(b_path, None, true, window, cx)
        })
        .await
        .unwrap();
    let finder = open_file_picker(&workspace, cx);
    finder
        .update_in(cx, |f, window, cx| {
            f.delegate
                .spawn_search(test_path_position("a.txt"), window, cx)
        })
        .await;

    finder.update(cx, |picker, _| {
        let matches = collect_search_matches(picker).search_paths_only();
        assert_eq!(matches[0].as_ref(), rel_path("dir2/a.txt"));
        assert_eq!(matches[1].as_ref(), rel_path("dir1/a.txt"));
    });
}

#[gpui::test]
async fn test_search_worktree_without_files(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/root",
            json!({
                "dir1": {},
                "dir2": {
                    "dir3": {}
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), ["/root".as_ref()], cx).await;
    let (picker, _workspace, cx) = build_find_picker(project, cx);

    picker
        .update_in(cx, |f, window, cx| {
            f.delegate
                .spawn_search(test_path_position("dir"), window, cx)
        })
        .await;
    cx.read(|cx| {
        let finder = picker.read(cx);
        assert_eq!(finder.delegate.matches.len(), 1);
        assert_match_at_position(finder, 0, "dir");
    });
}
