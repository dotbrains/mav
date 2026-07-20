use super::*;

async fn test_adding_directories_via_file(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            ".dockerignore": "",
            ".git": {
                "HEAD": "",
            },
            "a": {
                "0": { "q": "", "r": "", "s": "" },
                "1": { "t": "", "u": "" },
                "2": { "v": "", "w": "", "x": "", "y": "" },
            },
            "b": {
                "3": { "Q": "" },
                "4": { "R": "", "S": "", "T": "", "U": "" },
            },
            "C": {
                "5": {},
                "6": { "V": "", "W": "" },
                "7": { "X": "" },
                "8": { "Y": {}, "Z": "" }
            }
        }),
    )
    .await;
    fs.insert_tree(
        "/root2",
        json!({
            "d": {
                "9": ""
            },
            "e": {}
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref(), "/root2".as_ref()], cx).await;
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
    cx.run_until_parked();

    select_path(&panel, "root1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1  <== selected",
            "    > .git",
            "    > a",
            "    > b",
            "    > C",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    // Add a file with the root folder selected. The filename editor is placed
    // before the first file in the root folder.
    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    > b",
            "    > C",
            "      [EDITOR: '']  <== selected",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("/bdir1/dir2/the-new-filename", window, cx)
        });
        panel.confirm_edit(true, window, cx).unwrap()
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    > b",
            "    > C",
            "      [PROCESSING: 'bdir1/dir2/the-new-filename']  <== selected",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    confirm.await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..13, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    > b",
            "    v bdir1",
            "        v dir2",
            "              the-new-filename  <== selected  <== marked",
            "    > C",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );
}

#[gpui::test]
async fn test_adding_directory_via_file(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root1"),
        json!({
            ".dockerignore": "",
            ".git": {
                "HEAD": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root1").as_ref()], cx).await;
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
    cx.run_until_parked();

    select_path(&panel, "root1", cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root1  <== selected", "    > .git", "      .dockerignore",]
    );

    // Add a file with the root folder selected. The filename editor is placed
    // before the first file in the root folder.
    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "      [EDITOR: '']  <== selected",
            "      .dockerignore",
        ]
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        // If we want to create a subdirectory, there should be no prefix slash.
        panel
            .filename_editor
            .update(cx, |editor, cx| editor.set_text("new_dir/", window, cx));
        panel.confirm_edit(true, window, cx).unwrap()
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "      [PROCESSING: 'new_dir']  <== selected",
            "      .dockerignore",
        ]
    );

    confirm.await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    v new_dir  <== selected",
            "      .dockerignore",
        ]
    );

    // Test filename with whitespace
    select_path(&panel, "root1", cx);
    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    let confirm = panel.update_in(cx, |panel, window, cx| {
        // If we want to create a subdirectory, there should be no prefix slash.
        panel
            .filename_editor
            .update(cx, |editor, cx| editor.set_text("new dir 2/", window, cx));
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    v new dir 2  <== selected",
            "    v new_dir",
            "      .dockerignore",
        ]
    );

    // Test filename ends with "\"
    #[cfg(target_os = "windows")]
    {
        select_path(&panel, "root1", cx);
        panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
        let confirm = panel.update_in(cx, |panel, window, cx| {
            // If we want to create a subdirectory, there should be no prefix slash.
            panel
                .filename_editor
                .update(cx, |editor, cx| editor.set_text("new_dir_3\\", window, cx));
            panel.confirm_edit(true, window, cx).unwrap()
        });
        confirm.await.unwrap();
        cx.run_until_parked();
        assert_eq!(
            visible_entries_as_strings(&panel, 0..10, cx),
            &[
                "v root1",
                "    > .git",
                "    v new dir 2",
                "    v new_dir",
                "    v new_dir_3  <== selected",
                "      .dockerignore",
            ]
        );
    }
}
