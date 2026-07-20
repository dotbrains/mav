use super::*;

#[gpui::test]
async fn test_exclusions_in_visible_list(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions =
                    Some(vec!["**/.git".to_string(), "**/4/**".to_string()]);
            });
        });
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/root1",
        json!({
            ".dockerignore": "",
            ".git": {
                "HEAD": "",
            },
            "a": {
                "0": { "q": "", "r": "", "s": "" },
                "1": { "t": "", "u": "" },
                "2": { "v": "", "w": "", "x": "", "y": "" },
            },
            "b": {
                "3": { "Q": "" },
                "4": { "R": "", "S": "", "T": "", "U": "" },
            },
            "C": {
                "5": {},
                "6": { "V": "", "W": "" },
                "7": { "X": "" },
                "8": { "Y": {}, "Z": "" }
            }
        }),
    )
    .await;
    fs.insert_tree(
        "/root2",
        json!({
            "d": {
                "4": ""
            },
            "e": {}
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root1",
            "    > a",
            "    > b",
            "    > C",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    toggle_expand_dir(&panel, "root1/b", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root1",
            "    > a",
            "    v b  <== selected",
            "        > 3",
            "    > C",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    toggle_expand_dir(&panel, "root2/d", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root1",
            "    > a",
            "    v b",
            "        > 3",
            "    > C",
            "      .dockerignore",
            "v root2",
            "    v d  <== selected",
            "    > e",
        ]
    );

    toggle_expand_dir(&panel, "root2/e", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root1",
            "    > a",
            "    v b",
            "        > 3",
            "    > C",
            "      .dockerignore",
            "v root2",
            "    v d",
            "    v e  <== selected",
        ]
    );
}

#[gpui::test]
async fn test_auto_collapse_dir_paths(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root1"),
        json!({
            "dir_1": {
                "nested_dir_1": {
                    "nested_dir_2": {
                        "nested_dir_3": {
                            "file_a.java": "// File contents",
                            "file_b.java": "// File contents",
                            "file_c.java": "// File contents",
                            "nested_dir_4": {
                                "nested_dir_5": {
                                    "file_d.java": "// File contents",
                                }
                            }
                        }
                    }
                }
            }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/root2"),
        json!({
            "dir_2": {
                "file_1.java": "// File contents",
            }
        }),
    )
    .await;

    // Test 1: Multiple worktrees with auto_fold_dirs = true
    let project = Project::test(
        fs.clone(),
        [path!("/root1").as_ref(), path!("/root2").as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_fold_dirs: true,
                sort_mode: settings::ProjectPanelSortMode::DirectoriesFirst,
                ..settings
            },
            cx,
        );
    });
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > dir_1/nested_dir_1/nested_dir_2/nested_dir_3",
            "v root2",
            "    > dir_2",
        ]
    );

    toggle_expand_dir(
        &panel,
        "root1/dir_1/nested_dir_1/nested_dir_2/nested_dir_3",
        cx,
    );
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    v dir_1/nested_dir_1/nested_dir_2/nested_dir_3  <== selected",
            "        > nested_dir_4/nested_dir_5",
            "          file_a.java",
            "          file_b.java",
            "          file_c.java",
            "v root2",
            "    > dir_2",
        ]
    );

    toggle_expand_dir(
        &panel,
        "root1/dir_1/nested_dir_1/nested_dir_2/nested_dir_3/nested_dir_4/nested_dir_5",
        cx,
    );
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    v dir_1/nested_dir_1/nested_dir_2/nested_dir_3",
            "        v nested_dir_4/nested_dir_5  <== selected",
            "              file_d.java",
            "          file_a.java",
            "          file_b.java",
            "          file_c.java",
            "v root2",
            "    > dir_2",
        ]
    );
    toggle_expand_dir(&panel, "root2/dir_2", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    v dir_1/nested_dir_1/nested_dir_2/nested_dir_3",
            "        v nested_dir_4/nested_dir_5",
            "              file_d.java",
            "          file_a.java",
            "          file_b.java",
            "          file_c.java",
            "v root2",
            "    v dir_2  <== selected",
            "          file_1.java",
        ]
    );

    // Test 2: Single worktree with auto_fold_dirs = true and hide_root = true
    {
        let project = Project::test(fs.clone(), [path!("/root1").as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    auto_fold_dirs: true,
                    hide_root: true,
                    ..settings
                },
                cx,
            );
        });
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &["> dir_1/nested_dir_1/nested_dir_2/nested_dir_3"],
            "Single worktree with hide_root=true should hide root and show auto-folded paths"
        );

        toggle_expand_dir(
            &panel,
            "root1/dir_1/nested_dir_1/nested_dir_2/nested_dir_3",
            cx,
        );
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v dir_1/nested_dir_1/nested_dir_2/nested_dir_3  <== selected",
                "    > nested_dir_4/nested_dir_5",
                "      file_a.java",
                "      file_b.java",
                "      file_c.java",
            ],
            "Expanded auto-folded path with hidden root should show contents without root prefix"
        );

        toggle_expand_dir(
            &panel,
            "root1/dir_1/nested_dir_1/nested_dir_2/nested_dir_3/nested_dir_4/nested_dir_5",
            cx,
        );
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v dir_1/nested_dir_1/nested_dir_2/nested_dir_3",
                "    v nested_dir_4/nested_dir_5  <== selected",
                "          file_d.java",
                "      file_a.java",
                "      file_b.java",
                "      file_c.java",
            ],
            "Nested expansion with hidden root should maintain proper indentation"
        );
    }
}
