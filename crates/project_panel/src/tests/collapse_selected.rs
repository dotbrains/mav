#[gpui::test]
async fn test_collapse_selected_entry_and_children_action(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "subdir1": {
                    "nested1": {
                        "file1.txt": "",
                        "file2.txt": ""
                    },
                },
                "subdir2": {
                    "file3.txt": ""
                }
            },
            "dir2": {
                "file4.txt": ""
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
    toggle_expand_dir(&panel, "root/dir1/subdir1/nested1", cx);
    toggle_expand_dir(&panel, "root/dir1/subdir2", cx);
    toggle_expand_dir(&panel, "root/dir2", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1",
            "        v subdir1",
            "            v nested1",
            "                  file1.txt",
            "                  file2.txt",
            "        v subdir2",
            "              file3.txt",
            "    v dir2  <== selected",
            "          file4.txt",
        ],
        "Initial state with directories expanded"
    );

    select_path(&panel, "root/dir1", cx);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_selected_entry_and_children(&CollapseSelectedEntryAndChildren, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    > dir1  <== selected",
            "    v dir2",
            "          file4.txt",
        ],
        "dir1 and all its children should be collapsed, dir2 should remain expanded"
    );

    toggle_expand_dir(&panel, "root/dir1", cx);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "        > subdir1",
            "        > subdir2",
            "    v dir2",
            "          file4.txt",
        ],
        "After re-expanding dir1, its children should still be collapsed"
    );
}
