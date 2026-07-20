#[gpui::test]
async fn test_highlight_entry_for_external_drag(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "file1.txt": "",
                "dir2": {
                    "file2.txt": ""
                }
            },
            "file3.txt": ""
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

    panel.update(cx, |panel, cx| {
        let project = panel.project.read(cx);
        let worktree = project.visible_worktrees(cx).next().unwrap();
        let worktree = worktree.read(cx);

        // Test 1: Target is a directory, should highlight the directory itself
        let dir_entry = worktree.entry_for_path(rel_path("dir1")).unwrap();
        let result = panel.highlight_entry_for_external_drag(dir_entry, worktree);
        assert_eq!(
            result,
            Some(dir_entry.id),
            "Should highlight directory itself"
        );

        // Test 2: Target is nested file, should highlight immediate parent
        let nested_file = worktree
            .entry_for_path(rel_path("dir1/dir2/file2.txt"))
            .unwrap();
        let nested_parent = worktree.entry_for_path(rel_path("dir1/dir2")).unwrap();
        let result = panel.highlight_entry_for_external_drag(nested_file, worktree);
        assert_eq!(
            result,
            Some(nested_parent.id),
            "Should highlight immediate parent"
        );

        // Test 3: Target is root level file, should highlight root
        let root_file = worktree.entry_for_path(rel_path("file3.txt")).unwrap();
        let result = panel.highlight_entry_for_external_drag(root_file, worktree);
        assert_eq!(
            result,
            Some(worktree.root_entry().unwrap().id),
            "Root level file should return None"
        );

        // Test 4: Target is root itself, should highlight root
        let root_entry = worktree.root_entry().unwrap();
        let result = panel.highlight_entry_for_external_drag(root_entry, worktree);
        assert_eq!(
            result,
            Some(root_entry.id),
            "Root level file should return None"
        );
    });
}

