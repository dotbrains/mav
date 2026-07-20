use super::*;

#[gpui::test]
async fn test_basic_file_deletion_scenarios(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "dir1": {
                "subdir1": {},
                "file1.txt": "",
                "file2.txt": "",
            },
            "dir2": {
                "subdir2": {},
                "file3.txt": "",
                "file4.txt": "",
            },
            "file5.txt": "",
            "file6.txt": "",
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
    toggle_expand_dir(&panel, "root/dir2", cx);

    // Test Case 1: Delete middle file in directory
    select_path(&panel, "root/dir1/file1.txt", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1",
            "          file1.txt  <== selected",
            "          file2.txt",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt",
            "      file6.txt",
        ],
        "Initial state before deleting middle file"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1",
            "          file2.txt  <== selected",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt",
            "      file6.txt",
        ],
        "Should select next file after deleting middle file"
    );

    // Test Case 2: Delete last file in directory
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1  <== selected",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt",
            "      file6.txt",
        ],
        "Should select next directory when last file is deleted"
    );

    // Test Case 3: Delete root level file
    select_path(&panel, "root/file6.txt", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt",
            "      file6.txt  <== selected",
        ],
        "Initial state before deleting root level file"
    );

    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..15, cx),
        &[
            "v root",
            "    v dir1",
            "        > subdir1",
            "    v dir2",
            "        > subdir2",
            "          file3.txt",
            "          file4.txt",
            "      file5.txt  <== selected",
        ],
        "Should select prev entry at root level"
    );
}

#[gpui::test]
async fn test_deletion_gitignored(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "aa": "// Testing 1",
            "bb": "// Testing 2",
            "cc": "// Testing 3",
            "dd": "// Testing 4",
            "ee": "// Testing 5",
            "ff": "// Testing 6",
            "gg": "// Testing 7",
            "hh": "// Testing 8",
            "ii": "// Testing 8",
            ".gitignore": "bb\ndd\nee\nff\nii\n'",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);

    // Test 1: Auto selection with one gitignored file next to the deleted file
    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_gitignore: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    select_path(&panel, "root/aa", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "      .gitignore",
            "      aa  <== selected",
            "      cc",
            "      gg",
            "      hh"
        ],
        "Initial state should hide files on .gitignore"
    );

    submit_deletion(&panel, cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "      .gitignore",
            "      cc  <== selected",
            "      gg",
            "      hh"
        ],
        "Should select next entry not on .gitignore"
    );

    // Test 2: Auto selection with many gitignored files next to the deleted file
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "      .gitignore",
            "      gg  <== selected",
            "      hh"
        ],
        "Should select next entry not on .gitignore"
    );

    // Test 3: Auto selection of entry before deleted file
    select_path(&panel, "root/hh", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "      .gitignore",
            "      gg",
            "      hh  <== selected"
        ],
        "Should select next entry not on .gitignore"
    );
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root", "      .gitignore", "      gg  <== selected"],
        "Should select next entry not on .gitignore"
    );
}

#[gpui::test]
async fn test_nested_deletion_gitignore(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "file1": "// Testing",
                "file2": "// Testing",
                "file3": "// Testing"
            },
            "aa": "// Testing",
            ".gitignore": "file1\nfile3\n",
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
                hide_gitignore: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Test 1: Visible items should exclude files on gitignore
    toggle_expand_dir(&panel, "root/dir1", cx);
    select_path(&panel, "root/dir1/file2", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "    v dir1",
            "          file2  <== selected",
            "      .gitignore",
            "      aa"
        ],
        "Initial state should hide files on .gitignore"
    );
    submit_deletion(&panel, cx);

    // Test 2: Auto selection should go to the parent
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "    v dir1  <== selected",
            "      .gitignore",
            "      aa"
        ],
        "Initial state should hide files on .gitignore"
    );
}
