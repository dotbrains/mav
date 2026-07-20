#[gpui::test]
async fn test_hide_root(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "dir1": {
                "file1.txt": "content",
                "file2.txt": "content",
            },
            "dir2": {
                "file3.txt": "content",
            },
            "file4.txt": "content",
        }),
    )
    .await;

    fs.insert_tree(
        "/root2",
        json!({
            "dir3": {
                "file5.txt": "content",
            },
            "file6.txt": "content",
        }),
    )
    .await;

    // Test 1: Single worktree with hide_root = false
    {
        let project = Project::test(fs.clone(), ["/root1".as_ref()], cx).await;
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
                    hide_root: false,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        #[rustfmt::skip]
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v root1",
                "    > dir1",
                "    > dir2",
                "      file4.txt",
            ],
            "With hide_root=false and single worktree, root should be visible"
        );
    }

    // Test 2: Single worktree with hide_root = true
    {
        let project = Project::test(fs.clone(), ["/root1".as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window.into(), cx);

        // Set hide_root to true
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
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
            &["> dir1", "> dir2", "  file4.txt",],
            "With hide_root=true and single worktree, root should be hidden"
        );

        // Test expanding directories still works without root
        toggle_expand_dir(&panel, "root1/dir1", cx);
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v dir1  <== selected",
                "      file1.txt",
                "      file2.txt",
                "> dir2",
                "  file4.txt",
            ],
            "Should be able to expand directories even when root is hidden"
        );
    }

    // Test 3: Multiple worktrees with hide_root = true
    {
        let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
        let window =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window.into(), cx);

        // Set hide_root to true
        cx.update(|_, cx| {
            let settings = *ProjectPanelSettings::get_global(cx);
            ProjectPanelSettings::override_global(
                ProjectPanelSettings {
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
            &[
                "v root1",
                "    > dir1",
                "    > dir2",
                "      file4.txt",
                "v root2",
                "    > dir3",
                "      file6.txt",
            ],
            "With hide_root=true and multiple worktrees, roots should still be visible"
        );
    }

    // Test 4: Multiple worktrees with hide_root = false
    {
        let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
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
                    hide_root: false,
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
                "    > dir1",
                "    > dir2",
                "      file4.txt",
                "v root2",
                "    > dir3",
                "      file6.txt",
            ],
            "With hide_root=false and multiple worktrees, roots should be visible"
        );
    }
}
