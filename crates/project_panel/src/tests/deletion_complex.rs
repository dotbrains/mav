use super::*;

#[gpui::test]
async fn test_complex_selection_scenarios(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "subdir1": {
                    "a.txt": "",
                    "b.txt": ""
                },
                "file1.txt": "",
            },
            "dir2": {
                "subdir2": {
                    "c.txt": "",
                    "d.txt": ""
                },
                "file2.txt": "",
            },
            "file3.txt": "",
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

    toggle_expand_dir(&panel, "root/dir1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);
    toggle_expand_dir(&panel, "root/dir2/subdir2", cx);

    // Test Case 1: Select and delete nested directory with parent
    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });
    select_path_with_mark(&panel, "root/dir1/subdir1", cx);
    select_path_with_mark(&panel, "root/dir1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1  <== selected  <== marked",
            "        v subdir1  <== marked",
            "              a.txt",
            "              b.txt",
            "          file1.txt",
            "    v dir2",
            "        v subdir2",
            "              c.txt",
            "              d.txt",
            "          file2.txt",
            "      file3.txt",
        ],
        "Initial state before deleting nested directory with parent"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir2  <== selected",
            "        v subdir2",
            "              c.txt",
            "              d.txt",
            "          file2.txt",
            "      file3.txt",
        ],
        "Should select next directory after deleting directory with parent"
    );

    // Test Case 2: Select mixed files and directories across levels
    select_path_with_mark(&panel, "root/dir2/subdir2/c.txt", cx);
    select_path_with_mark(&panel, "root/dir2/file2.txt", cx);
    select_path_with_mark(&panel, "root/file3.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir2",
            "        v subdir2",
            "              c.txt  <== marked",
            "              d.txt",
            "          file2.txt  <== marked",
            "      file3.txt  <== selected  <== marked",
        ],
        "Initial state before deleting"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir2  <== selected",
            "        v subdir2",
            "              d.txt",
        ],
        "Should select sibling directory"
    );
}

#[gpui::test]
async fn test_delete_all_files_and_directories(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "subdir1": {
                    "a.txt": "",
                    "b.txt": ""
                },
                "file1.txt": "",
            },
            "dir2": {
                "subdir2": {
                    "c.txt": "",
                    "d.txt": ""
                },
                "file2.txt": "",
            },
            "file3.txt": "",
            "file4.txt": "",
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

    toggle_expand_dir(&panel, "root/dir1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);
    toggle_expand_dir(&panel, "root/dir2/subdir2", cx);

    // Test Case 1: Select all root files and directories
    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });
    select_path_with_mark(&panel, "root/dir1", cx);
    select_path_with_mark(&panel, "root/dir2", cx);
    select_path_with_mark(&panel, "root/file3.txt", cx);
    select_path_with_mark(&panel, "root/file4.txt", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== marked",
            "        v subdir1",
            "              a.txt",
            "              b.txt",
            "          file1.txt",
            "    v dir2  <== marked",
            "        v subdir2",
            "              c.txt",
            "              d.txt",
            "          file2.txt",
            "      file3.txt  <== marked",
            "      file4.txt  <== selected  <== marked",
        ],
        "State before deleting all contents"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root  <== selected"],
        "Only empty root directory should remain after deleting all contents"
    );
}

#[gpui::test]
async fn test_nested_selection_deletion(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "subdir1": {
                    "file_a.txt": "content a",
                    "file_b.txt": "content b",
                },
                "subdir2": {
                    "file_c.txt": "content c",
                },
                "file1.txt": "content 1",
            },
            "dir2": {
                "file2.txt": "content 2",
            },
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

    toggle_expand_dir(&panel, "root/dir1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);
    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });

    // Test Case 1: Select parent directory, subdirectory, and a file inside the subdirectory
    select_path_with_mark(&panel, "root/dir1", cx);
    select_path_with_mark(&panel, "root/dir1/subdir1", cx);
    select_path_with_mark(&panel, "root/dir1/subdir1/file_a.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== marked",
            "        v subdir1  <== marked",
            "              file_a.txt  <== selected  <== marked",
            "              file_b.txt",
            "        > subdir2",
            "          file1.txt",
            "    v dir2",
            "          file2.txt",
        ],
        "State with parent dir, subdir, and file selected"
    );
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root", "    v dir2  <== selected", "          file2.txt",],
        "Only dir2 should remain after deletion"
    );
}

#[gpui::test]
async fn test_multiple_worktrees_deletion(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    // First worktree
    fs.insert_tree(
        "/root1",
        json!({
            "dir1": {
                "file1.txt": "content 1",
                "file2.txt": "content 2",
            },
            "dir2": {
                "file3.txt": "content 3",
            },
        }),
    )
    .await;

    // Second worktree
    fs.insert_tree(
        "/root2",
        json!({
            "dir3": {
                "file4.txt": "content 4",
                "file5.txt": "content 5",
            },
            "file6.txt": "content 6",
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

    // Expand all directories for testing
    toggle_expand_dir(&panel, "root1/dir1", cx);
    toggle_expand_dir(&panel, "root1/dir2", cx);
    toggle_expand_dir(&panel, "root2/dir3", cx);

    // Test Case 1: Delete files across different worktrees
    cx.simulate_modifiers_change(gpui::Modifiers {
        control: true,
        ..Default::default()
    });
    select_path_with_mark(&panel, "root1/dir1/file1.txt", cx);
    select_path_with_mark(&panel, "root2/dir3/file4.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1",
            "          file1.txt  <== marked",
            "          file2.txt",
            "    v dir2",
            "          file3.txt",
            "v root2",
            "    v dir3",
            "          file4.txt  <== selected  <== marked",
            "          file5.txt",
            "      file6.txt",
        ],
        "Initial state with files selected from different worktrees"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1",
            "          file2.txt",
            "    v dir2",
            "          file3.txt",
            "v root2",
            "    v dir3",
            "          file5.txt  <== selected",
            "      file6.txt",
        ],
        "Should select next file in the last worktree after deletion"
    );

    // Test Case 2: Delete directories from different worktrees
    select_path_with_mark(&panel, "root1/dir1", cx);
    select_path_with_mark(&panel, "root2/dir3", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1  <== marked",
            "          file2.txt",
            "    v dir2",
            "          file3.txt",
            "v root2",
            "    v dir3  <== selected  <== marked",
            "          file5.txt",
            "      file6.txt",
        ],
        "State with directories marked from different worktrees"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir2",
            "          file3.txt",
            "v root2",
            "      file6.txt  <== selected",
        ],
        "Should select remaining file in last worktree after directory deletion"
    );

    // Test Case 4: Delete all remaining files except roots
    select_path_with_mark(&panel, "root1/dir2/file3.txt", cx);
    select_path_with_mark(&panel, "root2/file6.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir2",
            "          file3.txt  <== marked",
            "v root2",
            "      file6.txt  <== selected  <== marked",
        ],
        "State with all remaining files marked"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["v root1", "    v dir2", "v root2  <== selected"],
        "Second parent root should be selected after deleting"
    );
}
