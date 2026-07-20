#[gpui::test]
async fn test_sort_mode_default_fallback(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    // Verify that when sort_mode is not specified, it defaults to DirectoriesFirst
    let default_settings = cx.read(|cx| *ProjectPanelSettings::get_global(cx));
    assert_eq!(
        default_settings.sort_mode,
        settings::ProjectPanelSortMode::DirectoriesFirst,
        "sort_mode should default to DirectoriesFirst"
    );
}

/// Test sort modes: DirectoriesFirst (default) vs Mixed
#[gpui::test]
async fn test_sort_mode_directories_first(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "zebra.txt": "",
            "Apple": {},
            "banana.rs": "",
            "Carrot": {},
            "aardvark.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Default sort mode should be DirectoriesFirst
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "    > Apple",
            "    > Carrot",
            "      aardvark.txt",
            "      banana.rs",
            "      zebra.txt",
        ]
    );
}

#[gpui::test]
async fn test_sort_mode_mixed(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "Zebra.txt": "",
            "apple": {},
            "Banana.rs": "",
            "carrot": {},
            "Aardvark.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Switch to Mixed mode
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().sort_mode =
                    Some(settings::ProjectPanelSortMode::Mixed);
            });
        });
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Mixed mode: case-insensitive sorting
    // Aardvark < apple < Banana < carrot < Zebra (all case-insensitive)
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "      Aardvark.txt",
            "    > apple",
            "      Banana.rs",
            "    > carrot",
            "      Zebra.txt",
        ]
    );
}

#[gpui::test]
async fn test_sort_mode_files_first(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "Zebra.txt": "",
            "apple": {},
            "Banana.rs": "",
            "carrot": {},
            "Aardvark.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Switch to FilesFirst mode
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().sort_mode =
                    Some(settings::ProjectPanelSortMode::FilesFirst);
            });
        });
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // FilesFirst mode: files first, then directories (both case-insensitive)
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "      Aardvark.txt",
            "      Banana.rs",
            "      Zebra.txt",
            "    > apple",
            "    > carrot",
        ]
    );
}

#[gpui::test]
async fn test_sort_mode_toggle(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file2.txt": "",
            "dir1": {},
            "file1.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Initially DirectoriesFirst
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &["v root", "    > dir1", "      file1.txt", "      file2.txt",]
    );

    // Toggle to Mixed
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().sort_mode =
                    Some(settings::ProjectPanelSortMode::Mixed);
            });
        });
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &["v root", "    > dir1", "      file1.txt", "      file2.txt",]
    );

    // Toggle back to DirectoriesFirst
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project_panel.get_or_insert_default().sort_mode =
                    Some(settings::ProjectPanelSortMode::DirectoriesFirst);
            });
        });
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &["v root", "    > dir1", "      file1.txt", "      file2.txt",]
    );
}
