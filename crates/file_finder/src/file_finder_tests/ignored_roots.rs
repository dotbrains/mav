use super::*;

#[gpui::test]
async fn test_matching_cancellation(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/dir",
            json!({
                "hello": "",
                "goodbye": "",
                "halogen-light": "",
                "happiness": "",
                "height": "",
                "hi": "",
                "hiccup": "",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), ["/dir".as_ref()], cx).await;

    let (picker, _, cx) = build_find_picker(project, cx);

    let query = test_path_position("hi");
    picker
        .update_in(cx, |picker, window, cx| {
            picker.delegate.spawn_search(query.clone(), window, cx)
        })
        .await;

    picker.update(cx, |picker, _cx| {
        // CreateNew option not shown in this case since file already exists
        assert_eq!(picker.delegate.matches.len(), 5);
    });

    picker.update_in(cx, |picker, window, cx| {
        let matches = collect_search_matches(picker).search_matches_only();
        let delegate = &mut picker.delegate;

        // Simulate a search being cancelled after the time limit,
        // returning only a subset of the matches that would have been found.
        drop(delegate.spawn_search(query.clone(), window, cx));
        delegate.set_search_matches(
            delegate.latest_search_id,
            true, // did-cancel
            query.clone(),
            vec![
                ProjectPanelOrdMatch(matches[1].clone()),
                ProjectPanelOrdMatch(matches[3].clone()),
            ],
            cx,
        );

        // Simulate another cancellation.
        drop(delegate.spawn_search(query.clone(), window, cx));
        delegate.set_search_matches(
            delegate.latest_search_id,
            true, // did-cancel
            query.clone(),
            vec![
                ProjectPanelOrdMatch(matches[0].clone()),
                ProjectPanelOrdMatch(matches[2].clone()),
                ProjectPanelOrdMatch(matches[3].clone()),
            ],
            cx,
        );

        assert_eq!(
            collect_search_matches(picker)
                .search_matches_only()
                .as_slice(),
            &matches[0..4]
        );
    });
}

#[gpui::test]
async fn test_ignored_root_with_file_inclusions(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_inclusions = Some(vec![
                    "height_demo/**/hi_bonjour".to_string(),
                    "**/height_1".to_string(),
                ]);
            });
        })
    });
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/ancestor",
            json!({
                ".gitignore": "ignored-root",
                "ignored-root": {
                    "happiness": "",
                    "height": "",
                    "hi": "",
                    "hiccup": "",
                },
                "tracked-root": {
                    ".gitignore": "height*",
                    "happiness": "",
                    "height": "",
                    "heights": {
                        "height_1": "",
                        "height_2": "",
                    },
                    "height_demo": {
                        "test_1": {
                            "hi_bonjour": "hi_bonjour",
                            "hi": "hello",
                        },
                        "hihi": "bye",
                        "test_2": {
                            "hoi": "nl"
                        }
                    },
                    "height_include": {
                        "height_1_include": "",
                        "height_2_include": "",
                    },
                    "hi": "",
                    "hiccup": "",
                },
            }),
        )
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        [
            Path::new(path!("/ancestor/tracked-root")),
            Path::new(path!("/ancestor/ignored-root")),
        ],
        cx,
    )
    .await;
    let (picker, _workspace, cx) = build_find_picker(project, cx);

    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("hi"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(
            matches.search,
            vec![
                rel_path("ignored-root/hi").into(),
                rel_path("tracked-root/hi").into(),
                rel_path("ignored-root/hiccup").into(),
                rel_path("tracked-root/hiccup").into(),
                rel_path("tracked-root/height_demo/test_1/hi_bonjour").into(),
                rel_path("ignored-root/height").into(),
                rel_path("tracked-root/heights/height_1").into(),
                rel_path("ignored-root/happiness").into(),
                rel_path("tracked-root/happiness").into(),
            ],
            "All ignored files that were indexed are found for default ignored mode"
        );
    });
}

#[gpui::test]
async fn test_ignored_root_with_file_inclusions_repro(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_inclusions = Some(vec!["**/.env".to_string()]);
            });
        })
    });
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/src",
            json!({
                ".gitignore": "node_modules",
                "node_modules": {
                    "package.json": "// package.json",
                    ".env": "BAR=FOO"
                },
                ".env": "FOO=BAR"
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [Path::new(path!("/src"))], cx).await;
    let (picker, _workspace, cx) = build_find_picker(project, cx);

    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("json"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(
            matches.search,
            vec![],
            "All ignored files that were indexed are found for default ignored mode"
        );
    });
}

