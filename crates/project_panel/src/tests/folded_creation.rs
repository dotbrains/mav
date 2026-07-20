#[gpui::test]
async fn test_ensure_temporary_folding_when_creating_in_different_nested_dirs(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    // parent: accept
    run_create_file_in_folded_path_case(
        "parent",
        "root1/parent",
        "file_in_parent.txt",
        &[
            "v root1",
            "    v parent",
            "        > subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        &[
            "v root1",
            "    v parent",
            "        > subdir/child",
            "          file_in_parent.txt  <== selected  <== marked",
        ],
        true,
        cx,
    )
    .await;

    // parent: cancel
    run_create_file_in_folded_path_case(
        "parent",
        "root1/parent",
        "file_in_parent.txt",
        &[
            "v root1",
            "    v parent",
            "        > subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        &["v root1", "    > parent/subdir/child  <== selected"],
        false,
        cx,
    )
    .await;

    // subdir: accept
    run_create_file_in_folded_path_case(
        "subdir",
        "root1/parent/subdir",
        "file_in_subdir.txt",
        &[
            "v root1",
            "    v parent/subdir",
            "        > child",
            "          [EDITOR: '']  <== selected",
        ],
        &[
            "v root1",
            "    v parent/subdir",
            "        > child",
            "          file_in_subdir.txt  <== selected  <== marked",
        ],
        true,
        cx,
    )
    .await;

    // subdir: cancel
    run_create_file_in_folded_path_case(
        "subdir",
        "root1/parent/subdir",
        "file_in_subdir.txt",
        &[
            "v root1",
            "    v parent/subdir",
            "        > child",
            "          [EDITOR: '']  <== selected",
        ],
        &["v root1", "    > parent/subdir/child  <== selected"],
        false,
        cx,
    )
    .await;

    // child: accept
    run_create_file_in_folded_path_case(
        "child",
        "root1/parent/subdir/child",
        "file_in_child.txt",
        &[
            "v root1",
            "    v parent/subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        &[
            "v root1",
            "    v parent/subdir/child",
            "          file_in_child.txt  <== selected  <== marked",
        ],
        true,
        cx,
    )
    .await;

    // child: cancel
    run_create_file_in_folded_path_case(
        "child",
        "root1/parent/subdir/child",
        "file_in_child.txt",
        &[
            "v root1",
            "    v parent/subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        &["v root1", "    v parent/subdir/child  <== selected"],
        false,
        cx,
    )
    .await;
}

#[gpui::test]
async fn test_preserve_temporary_unfolded_active_index_on_blur_from_context_menu(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "parent": {
                "subdir": {
                    "child": {},
                }
            }
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

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx);
    });
    cx.run_until_parked();

    select_folded_path_with_mark(
        &panel,
        "root1/parent/subdir/child",
        "root1/parent/subdir",
        cx,
    );
    panel.update(cx, |panel, _| {
        panel.marked_entries.clear();
    });

    let parent_entry_id = find_project_entry(&panel, "root1/parent", cx)
        .expect("parent directory should exist for this test");
    let subdir_entry_id = find_project_entry(&panel, "root1/parent/subdir", cx)
        .expect("subdir directory should exist for this test");
    let child_entry_id = find_project_entry(&panel, "root1/parent/subdir/child", cx)
        .expect("child directory should exist for this test");

    panel.update(cx, |panel, _| {
        let selection = panel
            .selection
            .expect("leaf directory should be selected before creating a new entry");
        assert_eq!(
            selection.entry_id, child_entry_id,
            "initial selection should be the folded leaf entry"
        );
        assert_eq!(
            panel.resolve_entry(selection.entry_id),
            subdir_entry_id,
            "active folded component should start at subdir"
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.deploy_context_menu(
            gpui::point(gpui::px(1.), gpui::px(1.)),
            child_entry_id,
            window,
            cx,
        );
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    cx.run_until_parked();

    set_folded_active_ancestor(&panel, "root1/parent/subdir", "root1/parent", cx);

    panel.update_in(cx, |panel, window, cx| {
        panel.deploy_context_menu(
            gpui::point(gpui::px(2.), gpui::px(2.)),
            subdir_entry_id,
            window,
            cx,
        );
    });
    cx.run_until_parked();

    panel.update(cx, |panel, _| {
        assert!(
            panel.state.edit_state.is_none(),
            "opening another context menu should blur the filename editor and discard edit state"
        );
        let selection = panel
            .selection
            .expect("selection should restore to the previously focused leaf entry");
        assert_eq!(
            selection.entry_id, child_entry_id,
            "blur-driven cancellation should restore the previous leaf selection"
        );
        assert_eq!(
            panel.resolve_entry(selection.entry_id),
            parent_entry_id,
            "temporary unfolded pending state should preserve the active ancestor chosen before blur"
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    v parent",
            "        > subdir/child",
            "          [EDITOR: '']  <== selected",
        ],
        "new file after blur should use the preserved active ancestor"
    );
    panel.update(cx, |panel, _| {
        let edit_state = panel
            .state
            .edit_state
            .as_ref()
            .expect("new file should enter edit state");
        assert_eq!(
            edit_state.temporarily_unfolded,
            Some(parent_entry_id),
            "temporary unfolding should now target parent after restoring the active ancestor"
        );
    });

    let file_name = "created_after_blur.txt";
    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text(file_name, window, cx);
            });
            panel.confirm_edit(true, window, cx).expect(
                "confirm_edit should start creation for the file created after blur transition",
            )
        })
        .await
        .expect("creating file after blur transition should succeed");
    cx.run_until_parked();

    assert!(
        fs.is_file(Path::new("/root1/parent/created_after_blur.txt"))
            .await,
        "file should be created under parent after active ancestor is restored to parent"
    );
    assert!(
        !fs.is_file(Path::new("/root1/parent/subdir/created_after_blur.txt"))
            .await,
        "file should not be created under subdir when parent is the active ancestor"
    );
}

