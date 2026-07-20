use super::*;

async fn test_editing_files(cx: &mut gpui::TestAppContext) {
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
            editor.set_text("the-new-filename", window, cx)
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
            "      [PROCESSING: 'the-new-filename']  <== selected",
            "      .dockerignore",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    confirm.await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    > b",
            "    > C",
            "      .dockerignore",
            "      the-new-filename  <== selected  <== marked",
            "v root2",
            "    > d",
            "    > e",
        ]
    );

    select_path(&panel, "root1/b", cx);
    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > 3",
            "        > 4",
            "          [EDITOR: '']  <== selected",
            "    > C",
            "      .dockerignore",
            "      the-new-filename",
        ]
    );

    panel
        .update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                editor.set_text("another-filename.txt", window, cx)
            });
            panel.confirm_edit(true, window, cx).unwrap()
        })
        .await
        .unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > 3",
            "        > 4",
            "          another-filename.txt  <== selected  <== marked",
            "    > C",
            "      .dockerignore",
            "      the-new-filename",
        ]
    );

    select_path(&panel, "root1/b/another-filename.txt", cx);
    panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > 3",
            "        > 4",
            "          [EDITOR: 'another-filename.txt']  <== selected  <== marked",
            "    > C",
            "      .dockerignore",
            "      the-new-filename",
        ]
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            let file_name_selections = editor
                .selections
                .all::<MultiBufferOffset>(&editor.display_snapshot(cx));
            assert_eq!(
                file_name_selections.len(),
                1,
                "File editing should have a single selection, but got: {file_name_selections:?}"
            );
            let file_name_selection = &file_name_selections[0];
            assert_eq!(
                file_name_selection.start,
                MultiBufferOffset(0),
                "Should select the file name from the start"
            );
            assert_eq!(
                file_name_selection.end,
                MultiBufferOffset("another-filename".len()),
                "Should not select file extension"
            );

            editor.set_text("a-different-filename.tar.gz", window, cx)
        });
        panel.confirm_edit(true, window, cx).unwrap()
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > 3",
            "        > 4",
            "          [PROCESSING: 'a-different-filename.tar.gz']  <== selected  <== marked",
            "    > C",
            "      .dockerignore",
            "      the-new-filename",
        ]
    );

    confirm.await.unwrap();
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > 3",
            "        > 4",
            "          a-different-filename.tar.gz  <== selected",
            "    > C",
            "      .dockerignore",
            "      the-new-filename",
        ]
    );

    panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > 3",
            "        > 4",
            "          [EDITOR: 'a-different-filename.tar.gz']  <== selected",
            "    > C",
            "      .dockerignore",
            "      the-new-filename",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
            panel.filename_editor.update(cx, |editor, cx| {
                let file_name_selections = editor.selections.all::<MultiBufferOffset>(&editor.display_snapshot(cx));
                assert_eq!(file_name_selections.len(), 1, "File editing should have a single selection, but got: {file_name_selections:?}");
                let file_name_selection = &file_name_selections[0];
                assert_eq!(file_name_selection.start, MultiBufferOffset(0), "Should select the file name from the start");
                assert_eq!(file_name_selection.end, MultiBufferOffset("a-different-filename.tar".len()), "Should not select file extension, but still may select anything up to the last dot..");

            });
            panel.cancel(&menu::Cancel, window, cx)
        });
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        panel.new_directory(&NewDirectory, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > [EDITOR: '']  <== selected",
            "        > 3",
            "        > 4",
            "          a-different-filename.tar.gz",
            "    > C",
            "      .dockerignore",
        ]
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel
            .filename_editor
            .update(cx, |editor, cx| editor.set_text("new-dir", window, cx));
        panel.confirm_edit(true, window, cx).unwrap()
    });
    panel.update_in(cx, |panel, window, cx| {
        panel.select_next(&Default::default(), window, cx)
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > [PROCESSING: 'new-dir']",
            "        > 3  <== selected",
            "        > 4",
            "          a-different-filename.tar.gz",
            "    > C",
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
            "    > a",
            "    v b",
            "        > 3  <== selected",
            "        > 4",
            "        > new-dir",
            "          a-different-filename.tar.gz",
            "    > C",
            "      .dockerignore",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.rename(&Default::default(), window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > [EDITOR: '3']  <== selected",
            "        > 4",
            "        > new-dir",
            "          a-different-filename.tar.gz",
            "    > C",
            "      .dockerignore",
        ]
    );

    // Dismiss the rename editor when it loses focus.
    workspace.update_in(cx, |_, window, _| window.blur());
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        > 3  <== selected",
            "        > 4",
            "        > new-dir",
            "          a-different-filename.tar.gz",
            "    > C",
            "      .dockerignore",
        ]
    );

    // Test empty filename and filename with only whitespace
    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        v 3",
            "              [EDITOR: '']  <== selected",
            "              Q",
            "        > 4",
            "        > new-dir",
            "          a-different-filename.tar.gz",
        ]
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
        assert!(panel.confirm_edit(true, window, cx).is_none());
        panel.filename_editor.update(cx, |editor, cx| {
            editor.set_text("   ", window, cx);
        });
        assert!(panel.confirm_edit(true, window, cx).is_none());
        panel.cancel(&menu::Cancel, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root1",
            "    > .git",
            "    > a",
            "    v b",
            "        v 3  <== selected",
            "              Q",
            "        > 4",
            "        > new-dir",
            "          a-different-filename.tar.gz",
            "    > C",
        ]
    );
}

#[gpui::test]
async fn test_rename_folder_with_dot_selects_whole_name(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "my.folder": {},
            "archive.tar.gz": "",
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
    cx.run_until_parked();

    // Renaming a folder whose name contains a dot should pre-select the whole
    // name. The dot belongs to the directory name; it is not a file extension.
    select_path(&panel, "root1/my.folder", cx);
    panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));
    panel.update_in(cx, |panel, window, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            let selections = editor
                .selections
                .all::<MultiBufferOffset>(&editor.display_snapshot(cx));
            assert_eq!(selections.len(), 1);
            assert_eq!(selections[0].start, MultiBufferOffset(0));
            assert_eq!(
                selections[0].end,
                MultiBufferOffset("my.folder".len()),
                "Renaming a folder should select the whole name, including dots"
            );
        });
        panel.cancel(&Cancel, window, cx);
    });
    cx.run_until_parked();

    // Files keep the existing behavior: the last extension stays unselected.
    select_path(&panel, "root1/archive.tar.gz", cx);
    panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));
    panel.update_in(cx, |panel, _, cx| {
        panel.filename_editor.update(cx, |editor, cx| {
            let selections = editor
                .selections
                .all::<MultiBufferOffset>(&editor.display_snapshot(cx));
            assert_eq!(
                selections[0].end,
                MultiBufferOffset("archive.tar".len()),
                "Renaming a file should keep the last extension unselected"
            );
        });
    });
}
