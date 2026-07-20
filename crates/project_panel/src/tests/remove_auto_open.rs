use super::*;

#[gpui::test]
async fn test_remove_opened_file(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/src"),
        json!({
            "test": {
                "first.rs": "// First Rust file",
                "second.rs": "// Second Rust file",
                "third.rs": "// Third Rust file",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/src").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "src/test", cx);
    select_path(&panel, "src/test/first.rs", cx);
    panel.update_in(cx, |panel, window, cx| panel.open(&Open, window, cx));
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v src",
            "    v test",
            "          first.rs  <== selected  <== marked",
            "          second.rs",
            "          third.rs"
        ]
    );
    ensure_single_file_is_opened(&workspace, "test/first.rs", cx);

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v src",
            "    v test",
            "          second.rs  <== selected",
            "          third.rs"
        ],
        "Project panel should have no deleted file, no other file is selected in it"
    );
    ensure_no_open_items_and_panes(&workspace, cx);

    panel.update_in(cx, |panel, window, cx| panel.open(&Open, window, cx));
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v src",
            "    v test",
            "          second.rs  <== selected  <== marked",
            "          third.rs"
        ]
    );
    ensure_single_file_is_opened(&workspace, "test/second.rs", cx);

    workspace.update_in(cx, |workspace, window, cx| {
        let active_items = workspace
            .panes()
            .iter()
            .filter_map(|pane| pane.read(cx).active_item())
            .collect::<Vec<_>>();
        assert_eq!(active_items.len(), 1);
        let open_editor = active_items
            .into_iter()
            .next()
            .unwrap()
            .downcast::<Editor>()
            .expect("Open item should be an editor");
        open_editor.update(cx, |editor, cx| {
            editor.set_text("Another text!", window, cx)
        });
    });
    submit_deletion_skipping_prompt(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v src", "    v test", "          third.rs  <== selected"],
        "Project panel should have no deleted file, with one last file remaining"
    );
    ensure_no_open_items_and_panes(&workspace, cx);
}

#[gpui::test]
async fn test_auto_open_new_file_when_enabled(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    set_auto_open_settings(
        cx,
        ProjectPanelAutoOpenSettings {
            on_create: Some(true),
            ..Default::default()
        },
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text("auto-open.rs", window, cx);
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();

    ensure_single_file_is_opened(&workspace, "auto-open.rs", cx);
}

#[gpui::test]
async fn test_auto_open_new_file_when_disabled(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    set_auto_open_settings(
        cx,
        ProjectPanelAutoOpenSettings {
            on_create: Some(false),
            ..Default::default()
        },
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text("manual-open.rs", window, cx);
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();

    ensure_no_open_items_and_panes(&workspace, cx);
}

#[gpui::test]
async fn test_auto_open_on_paste_when_enabled(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    set_auto_open_settings(
        cx,
        ProjectPanelAutoOpenSettings {
            on_paste: Some(true),
            ..Default::default()
        },
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "original.rs": ""
            },
            "target": {}
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root/src", cx);
    toggle_expand_dir(&panel, "root/target", cx);

    select_path(&panel, "root/src/original.rs", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
    });

    select_path(&panel, "root/target", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    ensure_single_file_is_opened(&workspace, "target/original.rs", cx);
}

#[gpui::test]
async fn test_auto_open_on_paste_when_disabled(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    set_auto_open_settings(
        cx,
        ProjectPanelAutoOpenSettings {
            on_paste: Some(false),
            ..Default::default()
        },
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "original.rs": ""
            },
            "target": {}
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root/src", cx);
    toggle_expand_dir(&panel, "root/target", cx);

    select_path(&panel, "root/src/original.rs", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
    });

    select_path(&panel, "root/target", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    ensure_no_open_items_and_panes(&workspace, cx);
    assert!(
        find_project_entry(&panel, "root/target/original.rs", cx).is_some(),
        "Pasted entry should exist even when auto-open is disabled"
    );
}

#[gpui::test]
async fn test_auto_open_on_drop_when_enabled(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    set_auto_open_settings(
        cx,
        ProjectPanelAutoOpenSettings {
            on_drop: Some(true),
            ..Default::default()
        },
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let temp_dir = tempfile::tempdir().unwrap();
    let external_path = temp_dir.path().join("dropped.rs");
    std::fs::write(&external_path, "// dropped").unwrap();
    fs.insert_tree_from_real_fs(temp_dir.path(), temp_dir.path())
        .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    let root_entry = find_project_entry(&panel, "root", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        panel.drop_external_files(std::slice::from_ref(&external_path), root_entry, window, cx);
    });
    cx.executor().run_until_parked();

    ensure_single_file_is_opened(&workspace, "dropped.rs", cx);
}

#[gpui::test]
async fn test_auto_open_on_drop_when_disabled(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);
    set_auto_open_settings(
        cx,
        ProjectPanelAutoOpenSettings {
            on_drop: Some(false),
            ..Default::default()
        },
    );

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/root"), json!({})).await;

    let temp_dir = tempfile::tempdir().unwrap();
    let external_path = temp_dir.path().join("manual.rs");
    std::fs::write(&external_path, "// dropped").unwrap();
    fs.insert_tree_from_real_fs(temp_dir.path(), temp_dir.path())
        .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    let root_entry = find_project_entry(&panel, "root", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        panel.drop_external_files(std::slice::from_ref(&external_path), root_entry, window, cx);
    });
    cx.executor().run_until_parked();

    ensure_no_open_items_and_panes(&workspace, cx);
    assert!(
        find_project_entry(&panel, "root/manual.rs", cx).is_some(),
        "Dropped entry should exist even when auto-open is disabled"
    );
}