async fn run_create_file_in_folded_path_case(
    case_name: &str,
    active_ancestor_path: &str,
    created_file_name: &str,
    expected_temporary_state: &[&str],
    expected_final_state: &[&str],
    accept_creation: bool,
    cx: &mut gpui::TestAppContext,
) {
    let expected_collapsed_state = &["v root1", "    > parent/subdir/child  <== selected"];

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "parent": {
                "subdir": {
                    "child": {},
                }
            }
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

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx);
    });
    cx.run_until_parked();

    select_folded_path_with_mark(
        &panel,
        "root1/parent/subdir/child",
        active_ancestor_path,
        cx,
    );
    panel.update(cx, |panel, _| {
        panel.marked_entries.clear();
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        expected_collapsed_state,
        "case '{}' should start from a folded state",
        case_name
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.new_file(&NewFile, window, cx);
    });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        expected_temporary_state,
        "case '{}' ({}) should temporarily unfold the active ancestor while editing",
        case_name,
        if accept_creation { "accept" } else { "cancel" }
    );

    let relative_directory = active_ancestor_path
        .strip_prefix("root1/")
        .expect("active_ancestor_path should start with root1/");
    let created_file_path = PathBuf::from("/root1")
        .join(relative_directory)
        .join(created_file_name);

    if accept_creation {
        panel
            .update_in(cx, |panel, window, cx| {
                panel.filename_editor.update(cx, |editor, cx| {
                    editor.set_text(created_file_name, window, cx);
                });
                panel.confirm_edit(true, window, cx).unwrap()
            })
            .await
            .unwrap();
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            expected_final_state,
            "case '{}' should keep the newly created file selected and marked after accept",
            case_name
        );
        assert!(
            fs.is_file(created_file_path.as_path()).await,
            "case '{}' should create file '{}'",
            case_name,
            created_file_path.display()
        );
    } else {
        panel.update_in(cx, |panel, window, cx| {
            panel.cancel(&Cancel, window, cx);
        });
        cx.run_until_parked();

        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            expected_final_state,
            "case '{}' should keep the expected panel state after cancel",
            case_name
        );
        assert!(
            !fs.is_file(created_file_path.as_path()).await,
            "case '{}' should not create a file after cancel",
            case_name
        );
    }
}
