use super::*;

#[gpui::test]
async fn test_creating_excluded_entries(cx: &mut gpui::TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.worktree.file_scan_exclusions =
                    Some(vec!["excluded_dir".to_string(), "**/.git".to_string()]);
            });
        });
    });

    cx.update(|cx| {
        register_project_item::<TestProjectItemView>(cx);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            ".dockerignore": "",
            ".git": {
                "HEAD": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    select_path(&panel, "root1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root1  <== selected", "      .dockerignore",]
    );
    workspace.update_in(cx, |workspace, _, cx| {
        assert!(
            workspace.active_item(cx).is_none(),
            "Should have no active items in the beginning"
        );
    });

    let excluded_file_path = ".git/COMMIT_EDITMSG";
    let excluded_dir_path = "excluded_dir";

    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text(excluded_file_path, window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..13, cx),
        &["v root1", "      .dockerignore"],
        "Excluded dir should not be shown after opening a file in it"
    );
    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.filename_editor.read(cx).is_focused(window),
            "Should have closed the file name editor"
        );
    });
    workspace.update_in(cx, |workspace, _, cx| {
        let active_entry_path = workspace
            .active_item(cx)
            .expect("should have opened and activated the excluded item")
            .act_as::<TestProjectItemView>(cx)
            .expect("should have opened the corresponding project item for the excluded item")
            .read(cx)
            .path
            .clone();
        assert_eq!(
            active_entry_path.path.as_ref(),
            rel_path(excluded_file_path),
            "Should open the excluded file"
        );

        assert!(
            workspace.notification_ids().is_empty(),
            "Should have no notifications after opening an excluded file"
        );
    });
    assert!(
        fs.is_file(Path::new("/root1/.git/COMMIT_EDITMSG")).await,
        "Should have created the excluded file"
    );

    select_path(&panel, "root1", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text(excluded_file_path, window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..13, cx),
        &["v root1", "      .dockerignore"],
        "Should not change the project panel after trying to create an excluded directorya directory with the same name as the excluded file"
    );
    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.filename_editor.read(cx).is_focused(window),
            "Should have closed the file name editor"
        );
    });
    workspace.update_in(cx, |workspace, _, cx| {
        let notifications = workspace.notification_ids();
        assert_eq!(
            notifications.len(),
            1,
            "Should receive one notification with the error message"
        );
        workspace.dismiss_notification(notifications.first().unwrap(), cx);
        assert!(workspace.notification_ids().is_empty());
    });

    select_path(&panel, "root1", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });

    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text(excluded_dir_path, window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();

    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..13, cx),
        &["v root1", "      .dockerignore"],
        "Should not change the project panel after trying to create an excluded directory"
    );
    panel.update_in(cx, |panel, window, cx| {
        assert!(
            !panel.filename_editor.read(cx).is_focused(window),
            "Should have closed the file name editor"
        );
    });
    workspace.update_in(cx, |workspace, _, cx| {
        let notifications = workspace.notification_ids();
        assert_eq!(
            notifications.len(),
            1,
            "Should receive one notification explaining that no directory is actually shown"
        );
        workspace.dismiss_notification(notifications.first().unwrap(), cx);
        assert!(workspace.notification_ids().is_empty());
    });
    assert!(
        fs.is_dir(Path::new("/root1/excluded_dir")).await,
        "Should have created the excluded directory"
    );
}

#[gpui::test]
async fn test_selection_restored_when_creation_cancelled(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/src",
        json!({
            "test": {
                "first.rs": "// First Rust file",
                "second.rs": "// Second Rust file",
                "third.rs": "// Third Rust file",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/src".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    select_path(&panel, "src", cx);
    panel.update_in(cx, |panel, window, cx| panel.confirm(&Confirm, window, cx));
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src  <== selected",
            "    > test"
        ]
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.executor().run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src",
            "    > [EDITOR: '']  <== selected",
            "    > test"
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.cancel(&menu::Cancel, window, cx);
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src  <== selected",
            "    > test"
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.executor().run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src",
            "    > [EDITOR: '']  <== selected",
            "    > test"
        ]
    );
    workspace.update_in(cx, |_, window, _| window.blur());
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src  <== selected",
            "    > test"
        ]
    );
}
