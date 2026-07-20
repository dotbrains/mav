#[gpui::test]
async fn test_compare_selected_files(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file1.txt": "content of file1",
            "file2.txt": "content of file2",
            "dir1": {
                "file3.txt": "content of file3"
            }
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

    let file1_path = "root/file1.txt";
    let file2_path = "root/file2.txt";
    select_path_with_mark(&panel, file1_path, cx);
    select_path_with_mark(&panel, file2_path, cx);

    panel.update_in(cx, |panel, window, cx| {
        panel.compare_marked_files(&CompareMarkedFiles, window, cx);
    });
    cx.executor().run_until_parked();

    workspace.update_in(cx, |workspace, _, cx| {
        let active_items = workspace
            .panes()
            .iter()
            .filter_map(|pane| pane.read(cx).active_item())
            .collect::<Vec<_>>();
        assert_eq!(active_items.len(), 1);
        let diff_view = active_items
            .into_iter()
            .next()
            .unwrap()
            .downcast::<FileDiffView>()
            .expect("Open item should be an FileDiffView");
        assert_eq!(diff_view.tab_content_text(0, cx), "file1.txt ↔ file2.txt");
        assert_eq!(
            diff_view.tab_tooltip_text(cx).unwrap(),
            format!(
                "{} ↔ {}",
                rel_path(file1_path).display(PathStyle::local()),
                rel_path(file2_path).display(PathStyle::local())
            )
        );
    });

    let file1_entry_id = find_project_entry(&panel, file1_path, cx).unwrap();
    let file2_entry_id = find_project_entry(&panel, file2_path, cx).unwrap();
    let worktree_id = panel.update(cx, |panel, cx| {
        panel
            .project
            .read(cx)
            .worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .id()
    });

    let expected_entries = [
        SelectedEntry {
            worktree_id,
            entry_id: file1_entry_id,
        },
        SelectedEntry {
            worktree_id,
            entry_id: file2_entry_id,
        },
    ];
    panel.update(cx, |panel, _cx| {
        assert_eq!(
            &panel.marked_entries, &expected_entries,
            "Should keep marked entries after comparison"
        );
    });

    panel.update(cx, |panel, cx| {
        panel.project.update(cx, |_, cx| {
            cx.emit(project::Event::RevealInProjectPanel(file2_entry_id))
        })
    });

    panel.update(cx, |panel, _cx| {
        assert_eq!(
            &panel.marked_entries, &expected_entries,
            "Marked entries should persist after focusing back on the project panel"
        );
    });
}

#[gpui::test]
async fn test_compare_files_context_menu(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file1.txt": "content of file1",
            "file2.txt": "content of file2",
            "dir1": {},
            "dir2": {
                "file3.txt": "content of file3"
            }
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

    // Test 1: When only one file is selected, there should be no compare option
    select_path(&panel, "root/file1.txt", cx);

    let selected_files = panel.update(cx, |panel, cx| panel.file_abs_paths_to_diff(cx));
    assert_eq!(
        selected_files, None,
        "Should not have compare option when only one file is selected"
    );

    // Test 2: When multiple files are selected, there should be a compare option
    select_path_with_mark(&panel, "root/file1.txt", cx);
    select_path_with_mark(&panel, "root/file2.txt", cx);

    let selected_files = panel.update(cx, |panel, cx| panel.file_abs_paths_to_diff(cx));
    assert!(
        selected_files.is_some(),
        "Should have files selected for comparison"
    );
    if let Some((file1, file2)) = selected_files {
        assert!(
            file1.to_string_lossy().ends_with("file1.txt")
                && file2.to_string_lossy().ends_with("file2.txt"),
            "Should have file1.txt and file2.txt as the selected files when multi-selecting"
        );
    }

    // Test 3: Selecting a directory shouldn't count as a comparable file
    select_path_with_mark(&panel, "root/dir1", cx);

    let selected_files = panel.update(cx, |panel, cx| panel.file_abs_paths_to_diff(cx));
    assert!(
        selected_files.is_some(),
        "Directory selection should not affect comparable files"
    );
    if let Some((file1, file2)) = selected_files {
        assert!(
            file1.to_string_lossy().ends_with("file1.txt")
                && file2.to_string_lossy().ends_with("file2.txt"),
            "Selecting a directory should not affect the number of comparable files"
        );
    }

    // Test 4: Selecting one more file
    select_path_with_mark(&panel, "root/dir2/file3.txt", cx);

    let selected_files = panel.update(cx, |panel, cx| panel.file_abs_paths_to_diff(cx));
    assert!(
        selected_files.is_some(),
        "Directory selection should not affect comparable files"
    );
    if let Some((file1, file2)) = selected_files {
        assert!(
            file1.to_string_lossy().ends_with("file2.txt")
                && file2.to_string_lossy().ends_with("file3.txt"),
            "Selecting a directory should not affect the number of comparable files"
        );
    }
}

#[gpui::test]
async fn test_reveal_in_file_manager_path_falls_back_to_worktree_root(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "file.txt": "content",
            "dir": {},
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

    select_path(&panel, "root/file.txt", cx);
    let selected_reveal_path = panel
        .update(cx, |panel, cx| panel.reveal_in_file_manager_path(cx))
        .expect("selected entry should produce a reveal path");
    assert!(
        selected_reveal_path.ends_with(Path::new("file.txt")),
        "Expected selected file path, got {:?}",
        selected_reveal_path
    );

    panel.update(cx, |panel, _| {
        panel.selection = None;
        panel.marked_entries.clear();
    });
    let fallback_reveal_path = panel
        .update(cx, |panel, cx| panel.reveal_in_file_manager_path(cx))
        .expect("project root should be used when selection is empty");
    assert!(
        fallback_reveal_path.ends_with(Path::new("root")),
        "Expected worktree root path, got {:?}",
        fallback_reveal_path
    );
}
