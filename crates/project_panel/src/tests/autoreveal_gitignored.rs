use super::*;

#[gpui::test]
async fn test_autoreveal_and_gitignored_files(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(false);
            });
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/project_root",
        json!({
            ".git": {},
            ".gitignore": "**/gitignored_dir",
            "dir_1": {
                "file_1.py": "# File 1_1 contents",
                "file_2.py": "# File 1_2 contents",
                "file_3.py": "# File 1_3 contents",
                "gitignored_dir": {
                    "file_a.py": "# File contents",
                    "file_b.py": "# File contents",
                    "file_c.py": "# File contents",
                },
            },
            "dir_2": {
                "file_1.py": "# File 2_1 contents",
                "file_2.py": "# File 2_2 contents",
                "file_3.py": "# File 2_3 contents",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/project_root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    > dir_1",
            "    > dir_2",
            "      .gitignore",
        ]
    );

    let dir_1_file = find_project_entry(&panel, "project_root/dir_1/file_1.py", cx)
        .expect("dir 1 file is not ignored and should have an entry");
    let dir_2_file = find_project_entry(&panel, "project_root/dir_2/file_1.py", cx)
        .expect("dir 2 file is not ignored and should have an entry");
    let gitignored_dir_file =
        find_project_entry(&panel, "project_root/dir_1/gitignored_dir/file_a.py", cx);
    assert_eq!(
        gitignored_dir_file, None,
        "File in the gitignored dir should not have an entry before its dir is toggled"
    );

    toggle_expand_dir(&panel, "project_root/dir_1", cx);
    toggle_expand_dir(&panel, "project_root/dir_1/gitignored_dir", cx);
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir  <== selected",
            "              file_a.py",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    > dir_2",
            "      .gitignore",
        ],
        "Should show gitignored dir file list in the project panel"
    );
    let gitignored_dir_file =
        find_project_entry(&panel, "project_root/dir_1/gitignored_dir/file_a.py", cx)
            .expect("after gitignored dir got opened, a file entry should be present");

    toggle_expand_dir(&panel, "project_root/dir_1/gitignored_dir", cx);
    toggle_expand_dir(&panel, "project_root/dir_1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    > dir_1  <== selected",
            "    > dir_2",
            "      .gitignore",
        ],
        "Should hide all dir contents again and prepare for the auto reveal test"
    );

    for file_entry in [dir_1_file, dir_2_file, gitignored_dir_file] {
        panel.update(cx, |panel, cx| {
            panel.project.update(cx, |_, cx| {
                cx.emit(project::Event::ActiveEntryChanged(Some(file_entry)))
            })
        });
        cx.run_until_parked();
        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v project_root",
                "    > .git",
                "    > dir_1  <== selected",
                "    > dir_2",
                "      .gitignore",
            ],
            "When no auto reveal is enabled, the selected entry should not be revealed in the project panel"
        );
    }

    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(true)
            });
        })
    });

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(dir_1_file)))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "    > dir_2",
            "      .gitignore",
        ],
        "When auto reveal is enabled, not ignored dir_1 entry should be revealed"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(dir_2_file)))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "When auto reveal is enabled, not ignored dir_2 entry should be revealed"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(
                gitignored_dir_file,
            )))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "When auto reveal is enabled, a gitignored selected entry should not be revealed in the project panel"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::RevealInProjectPanel(gitignored_dir_file))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir",
            "              file_a.py  <== selected  <== marked",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "When a gitignored entry is explicitly revealed, it should be shown in the project tree"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(dir_2_file)))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir",
            "              file_a.py",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py  <== selected  <== marked",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "After switching to dir_2_file, it should be selected and marked"
    );

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(
                gitignored_dir_file,
            )))
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v dir_1",
            "        v gitignored_dir",
            "              file_a.py  <== selected  <== marked",
            "              file_b.py",
            "              file_c.py",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    v dir_2",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "      .gitignore",
        ],
        "When a gitignored entry is already visible, auto reveal should mark it as selected"
    );
}

#[gpui::test]
async fn test_gitignored_and_always_included(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions = Some(Vec::new());
                settings.project.worktree.file_scan_inclusions =
                    Some(vec!["always_included_but_ignored_dir/*".to_string()]);
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(false)
            });
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        "/project_root",
        json!({
            ".git": {},
            ".gitignore": "**/gitignored_dir\n/always_included_but_ignored_dir",
            "dir_1": {
                "file_1.py": "# File 1_1 contents",
                "file_2.py": "# File 1_2 contents",
                "file_3.py": "# File 1_3 contents",
                "gitignored_dir": {
                    "file_a.py": "# File contents",
                    "file_b.py": "# File contents",
                    "file_c.py": "# File contents",
                },
            },
            "dir_2": {
                "file_1.py": "# File 2_1 contents",
                "file_2.py": "# File 2_2 contents",
                "file_3.py": "# File 2_3 contents",
            },
            "always_included_but_ignored_dir": {
                "file_a.py": "# File contents",
                "file_b.py": "# File contents",
                "file_c.py": "# File contents",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/project_root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    > always_included_but_ignored_dir",
            "    > dir_1",
            "    > dir_2",
            "      .gitignore",
        ]
    );

    let gitignored_dir_file =
        find_project_entry(&panel, "project_root/dir_1/gitignored_dir/file_a.py", cx);
    let always_included_but_ignored_dir_file = find_project_entry(
        &panel,
        "project_root/always_included_but_ignored_dir/file_a.py",
        cx,
    )
    .expect("file that is .gitignored but set to always be included should have an entry");
    assert_eq!(
        gitignored_dir_file, None,
        "File in the gitignored dir should not have an entry unless its directory is toggled"
    );

    toggle_expand_dir(&panel, "project_root/dir_1", cx);
    cx.run_until_parked();
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings
                    .project_panel
                    .get_or_insert_default()
                    .auto_reveal_entries = Some(true)
            });
        })
    });

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::ActiveEntryChanged(Some(
                always_included_but_ignored_dir_file,
            )))
        })
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root",
            "    > .git",
            "    v always_included_but_ignored_dir",
            "          file_a.py  <== selected  <== marked",
            "          file_b.py",
            "          file_c.py",
            "    v dir_1",
            "        > gitignored_dir",
            "          file_1.py",
            "          file_2.py",
            "          file_3.py",
            "    > dir_2",
            "      .gitignore",
        ],
        "When auto reveal is enabled, a gitignored but always included selected entry should be revealed in the project panel"
    );
}
