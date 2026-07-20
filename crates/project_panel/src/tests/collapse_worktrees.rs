#[gpui::test]
async fn test_collapse_root_single_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "subdir1": {
                    "file1.txt": ""
                },
                "file2.txt": ""
            },
            "dir2": {
                "file3.txt": ""
            }
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

    toggle_expand_dir(&panel, "root/dir1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1",
            "        v subdir1",
            "              file1.txt",
            "          file2.txt",
            "    v dir2  <== selected",
            "          file3.txt",
        ],
        "Initial state with directories expanded"
    );

    // Select the root and collapse it and its children
    select_path(&panel, "root", cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_selected_entry_and_children(&CollapseSelectedEntryAndChildren, window, cx);
    });
    cx.run_until_parked();

    // The root and all its children should be collapsed
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &["> root  <== selected"],
        "Root and all children should be collapsed"
    );

    // Re-expand root and dir1, verify children were recursively collapsed
    toggle_expand_dir(&panel, "root", cx);
    toggle_expand_dir(&panel, "root/dir1", cx);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        > subdir1",
            "          file2.txt",
            "    > dir2",
        ],
        "After re-expanding root and dir1, subdir1 should still be collapsed"
    );
}

#[gpui::test]
async fn test_collapse_root_multi_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root1"),
        json!({
            "dir1": {
                "subdir1": {
                    "file1.txt": ""
                },
                "file2.txt": ""
            }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/root2"),
        json!({
            "dir2": {
                "file3.txt": ""
            },
            "file4.txt": ""
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [path!("/root1").as_ref(), path!("/root2").as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root1/dir1", cx);
    toggle_expand_dir(&panel, "root1/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root2/dir2", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1",
            "        v subdir1",
            "              file1.txt",
            "          file2.txt",
            "v root2",
            "    v dir2  <== selected",
            "          file3.txt",
            "      file4.txt",
        ],
        "Initial state with directories expanded across worktrees"
    );

    // Select root1 and collapse it and its children.
    // In a multi-worktree project, this should only collapse the selected worktree,
    // leaving other worktrees unaffected.
    select_path(&panel, "root1", cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_selected_entry_and_children(&CollapseSelectedEntryAndChildren, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> root1  <== selected",
            "v root2",
            "    v dir2",
            "          file3.txt",
            "      file4.txt",
        ],
        "Only root1 should be collapsed, root2 should remain expanded"
    );

    // Re-expand root1 and verify its children were recursively collapsed
    toggle_expand_dir(&panel, "root1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1  <== selected",
            "    > dir1",
            "v root2",
            "    v dir2",
            "          file3.txt",
            "      file4.txt",
        ],
        "After re-expanding root1, dir1 should still be collapsed, root2 should be unaffected"
    );
}

#[gpui::test]
async fn test_collapse_non_root_multi_worktree(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root1"),
        json!({
            "dir1": {
                "subdir1": {
                    "file1.txt": ""
                },
                "file2.txt": ""
            }
        }),
    )
    .await;
    fs.insert_tree(
        path!("/root2"),
        json!({
            "dir2": {
                "subdir2": {
                    "file3.txt": ""
                },
                "file4.txt": ""
            }
        }),
    )
    .await;

    let project = Project::test(
        fs.clone(),
        [path!("/root1").as_ref(), path!("/root2").as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root1/dir1", cx);
    toggle_expand_dir(&panel, "root1/dir1/subdir1", cx);
    toggle_expand_dir(&panel, "root2/dir2", cx);
    toggle_expand_dir(&panel, "root2/dir2/subdir2", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1",
            "        v subdir1",
            "              file1.txt",
            "          file2.txt",
            "v root2",
            "    v dir2",
            "        v subdir2  <== selected",
            "              file3.txt",
            "          file4.txt",
        ],
        "Initial state with directories expanded across worktrees"
    );

    // Select dir1 in root1 and collapse it
    select_path(&panel, "root1/dir1", cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_selected_entry_and_children(&CollapseSelectedEntryAndChildren, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    > dir1  <== selected",
            "v root2",
            "    v dir2",
            "        v subdir2",
            "              file3.txt",
            "          file4.txt",
        ],
        "Only dir1 should be collapsed, root2 should be completely unaffected"
    );

    // Re-expand dir1 and verify subdir1 was recursively collapsed
    toggle_expand_dir(&panel, "root1/dir1", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root1",
            "    v dir1  <== selected",
            "        > subdir1",
            "          file2.txt",
            "v root2",
            "    v dir2",
            "        v subdir2",
            "              file3.txt",
            "          file4.txt",
        ],
        "After re-expanding dir1, subdir1 should still be collapsed"
    );
}
