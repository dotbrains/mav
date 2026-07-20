#[gpui::test]
async fn test_create_entries_without_selection(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "file1.txt": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
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

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1",
        ],
        "Initial state with nothing selected"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text("hello_from_no_selections", window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1",
            "      hello_from_no_selections  <== selected  <== marked",
        ],
        "A new file is created under the root directory"
    );
}

#[gpui::test]
async fn test_create_entries_without_selection_hide_root(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "existing_dir": {
                "existing_file.txt": "",
            },
            "existing_file.txt": "",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

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

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> existing_dir",
            "  existing_file.txt",
        ],
        "Initial state with hide_root=true, root should be hidden and nothing selected"
    );

    panel.update(cx, |panel, _| {
        assert!(
            panel.selection.is_none(),
            "Should have no selection initially"
        );
    });

    // Test 1: Create new file when no entry is selected
    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    cx.run_until_parked();
    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> existing_dir",
            "  [EDITOR: '']  <== selected",
            "  existing_file.txt",
        ],
        "Editor should appear at root level when hide_root=true and no selection"
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("new_file_at_root.txt", window, cx)
        });
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> existing_dir",
            "  existing_file.txt",
            "  new_file_at_root.txt  <== selected  <== marked",
        ],
        "New file should be created at root level and visible without root prefix"
    );

    assert!(
        fs.is_file(Path::new("/root/new_file_at_root.txt")).await,
        "File should be created in the actual root directory"
    );

    // Test 2: Create new directory when no entry is selected
    panel.update(cx, |panel, _| {
        panel.selection = None;
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx);
    });
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> [EDITOR: '']  <== selected",
            "> existing_dir",
            "  existing_file.txt",
            "  new_file_at_root.txt",
        ],
        "Directory editor should appear at root level when hide_root=true and no selection"
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("new_dir_at_root", window, cx)
        });
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> existing_dir",
            "v new_dir_at_root  <== selected",
            "  existing_file.txt",
            "  new_file_at_root.txt",
        ],
        "New directory should be created at root level and visible without root prefix"
    );

    assert!(
        fs.is_dir(Path::new("/root/new_dir_at_root")).await,
        "Directory should be created in the actual root directory"
    );
}

#[gpui::test]
async fn test_context_menu_new_file_in_empty_hidden_root(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

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

    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    assert!(
        visible_entries_as_strings(&panel, 0..20, cx).is_empty(),
        "Empty worktree with hide_root=true should render no entries"
    );

    panel.update(cx, |panel, _| {
        assert!(
            panel.selection.is_none(),
            "Project panel should start without a selection"
        );
        assert!(
            panel.state.last_worktree_root_id.is_some(),
            "Project panel should still track the hidden root entry"
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        let root_entry_id = panel
            .state
            .last_worktree_root_id
            .expect("hidden root should be available for background context menu actions");
        panel.deploy_context_menu(
            gpui::point(gpui::px(1.), gpui::px(1.)),
            root_entry_id,
            window,
            cx,
        );
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        assert!(
            panel.filename_editor.read(cx).is_focused(window),
            "New File from the background context menu should open the filename editor"
        );
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["  [EDITOR: '']  <== selected"],
        "New file editor should appear at the hidden root level"
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("new_file_from_context_menu.txt", window, cx)
        });
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["  new_file_from_context_menu.txt  <== selected  <== marked"],
        "Confirmed file should appear at the hidden root level"
    );

    assert!(
        fs.is_file(Path::new("/root/new_file_from_context_menu.txt"))
            .await,
        "File should be created in the empty root directory"
    );
}

#[cfg(windows)]
#[gpui::test]
async fn test_create_entry_with_trailing_dot_windows(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "file1.txt": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
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

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1",
        ],
        "Initial state with nothing selected"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    panel
        .update_in(cx, |panel, window, cx| {
            panel
                .filename_editor
                .update(cx, |editor, cx| editor.set_text("foo.", window, cx));
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1",
            "      foo  <== selected  <== marked",
        ],
        "A new file is created under the root directory without the trailing dot"
    );
}
