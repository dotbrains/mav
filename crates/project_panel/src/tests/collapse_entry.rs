#[gpui::test]
async fn test_collapse_all_for_entry(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "subdir1": {
                    "nested1": {
                        "file1.txt": "",
                        "file2.txt": ""
                    },
                },
                "subdir2": {
                    "file4.txt": ""
                }
            },
            "dir2": {
                "single_file": {
                    "file5.txt": ""
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Test 1: Basic collapsing
    {
        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "root/dir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1/nested1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir2", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1",
                "        v subdir1",
                "            v nested1",
                "                  file1.txt",
                "                  file2.txt",
                "        v subdir2  <== selected",
                "              file4.txt",
                "    > dir2",
            ],
            "Initial state with everything expanded"
        );

        let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
        panel.update_in(cx, |panel, window, cx| {
            let project = panel.project.read(cx);
            let worktree = project.worktrees(cx).next().unwrap().read(cx);
            panel.collapse_all_for_entry(worktree.id(), entry_id, cx);
            panel.update_visible_entries(None, false, false, window, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &["v root", "    > dir1", "    > dir2",],
            "All subdirs under dir1 should be collapsed"
        );
    }

    // Test 2: With auto-fold enabled
    {
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    auto_fold_dirs: true,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "root/dir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1/nested1", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1",
                "        v subdir1/nested1  <== selected",
                "              file1.txt",
                "              file2.txt",
                "        > subdir2",
                "    > dir2/single_file",
            ],
            "Initial state with some dirs expanded"
        );

        let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
        panel.update(cx, |panel, cx| {
            let project = panel.project.read(cx);
            let worktree = project.worktrees(cx).next().unwrap().read(cx);
            panel.collapse_all_for_entry(worktree.id(), entry_id, cx);
        });

        toggle_expand_dir(&panel, "root/dir1", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1  <== selected",
                "        > subdir1/nested1",
                "        > subdir2",
                "    > dir2/single_file",
            ],
            "Subdirs should be collapsed and folded with auto-fold enabled"
        );
    }

    // Test 3: With auto-fold disabled
    {
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
                    auto_fold_dirs: false,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        toggle_expand_dir(&panel, "root/dir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
        toggle_expand_dir(&panel, "root/dir1/subdir1/nested1", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1",
                "        v subdir1",
                "            v nested1  <== selected",
                "                  file1.txt",
                "                  file2.txt",
                "        > subdir2",
                "    > dir2",
            ],
            "Initial state with some dirs expanded and auto-fold disabled"
        );

        let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
        panel.update(cx, |panel, cx| {
            let project = panel.project.read(cx);
            let worktree = project.worktrees(cx).next().unwrap().read(cx);
            panel.collapse_all_for_entry(worktree.id(), entry_id, cx);
        });

        toggle_expand_dir(&panel, "root/dir1", cx);

        assert_eq!(
            visible_entries_as_strings(&panel, 0..20, cx),
            &[
                "v root",
                "    v dir1  <== selected",
                "        > subdir1",
                "        > subdir2",
                "    > dir2",
            ],
            "Subdirs should be collapsed but not folded with auto-fold disabled"
        );
    }
}