#[gpui::test]
async fn test_highlight_entry_for_selection_drag(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "parent_dir": {
                "child_file.txt": "",
                "sibling_file.txt": "",
                "child_dir": {
                    "nested_file.txt": ""
                }
            },
            "other_dir": {
                "other_file.txt": ""
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

    panel.update(cx, |panel, cx| {
        let project = panel.project.read(cx);
        let worktree = project.visible_worktrees(cx).next().unwrap();
        let worktree_id = worktree.read(cx).id();
        let worktree = worktree.read(cx);

        let parent_dir = worktree.entry_for_path(rel_path("parent_dir")).unwrap();
        let child_file = worktree
            .entry_for_path(rel_path("parent_dir/child_file.txt"))
            .unwrap();
        let sibling_file = worktree
            .entry_for_path(rel_path("parent_dir/sibling_file.txt"))
            .unwrap();
        let child_dir = worktree
            .entry_for_path(rel_path("parent_dir/child_dir"))
            .unwrap();
        let other_dir = worktree.entry_for_path(rel_path("other_dir")).unwrap();
        let other_file = worktree
            .entry_for_path(rel_path("other_dir/other_file.txt"))
            .unwrap();

        // Test 1: Single item drag, don't highlight parent directory
        let dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id,
                entry_id: child_file.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id,
                entry_id: child_file.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let result =
            panel.highlight_entry_for_selection_drag(parent_dir, worktree, &dragged_selection, cx);
        assert_eq!(result, None, "Should not highlight parent of dragged item");

        // Test 2: Single item drag, don't highlight sibling files
        let result = panel.highlight_entry_for_selection_drag(
            sibling_file,
            worktree,
            &dragged_selection,
            cx,
        );
        assert_eq!(result, None, "Should not highlight sibling files");

        // Test 3: Single item drag, highlight unrelated directory
        let result =
            panel.highlight_entry_for_selection_drag(other_dir, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(other_dir.id),
            "Should highlight unrelated directory"
        );

        // Test 4: Single item drag, highlight sibling directory
        let result =
            panel.highlight_entry_for_selection_drag(child_dir, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(child_dir.id),
            "Should highlight sibling directory"
        );

        // Test 5: Multiple items drag, highlight parent directory
        let dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id,
                entry_id: child_file.id,
            },
            marked_selections: Arc::new([
                SelectedEntry {
                    worktree_id,
                    entry_id: child_file.id,
                },
                SelectedEntry {
                    worktree_id,
                    entry_id: sibling_file.id,
                },
            ]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let result =
            panel.highlight_entry_for_selection_drag(parent_dir, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(parent_dir.id),
            "Should highlight parent with multiple items"
        );

        // Test 6: Target is file in different directory, highlight parent
        let result =
            panel.highlight_entry_for_selection_drag(other_file, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(other_dir.id),
            "Should highlight parent of target file"
        );

        // Test 7: Target is directory, always highlight
        let result =
            panel.highlight_entry_for_selection_drag(child_dir, worktree, &dragged_selection, cx);
        assert_eq!(
            result,
            Some(child_dir.id),
            "Should always highlight directories"
        );
    });
}

#[gpui::test]
async fn test_highlight_entry_for_selection_drag_cross_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "src": {
                "main.rs": "",
                "lib.rs": ""
            }
        }),
    )
    .await;
    fs.insert_tree(
        "/root2",
        json!({
            "src": {
                "main.rs": "",
                "test.rs": ""
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update(cx, |panel, cx| {
        let project = panel.project.read(cx);
        let worktrees: Vec<_> = project.visible_worktrees(cx).collect();

        let worktree_a = &worktrees[0];
        let main_rs_from_a = worktree_a
            .read(cx)
            .entry_for_path(rel_path("src/main.rs"))
            .unwrap();

        let worktree_b = &worktrees[1];
        let src_dir_from_b = worktree_b.read(cx).entry_for_path(rel_path("src")).unwrap();
        let main_rs_from_b = worktree_b
            .read(cx)
            .entry_for_path(rel_path("src/main.rs"))
            .unwrap();

        // Test dragging file from worktree A onto parent of file with same relative path in worktree B
        let dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree_a.read(cx).id(),
                entry_id: main_rs_from_a.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id: worktree_a.read(cx).id(),
                entry_id: main_rs_from_a.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.highlight_entry_for_selection_drag(
            src_dir_from_b,
            worktree_b.read(cx),
            &dragged_selection,
            cx,
        );
        assert_eq!(
            result,
            Some(src_dir_from_b.id),
            "Should highlight target directory from different worktree even with same relative path"
        );

        // Test dragging file from worktree A onto file with same relative path in worktree B
        let result = panel.highlight_entry_for_selection_drag(
            main_rs_from_b,
            worktree_b.read(cx),
            &dragged_selection,
            cx,
        );
        assert_eq!(
            result,
            Some(src_dir_from_b.id),
            "Should highlight parent of target file from different worktree"
        );
    });
}

#[gpui::test]
async fn test_should_highlight_background_for_selection_drag(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "parent_dir": {
                "child_file.txt": "",
                "nested_dir": {
                    "nested_file.txt": ""
                }
            },
            "root_file.txt": ""
        }),
    )
    .await;

    fs.insert_tree(
        "/root2",
        json!({
            "other_dir": {
                "other_file.txt": ""
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update(cx, |panel, cx| {
        let project = panel.project.read(cx);
        let worktrees: Vec<_> = project.visible_worktrees(cx).collect();
        let worktree1 = worktrees[0].read(cx);
        let worktree2 = worktrees[1].read(cx);
        let worktree1_id = worktree1.id();
        let _worktree2_id = worktree2.id();

        let root1_entry = worktree1.root_entry().unwrap();
        let root2_entry = worktree2.root_entry().unwrap();
        let _parent_dir = worktree1.entry_for_path(rel_path("parent_dir")).unwrap();
        let child_file = worktree1
            .entry_for_path(rel_path("parent_dir/child_file.txt"))
            .unwrap();
        let nested_file = worktree1
            .entry_for_path(rel_path("parent_dir/nested_dir/nested_file.txt"))
            .unwrap();
        let root_file = worktree1.entry_for_path(rel_path("root_file.txt")).unwrap();

        // Test 1: Multiple entries - should always highlight background
        let multiple_dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: child_file.id,
            },
            marked_selections: Arc::new([
                SelectedEntry {
                    worktree_id: worktree1_id,
                    entry_id: child_file.id,
                },
                SelectedEntry {
                    worktree_id: worktree1_id,
                    entry_id: nested_file.id,
                },
            ]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.should_highlight_background_for_selection_drag(
            &multiple_dragged_selection,
            root1_entry.id,
            cx,
        );
        assert!(result, "Should highlight background for multiple entries");

        // Test 2: Single entry with non-empty parent path - should highlight background
        let nested_dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: nested_file.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: nested_file.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.should_highlight_background_for_selection_drag(
            &nested_dragged_selection,
            root1_entry.id,
            cx,
        );
        assert!(result, "Should highlight background for nested file");

        // Test 3: Single entry at root level, same worktree - should NOT highlight background
        let root_file_dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: root_file.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: root_file.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.should_highlight_background_for_selection_drag(
            &root_file_dragged_selection,
            root1_entry.id,
            cx,
        );
        assert!(
            !result,
            "Should NOT highlight background for root file in same worktree"
        );

        // Test 4: Single entry at root level, different worktree - should highlight background
        let result = panel.should_highlight_background_for_selection_drag(
            &root_file_dragged_selection,
            root2_entry.id,
            cx,
        );
        assert!(
            result,
            "Should highlight background for root file from different worktree"
        );

        // Test 5: Single entry in subdirectory - should highlight background
        let child_file_dragged_selection = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: child_file.id,
            },
            marked_selections: Arc::new([SelectedEntry {
                worktree_id: worktree1_id,
                entry_id: child_file.id,
            }]),
            source_pane: None,
            active_selection_is_file: true,
        };

        let result = panel.should_highlight_background_for_selection_drag(
            &child_file_dragged_selection,
            root1_entry.id,
            cx,
        );
        assert!(
            result,
            "Should highlight background for file with non-empty parent path"
        );
    });
}
