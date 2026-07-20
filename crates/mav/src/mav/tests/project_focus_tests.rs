use super::*;

#[gpui::test]
async fn test_opening_project_settings_when_excluded(cx: &mut gpui::TestAppContext) {
    // Use the proper initialization for runtime state
    let app_state = init_keymap_test(cx);

    eprintln!("Running test_opening_project_settings_when_excluded");

    // 1. Set up a project with some project settings
    let settings_init =
        r#"{ "UNIQUEVALUE": true, "git": { "inline_blame": { "enabled": false } } }"#;
    app_state
        .fs
        .as_fake()
        .insert_tree(
            Path::new("/root"),
            json!({
                ".mav": {
                    "settings.json": settings_init
                }
            }),
        )
        .await;

    eprintln!("Created project with .mav/settings.json containing UNIQUEVALUE");

    // 2. Create a project with the file system and load it
    let project = Project::test(app_state.fs.clone(), [Path::new("/root")], cx).await;

    // Save original settings content for comparison
    let original_settings = app_state
        .fs
        .load(Path::new("/root/.mav/settings.json"))
        .await
        .unwrap();

    let original_settings_str = original_settings.clone();

    // Verify settings exist on disk and have expected content
    eprintln!("Original settings content: {}", original_settings_str);
    assert!(
        original_settings_str.contains("UNIQUEVALUE"),
        "Test setup failed - settings file doesn't contain our marker"
    );

    // 3. Add .mav to file scan exclusions in user settings
    cx.update_global::<SettingsStore, _>(|store, cx| {
        store.update_user_settings(cx, |worktree_settings| {
            worktree_settings.project.worktree.file_scan_exclusions =
                Some(vec![".mav".to_string()]);
        });
    });

    eprintln!("Added .mav to file_scan_exclusions in settings");

    // 4. Run tasks to apply settings
    cx.background_executor.run_until_parked();

    // 5. Critical: Verify .mav is actually excluded from worktree
    let worktree = cx.update(|cx| project.read(cx).worktrees(cx).next().unwrap());

    let has_mav_entry =
        cx.update(|cx| worktree.read(cx).entry_for_path(rel_path(".mav")).is_some());

    eprintln!(
        "Is .mav directory visible in worktree after exclusion: {}",
        has_mav_entry
    );

    // This assertion verifies the test is set up correctly to show the bug
    // If .mav is not excluded, the test will fail here
    assert!(
        !has_mav_entry,
        "Test precondition failed: .mav directory should be excluded but was found in worktree"
    );

    // 6. Create workspace and trigger the actual function that causes the bug
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                // Call the exact function that contains the bug
                eprintln!("About to call open_project_settings_file");
                open_project_settings_file(workspace, &OpenProjectSettingsFile, window, cx);
            });
        })
        .unwrap();

    // 7. Run background tasks until completion
    cx.background_executor.run_until_parked();

    // 8. Verify file contents after calling function
    let new_content = app_state
        .fs
        .load(Path::new("/root/.mav/settings.json"))
        .await
        .unwrap();

    let new_content_str = new_content;
    eprintln!("New settings content: {}", new_content_str);

    // The bug causes the settings to be overwritten with empty settings
    // So if the unique value is no longer present, the bug has been reproduced
    let bug_exists = !new_content_str.contains("UNIQUEVALUE");
    eprintln!("Bug reproduced: {}", bug_exists);

    // This assertion should fail if the bug exists - showing the bug is real
    assert!(
        new_content_str.contains("UNIQUEVALUE"),
        "BUG FOUND: Project settings were overwritten when opening via command - original custom content was lost"
    );
}

#[gpui::test]
async fn test_disable_ai_crash(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);
    cx.update(init);
    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let _window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));

    cx.run_until_parked();

    cx.update(|cx| {
        SettingsStore::update_global(cx, |settings_store, cx| {
            settings_store.update_user_settings(cx, |settings| {
                settings.project.disable_ai = Some(SaturatingBool(true));
            });
        });
    });

    cx.run_until_parked();

    // If this panics, the test has failed
}

#[gpui::test]
async fn test_prefer_focused_window(cx: &mut gpui::TestAppContext) {
    let app_state = init_test(cx);
    let paths = [PathBuf::from(path!("/dir/document.txt"))];

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/dir"),
            json!({
                "document.txt": "Some of the documentation's content."
            }),
        )
        .await;

    let project_a = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window_a = cx.add_window({
        let project = project_a.clone();
        |window, cx| MultiWorkspace::test_new(project, window, cx)
    });

    let project_b = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window_b = cx.add_window({
        let project = project_b.clone();
        |window, cx| MultiWorkspace::test_new(project, window, cx)
    });

    let project_c = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window_c = cx.add_window({
        let project = project_c.clone();
        |window, cx| MultiWorkspace::test_new(project, window, cx)
    });

    for window in [window_a, window_b, window_c] {
        let _ = cx.update_window(*window, |_, window, _| {
            window.activate_window();
        });

        cx.update(|cx| {
            let open_options = OpenOptions {
                wait: true,
                ..Default::default()
            };

            workspace::open_paths(&paths, app_state.clone(), open_options, cx)
        })
        .await
        .unwrap();

        cx.update_window(*window, |_, window, _| assert!(window.is_window_active()))
            .unwrap();

        let _ = window.read_with(cx, |multi_workspace, cx| {
            let pane = multi_workspace.workspace().read(cx).active_pane().read(cx);
            let project_path = pane.active_item().unwrap().project_path(cx).unwrap();

            assert_eq!(
                project_path.path.as_ref().as_std_path().to_str().unwrap(),
                path!("document.txt")
            )
        });
    }
}
