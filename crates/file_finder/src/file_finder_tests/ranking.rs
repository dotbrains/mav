use super::*;

#[gpui::test]
async fn test_filename_precedence(cx: &mut TestAppContext) {
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
                "layout": {
                    "app.css": "",
                    "app.d.ts": "",
                    "app.html": "",
                    "+page.svelte": "",
                },
                "routes": {
                    "+layout.svelte": "",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    let (picker, _, cx) = build_find_picker(project, cx);

    simulate_input(cx, "layout");

    picker.update(cx, |finder, _| {
        let search_matches = collect_search_matches(finder).search_paths_only();

        assert_eq!(
            search_matches,
            vec![
                rel_path("routes/+layout.svelte").into(),
                rel_path("layout/app.css").into(),
                rel_path("layout/app.d.ts").into(),
                rel_path("layout/app.html").into(),
                rel_path("layout/+page.svelte").into(),
            ],
            "File with 'layout' in filename should be prioritized over files in 'layout' directory"
        );
    });
}

#[gpui::test]
async fn test_paths_with_starting_slash(cx: &mut TestAppContext) {
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

    let matching_abs_path = "/file1.txt".to_string();
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
            vec![rel_path("a/file1.txt").into()],
            "Relative path starting with slash should match"
        )
    });
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(Confirm);
    cx.read(|cx| {
        let active_editor = workspace.read(cx).active_item_as::<Editor>(cx).unwrap();
        assert_eq!(active_editor.read(cx).title(cx), "file1.txt");
    });
}

#[gpui::test]
async fn test_clear_navigation_history(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/src"),
            json!({
                "test": {
                    "first.rs": "// First file",
                    "second.rs": "// Second file",
                    "third.rs": "// Third file",
                }
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/src").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    workspace.update_in(cx, |_workspace, window, cx| window.focused(cx));

    // Open some files to generate navigation history
    open_close_queried_buffer("fir", 1, "first.rs", &workspace, cx).await;
    open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;
    let history_before_clear =
        open_close_queried_buffer("thi", 1, "third.rs", &workspace, cx).await;

    assert_eq!(
        history_before_clear.len(),
        2,
        "Should have history items before clearing"
    );

    // Verify that file finder shows history items
    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "fir");
    picker.update(cx, |finder, _| {
        let matches = collect_search_matches(finder);
        assert!(
            !matches.history.is_empty(),
            "File finder should show history items before clearing"
        );
    });
    workspace.update_in(cx, |_, window, cx| {
        window.dispatch_action(menu::Cancel.boxed_clone(), cx);
    });

    // Verify navigation state before clear
    workspace.update(cx, |workspace, cx| {
        let pane = workspace.active_pane();
        pane.read(cx).can_navigate_backward()
    });

    // Clear navigation history
    cx.dispatch_action(workspace::ClearNavigationHistory);

    // Verify that navigation is disabled immediately after clear
    workspace.update(cx, |workspace, cx| {
        let pane = workspace.active_pane();
        assert!(
            !pane.read(cx).can_navigate_backward(),
            "Should not be able to navigate backward after clearing history"
        );
        assert!(
            !pane.read(cx).can_navigate_forward(),
            "Should not be able to navigate forward after clearing history"
        );
    });

    // Verify that file finder no longer shows history items
    let picker = open_file_picker(&workspace, cx);
    simulate_input(cx, "fir");
    picker.update(cx, |finder, _| {
        let matches = collect_search_matches(finder);
        assert!(
            matches.history.is_empty(),
            "File finder should not show history items after clearing"
        );
    });
    workspace.update_in(cx, |_, window, cx| {
        window.dispatch_action(menu::Cancel.boxed_clone(), cx);
    });

    // Verify history is empty by opening a new file
    // (this should not show any previous history)
    let history_after_clear =
        open_close_queried_buffer("sec", 1, "second.rs", &workspace, cx).await;
    assert_eq!(
        history_after_clear.len(),
        0,
        "Should have no history items after clearing"
    );
}

#[gpui::test]
async fn test_order_independent_search(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                "internal": {
                    "auth": {
                        "login.rs": "",
                    }
                }
            }),
        )
        .await;
    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let (picker, _, cx) = build_find_picker(project, cx);

    // forward order
    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("auth internal"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker).search_matches_only();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path.as_unix_str(), "internal/auth/login.rs");
    });

    // reverse order should give same result
    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("internal auth"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker).search_matches_only();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].path.as_unix_str(), "internal/auth/login.rs");
    });
}

#[gpui::test]
async fn test_filename_preferred_over_directory_match(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                "crates": {
                    "settings_ui": {
                        "src": {
                            "pages": {
                                "audio_test_window.rs": "",
                                "audio_input_output_setup.rs": "",
                            }
                        }
                    },
                    "audio": {
                        "src": {
                            "audio_settings.rs": "",
                        }
                    }
                }
            }),
        )
        .await;
    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let (picker, _, cx) = build_find_picker(project, cx);

    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("settings audio"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker).search_matches_only();
        assert!(!matches.is_empty(),);
        assert_eq!(
            matches[0].path.as_unix_str(),
            "crates/audio/src/audio_settings.rs"
        );
    });
}

#[gpui::test]
async fn test_start_of_word_preferred_over_scattered_match(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                "crates": {
                    "livekit_client": {
                        "src": {
                            "livekit_client": {
                                "playback.rs": "",
                            }
                        }
                    },
                    "vim": {
                        "test_data": {
                            "test_record_replay_interleaved.json": "",
                        }
                    }
                }
            }),
        )
        .await;
    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let (picker, _, cx) = build_find_picker(project, cx);

    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("live pla"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker).search_matches_only();
        assert!(!matches.is_empty(),);
        assert_eq!(
            matches[0].path.as_unix_str(),
            "crates/livekit_client/src/livekit_client/playback.rs",
        );
    });
}

#[gpui::test]
async fn test_exact_filename_stem_preferred(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                "assets": {
                    "icons": {
                        "file_icons": {
                            "nix.svg": "",
                        }
                    }
                },
                "crates": {
                    "mav": {
                        "resources": {
                            "app-icon-nightly@2x.png": "",
                            "app-icon-preview@2x.png": "",
                        }
                    }
                }
            }),
        )
        .await;
    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let (picker, _, cx) = build_find_picker(project, cx);

    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("nix icon"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker).search_matches_only();
        assert!(!matches.is_empty(),);
        assert_eq!(
            matches[0].path.as_unix_str(),
            "assets/icons/file_icons/nix.svg",
        );
    });
}

#[gpui::test]
async fn test_exact_filename_with_directory_token(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                "crates": {
                    "agent_servers": {
                        "src": {
                            "acp.rs": "",
                            "agent_server.rs": "",
                            "custom.rs": "",
                        }
                    }
                }
            }),
        )
        .await;
    let project = Project::test(app_state.fs.clone(), ["/src".as_ref()], cx).await;
    let (picker, _, cx) = build_find_picker(project, cx);

    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("acp server"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker).search_matches_only();
        assert!(!matches.is_empty(),);
        assert_eq!(
            matches[0].path.as_unix_str(),
            "crates/agent_servers/src/acp.rs",
        );
    });
}
