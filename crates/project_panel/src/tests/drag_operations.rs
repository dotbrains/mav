use super::*;

#[gpui::test]
async fn test_drag_marked_entries_in_folded_directories(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "a": {
                "b": {
                    "c": {}
                }
            },
            "e": {
                "f": {
                    "g": {}
                }
            },
            "target": {}
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

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

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root", "    > a/b/c", "    > e/f/g", "    > target"]
    );

    select_folded_path_with_mark(&panel, "root/a/b/c", "root/a/b", cx);
    select_folded_path_with_mark(&panel, "root/e/f/g", "root/e/f", cx);

    panel.update_in(cx, |panel, window, cx| {
        let drag = DraggedSelection {
            active_selection: *panel.selection.as_ref().unwrap(),
            marked_selections: panel.marked_entries.clone().into(),
            source_pane: None,
            active_selection_is_file: true,
        };
        let target_entry = panel
            .project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .entry_for_path(rel_path("target"))
            .unwrap();
        panel.drag_onto(&drag, target_entry.id, false, window, cx);
    });
    cx.executor().run_until_parked();

    // After dragging 'b/c' and 'f/g' should be moved to target
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "    > a",
            "    > e",
            "    v target",
            "        > b/c",
            "        > f/g  <== selected  <== marked"
        ],
        "Should move 'b/c' and 'f/g' to target, leaving 'a' and 'e'"
    );
}

#[gpui::test]
async fn test_dragging_same_named_files_preserves_one_source_on_conflict(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir_a": {
                "shared.txt": "from a"
            },
            "dir_b": {
                "shared.txt": "from b"
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |multi_workspace, _| multi_workspace.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        let (root_entry_id, worktree_id, entry_a_id, entry_b_id) = {
            let worktree = panel.project.read(cx).visible_worktrees(cx).next().unwrap();
            let worktree = worktree.read(cx);
            let root_entry_id = worktree.root_entry().unwrap().id;
            let worktree_id = worktree.id();
            let entry_a_id = worktree
                .entry_for_path(rel_path("dir_a/shared.txt"))
                .unwrap()
                .id;
            let entry_b_id = worktree
                .entry_for_path(rel_path("dir_b/shared.txt"))
                .unwrap()
                .id;
            (root_entry_id, worktree_id, entry_a_id, entry_b_id)
        };

        let drag = DraggedSelection {
            active_selection: SelectedEntry {
                worktree_id,
                entry_id: entry_a_id,
            },
            marked_selections: Arc::new([
                SelectedEntry {
                    worktree_id,
                    entry_id: entry_a_id,
                },
                SelectedEntry {
                    worktree_id,
                    entry_id: entry_b_id,
                },
            ]),
            source_pane: None,
            active_selection_is_file: true,
        };

        panel.drag_onto(&drag, root_entry_id, false, window, cx);
    });
    cx.executor().run_until_parked();

    let files = fs.files();
    assert!(files.contains(&PathBuf::from(path!("/root/shared.txt"))));

    let remaining_sources = [
        PathBuf::from(path!("/root/dir_a/shared.txt")),
        PathBuf::from(path!("/root/dir_b/shared.txt")),
    ]
    .into_iter()
    .filter(|path| files.contains(path))
    .count();

    assert_eq!(
        remaining_sources, 1,
        "one conflicting source file should remain in place"
    );
}