#[gpui::test]
async fn test_ignored_root(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            "/ancestor",
            json!({
                ".gitignore": "ignored-root",
                "ignored-root": {
                    "happiness": "",
                    "height": "",
                    "hi": "",
                    "hiccup": "",
                },
                "tracked-root": {
                    ".gitignore": "height*",
                    "happiness": "",
                    "height": "",
                    "heights": {
                        "height_1": "",
                        "height_2": "",
                    },
                    "hi": "",
                    "hiccup": "",
                },
            }),
        )
        .await;

    let project = Project::test(
        app_state.fs.clone(),
        [
            Path::new(path!("/ancestor/tracked-root")),
            Path::new(path!("/ancestor/ignored-root")),
        ],
        cx,
    )
    .await;
    let (picker, workspace, cx) = build_find_picker(project, cx);

    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("hi"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(
            matches.search,
            vec![
                rel_path("ignored-root/hi").into(),
                rel_path("tracked-root/hi").into(),
                rel_path("ignored-root/hiccup").into(),
                rel_path("tracked-root/hiccup").into(),
                rel_path("ignored-root/height").into(),
                rel_path("ignored-root/happiness").into(),
                rel_path("tracked-root/happiness").into(),
            ],
            "All ignored files that were indexed are found for default ignored mode"
        );
    });
    cx.dispatch_action(ToggleIncludeIgnored);
    picker
        .update_in(cx, |picker, window, cx| {
            picker
                .delegate
                .spawn_search(test_path_position("hi"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(
            matches.search,
            vec![
                rel_path("ignored-root/hi").into(),
                rel_path("tracked-root/hi").into(),
                rel_path("ignored-root/hiccup").into(),
                rel_path("tracked-root/hiccup").into(),
                rel_path("ignored-root/height").into(),
                rel_path("tracked-root/height").into(),
                rel_path("ignored-root/happiness").into(),
                rel_path("tracked-root/happiness").into(),
            ],
            "All ignored files should be found, for the toggled on ignored mode"
        );
    });

    picker
        .update_in(cx, |picker, window, cx| {
            picker.delegate.include_ignored = Some(false);
            picker
                .delegate
                .spawn_search(test_path_position("hi"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(
            matches.search,
            vec![
                rel_path("tracked-root/hi").into(),
                rel_path("tracked-root/hiccup").into(),
                rel_path("tracked-root/happiness").into(),
            ],
            "Only non-ignored files should be found for the turned off ignored mode"
        );
    });

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/ancestor/tracked-root/heights/height_1")),
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
    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.active_pane().update(cx, |pane, cx| {
                pane.close_active_item(&CloseActiveItem::default(), window, cx)
            })
        })
        .await
        .unwrap();
    cx.run_until_parked();

    picker
        .update_in(cx, |picker, window, cx| {
            picker.delegate.include_ignored = None;
            picker
                .delegate
                .spawn_search(test_path_position("hi"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(
            matches.search,
            vec![
                rel_path("ignored-root/hi").into(),
                rel_path("tracked-root/hi").into(),
                rel_path("ignored-root/hiccup").into(),
                rel_path("tracked-root/hiccup").into(),
                rel_path("ignored-root/height").into(),
                rel_path("ignored-root/happiness").into(),
                rel_path("tracked-root/happiness").into(),
            ],
            "Only for the worktree with the ignored root, all indexed ignored files are found in the auto ignored mode"
        );
    });

    picker
        .update_in(cx, |picker, window, cx| {
            picker.delegate.include_ignored = Some(true);
            picker
                .delegate
                .spawn_search(test_path_position("hi"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(
            matches.search,
            vec![
                rel_path("ignored-root/hi").into(),
                rel_path("tracked-root/hi").into(),
                rel_path("ignored-root/hiccup").into(),
                rel_path("tracked-root/hiccup").into(),
                rel_path("ignored-root/height").into(),
                rel_path("tracked-root/height").into(),
                rel_path("tracked-root/heights/height_1").into(),
                rel_path("tracked-root/heights/height_2").into(),
                rel_path("ignored-root/happiness").into(),
                rel_path("tracked-root/happiness").into(),
            ],
            "All ignored files that were indexed are found in the turned on ignored mode"
        );
    });

    picker
        .update_in(cx, |picker, window, cx| {
            picker.delegate.include_ignored = Some(false);
            picker
                .delegate
                .spawn_search(test_path_position("hi"), window, cx)
        })
        .await;
    picker.update(cx, |picker, _| {
        let matches = collect_search_matches(picker);
        assert_eq!(matches.history.len(), 0);
        assert_eq!(
            matches.search,
            vec![
                rel_path("tracked-root/hi").into(),
                rel_path("tracked-root/hiccup").into(),
                rel_path("tracked-root/happiness").into(),
            ],
            "Only non-ignored files should be found for the turned off ignored mode"
        );
    });
}
