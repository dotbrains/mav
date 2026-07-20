use super::*;

#[gpui::test]
async fn test_new_file_move(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.as_fake().insert_tree(path!("/root"), json!({})).await;
    let project = Project::test(fs, [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Make a new buffer with no backing file
    workspace.update_in(cx, |workspace, window, cx| {
        Editor::new_file(workspace, &Default::default(), window, cx)
    });

    cx.executor().run_until_parked();

    // "Save as" the buffer, creating a new backing file for it
    let save_task = workspace.update_in(cx, |workspace, window, cx| {
        workspace.save_active_item(workspace::SaveIntent::Save, window, cx)
    });

    cx.executor().run_until_parked();
    cx.simulate_new_path_selection(|_| Some(PathBuf::from(path!("/root/new"))));
    save_task.await.unwrap();

    // Rename the file
    select_path(&panel, "root/new", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root", "      new  <== selected  <== marked"]
    );
    panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));
    panel.update_in(cx, |panel, window, cx| {
        panel
            .filename_editor
            .update(cx, |editor, cx| editor.set_text("newer", window, cx));
    });
    panel.update_in(cx, |panel, window, cx| panel.confirm(&Confirm, window, cx));

    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root", "      newer  <== selected"]
    );

    workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.save_active_item(workspace::SaveIntent::Save, window, cx)
        })
        .await
        .unwrap();

    cx.executor().run_until_parked();
    // assert that saving the file doesn't restore "new"
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root", "      newer  <== selected"]
    );
}

// NOTE: This test is skipped on Windows, because on Windows, unlike on Unix,
// you can't rename a directory which some program has already open. This is a
// limitation of the Windows. Since Mav will have the root open, it will hold an open handle
// to it, and thus renaming it will fail on Windows.
// See: https://stackoverflow.com/questions/41365318/access-is-denied-when-renaming-folder
// See: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information
#[gpui::test]
#[cfg_attr(target_os = "windows", ignore)]
async fn test_rename_root_of_worktree(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "dir1": {
                "file1.txt": "content 1",
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
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root1/dir1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root1", "    v dir1  <== selected", "          file1.txt",],
        "Initial state with worktrees"
    );

    select_path(&panel, "root1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root1  <== selected", "    v dir1", "          file1.txt",],
    );

    // Rename root1 to new_root1
    panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v [EDITOR: 'root1']  <== selected",
            "    v dir1",
            "          file1.txt",
        ],
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel
            .filename_editor
            .update(cx, |editor, cx| editor.set_text("new_root1", window, cx));
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v new_root1  <== selected",
            "    v dir1",
            "          file1.txt",
        ],
        "Should update worktree name"
    );

    // Ensure internal paths have been updated
    select_path(&panel, "new_root1/dir1/file1.txt", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v new_root1",
            "    v dir1",
            "          file1.txt  <== selected",
        ],
        "Files in renamed worktree are selectable"
    );
}

#[gpui::test]
async fn test_rename_with_hide_root(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "dir1": { "file1.txt": "content" },
            "file2.txt": "content",
        }),
    )
    .await;
    fs.insert_tree("/root2", json!({ "file3.txt": "content" }))
        .await;

    // Test 1: Single worktree, hide_root=true - rename should be blocked
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
                    hide_root: true,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        panel.update(cx, |panel, cx| {
            let project = panel.project.read(cx);
            let worktree = project.visible_worktrees(cx).next().unwrap();
            let root_entry = worktree.read(cx).root_entry().unwrap();
            panel.selection = Some(SelectedEntry {
                worktree_id: worktree.read(cx).id(),
                entry_id: root_entry.id,
            });
        });

        panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));

        assert!(
            panel.read_with(cx, |panel, _| panel.state.edit_state.is_none()),
            "Rename should be blocked when hide_root=true with single worktree"
        );
    }

    // Test 2: Multiple worktrees, hide_root=true - rename should work
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
                    hide_root: true,
                    ..settings
                },
                cx,
            );
        });

        let panel = workspace.update_in(cx, ProjectPanel::new);
        cx.run_until_parked();

        select_path(&panel, "root1", cx);
        panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));

        #[cfg(target_os = "windows")]
        assert!(
            panel.read_with(cx, |panel, _| panel.state.edit_state.is_none()),
            "Rename should be blocked on Windows even with multiple worktrees"
        );

        #[cfg(not(target_os = "windows"))]
        {
            assert!(
                panel.read_with(cx, |panel, _| panel.state.edit_state.is_some()),
                "Rename should work with multiple worktrees on non-Windows when hide_root=true"
            );
            panel.update_in(cx, |panel, window, cx| {
                panel.cancel(&menu::Cancel, window, cx)
            });
        }
    }
}