#[gpui::test]
async fn test_drag_entries_between_different_worktrees(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root_a",
        json!({
            "src": {
                "lib.rs": "",
                "main.rs": ""
            },
            "docs": {
                "guide.md": ""
            },
            "multi": {
                "alpha.txt": "",
                "beta.txt": ""
            }
        }),
    )
    .await;
    fs.insert_tree(
        "/root_b",
        json!({
            "dst": {
                "existing.md": ""
            },
            "target.txt": ""
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root_a".as_ref(), "/root_b".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Case 1: move a file onto a directory in another worktree.
    select_path(&panel, "root_a/src/main.rs", cx);
    drag_selection_to(&panel, "root_b/dst", false, cx);
    assert!(
        find_project_entry(&panel, "root_b/dst/main.rs", cx).is_some(),
        "Dragged file should appear under destination worktree"
    );
    assert_eq!(
        find_project_entry(&panel, "root_a/src/main.rs", cx),
        None,
        "Dragged file should be removed from the source worktree"
    );

    // Case 2: drop a file onto another worktree file so it lands in the parent directory.
    select_path(&panel, "root_a/docs/guide.md", cx);
    drag_selection_to(&panel, "root_b/dst/existing.md", true, cx);
    assert!(
        find_project_entry(&panel, "root_b/dst/guide.md", cx).is_some(),
        "Dropping onto a file should place the entry beside the target file"
    );
    assert_eq!(
        find_project_entry(&panel, "root_a/docs/guide.md", cx),
        None,
        "Source file should be removed after the move"
    );

    // Case 3: move an entire directory.
    select_path(&panel, "root_a/src", cx);
    drag_selection_to(&panel, "root_b/dst", false, cx);
    assert!(
        find_project_entry(&panel, "root_b/dst/src/lib.rs", cx).is_some(),
        "Dragging a directory should move its nested contents"
    );
    assert_eq!(
        find_project_entry(&panel, "root_a/src", cx),
        None,
        "Directory should no longer exist in the source worktree"
    );

    // Case 4: multi-selection drag between worktrees.
    panel.update(cx, |panel, _| panel.marked_entries.clear());
    select_path_with_mark(&panel, "root_a/multi/alpha.txt", cx);
    select_path_with_mark(&panel, "root_a/multi/beta.txt", cx);
    drag_selection_to(&panel, "root_b/dst", false, cx);
    assert!(
        find_project_entry(&panel, "root_b/dst/alpha.txt", cx).is_some()
            && find_project_entry(&panel, "root_b/dst/beta.txt", cx).is_some(),
        "All marked entries should move to the destination worktree"
    );
    assert_eq!(
        find_project_entry(&panel, "root_a/multi/alpha.txt", cx),
        None,
        "Marked entries should be removed from the origin worktree"
    );
    assert_eq!(
        find_project_entry(&panel, "root_a/multi/beta.txt", cx),
        None,
        "Marked entries should be removed from the origin worktree"
    );
}

#[gpui::test]
async fn test_drag_multiple_entries(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "src": {
                "folder1": {
                    "mod.rs": "// folder1 mod"
                },
                "folder2": {
                    "mod.rs": "// folder2 mod"
                },
                "folder3": {
                    "mod.rs": "// folder3 mod",
                    "helper.rs": "// helper"
                },
                "main.rs": ""
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

    toggle_expand_dir(&panel, "root/src", cx);
    toggle_expand_dir(&panel, "root/src/folder1", cx);
    toggle_expand_dir(&panel, "root/src/folder2", cx);
    toggle_expand_dir(&panel, "root/src/folder3", cx);
    cx.run_until_parked();

    // Case 1: Dragging a folder and a file from a sibling folder together.
    panel.update(cx, |panel, _| panel.marked_entries.clear());
    select_path_with_mark(&panel, "root/src/folder1", cx);
    select_path_with_mark(&panel, "root/src/folder2/mod.rs", cx);

    drag_selection_to(&panel, "root", false, cx);

    assert!(
        find_project_entry(&panel, "root/folder1", cx).is_some(),
        "folder1 should be at root after drag"
    );
    assert!(
        find_project_entry(&panel, "root/folder1/mod.rs", cx).is_some(),
        "folder1/mod.rs should still be inside folder1 after drag"
    );
    assert_eq!(
        find_project_entry(&panel, "root/src/folder1", cx),
        None,
        "folder1 should no longer be in src"
    );
    assert!(
        find_project_entry(&panel, "root/mod.rs", cx).is_some(),
        "mod.rs from folder2 should be at root"
    );

    // Case 2: Dragging a folder and its own child together.
    panel.update(cx, |panel, _| panel.marked_entries.clear());
    select_path_with_mark(&panel, "root/src/folder3", cx);
    select_path_with_mark(&panel, "root/src/folder3/mod.rs", cx);

    drag_selection_to(&panel, "root", false, cx);

    assert!(
        find_project_entry(&panel, "root/folder3", cx).is_some(),
        "folder3 should be at root after drag"
    );
    assert!(
        find_project_entry(&panel, "root/folder3/mod.rs", cx).is_some(),
        "folder3/mod.rs should still be inside folder3"
    );
    assert!(
        find_project_entry(&panel, "root/folder3/helper.rs", cx).is_some(),
        "folder3/helper.rs should still be inside folder3"
    );
}
