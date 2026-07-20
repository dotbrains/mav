#[gpui::test]
async fn test_expand_all_for_entry(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".gitignore": "**/ignored_dir\n**/ignored_nested",
            "dir1": {
                "empty1": {
                    "empty2": {
                        "empty3": {
                            "file.txt": ""
                        }
                    }
                },
                "subdir1": {
                    "file1.txt": "",
                    "file2.txt": "",
                    "ignored_nested": {
                        "ignored_file.txt": ""
                    }
                },
                "ignored_dir": {
                    "subdir": {
                        "deep_file.txt": ""
                    }
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

    // Test 1: When auto-fold is enabled
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

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root", "    > dir1", "      .gitignore",],
        "Initial state should show collapsed root structure"
    );

    toggle_expand_dir(&panel, "root/dir1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        > empty1/empty2/empty3",
            "        > ignored_dir",
            "        > subdir1",
            "      .gitignore",
        ],
        "Should show first level with auto-folded dirs and ignored dir visible"
    );

    let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        let project = panel.project.read(cx);
        let worktree = project.worktrees(cx).next().unwrap().read(cx);
        panel.expand_all_for_entry(worktree.id(), entry_id, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        v empty1",
            "            v empty2",
            "                v empty3",
            "                      file.txt",
            "        > ignored_dir",
            "        v subdir1",
            "            > ignored_nested",
            "              file1.txt",
            "              file2.txt",
            "      .gitignore",
        ],
        "After expand_all with auto-fold: should not expand ignored_dir, should expand folded dirs, and should not expand ignored_nested"
    );

    // Test 2: When auto-fold is disabled
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

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx);
    });

    toggle_expand_dir(&panel, "root/dir1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        > empty1",
            "        > ignored_dir",
            "        > subdir1",
            "      .gitignore",
        ],
        "With auto-fold disabled: should show all directories separately"
    );

    let entry_id = find_project_entry(&panel, "root/dir1", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        let project = panel.project.read(cx);
        let worktree = project.worktrees(cx).next().unwrap().read(cx);
        panel.expand_all_for_entry(worktree.id(), entry_id, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        v empty1",
            "            v empty2",
            "                v empty3",
            "                      file.txt",
            "        > ignored_dir",
            "        v subdir1",
            "            > ignored_nested",
            "              file1.txt",
            "              file2.txt",
            "      .gitignore",
        ],
        "After expand_all without auto-fold: should expand all dirs normally, \
         expand ignored_dir itself but not its subdirs, and not expand ignored_nested"
    );

    // Test 3: When explicitly called on ignored directory
    let ignored_dir_entry = find_project_entry(&panel, "root/dir1/ignored_dir", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        let project = panel.project.read(cx);
        let worktree = project.worktrees(cx).next().unwrap().read(cx);
        panel.expand_all_for_entry(worktree.id(), ignored_dir_entry, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        v empty1",
            "            v empty2",
            "                v empty3",
            "                      file.txt",
            "        v ignored_dir",
            "            v subdir",
            "                  deep_file.txt",
            "        v subdir1",
            "            > ignored_nested",
            "              file1.txt",
            "              file2.txt",
            "      .gitignore",
        ],
        "After expand_all on ignored_dir: should expand all contents of the ignored directory"
    );
}
