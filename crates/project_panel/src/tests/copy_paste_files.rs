use super::*;

#[gpui::test]
async fn test_copy_paste(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "one.two.txt": "",
            "one.txt": ""
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/root1".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.select_next(&Default::default(), window, cx);
        panel.select_next(&Default::default(), window, cx);
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root1",
            "      one.txt  <== selected",
            "      one.two.txt",
        ]
    );

    // Regression test - file name is created correctly when
    // the copied file's name contains multiple dots.
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root1",
            "      one.txt",
            "      [EDITOR: 'one copy.txt']  <== selected  <== marked",
            "      one.two.txt",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
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
                "Should select from the beginning of the filename"
            );
            assert_eq!(
                file_name_selection.end,
                MultiBufferOffset("one copy".len()),
                "Should select the file name disambiguation until the extension"
            );
        });
        assert!(panel.confirm_edit(true, window, cx).is_none());
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root1",
            "      one.txt",
            "      one copy.txt",
            "      [EDITOR: 'one copy 1.txt']  <== selected  <== marked",
            "      one.two.txt",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.confirm_edit(true, window, cx).is_none())
    });
}

#[gpui::test]
async fn test_cut_paste(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "one.txt": "",
            "two.txt": "",
            "a": {},
            "b": {}
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

    select_path_with_mark(&panel, "root/one.txt", cx);
    select_path_with_mark(&panel, "root/two.txt", cx);

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "    > a",
            "    > b",
            "      one.txt  <== marked",
            "      two.txt  <== selected  <== marked",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.cut(&Default::default(), window, cx);
    });

    select_path(&panel, "root/a", cx);

    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "    v a",
            "          one.txt  <== marked",
            "          two.txt  <== selected  <== marked",
            "    > b",
        ],
        "Cut entries should be moved on first paste."
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.cancel(&menu::Cancel {}, window, cx)
    });
    cx.executor().run_until_parked();

    select_path(&panel, "root/b", cx);

    panel.update_in(cx, |panel, window, cx| {
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            "v root",
            "    v a",
            "          one.txt",
            "          two.txt",
            "    v b",
            "          one.txt",
            "          two.txt  <== selected",
        ],
        "Cut entries should only be copied for the second paste!"
    );
}

#[gpui::test]
async fn test_cut_paste_between_different_worktrees(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "one.txt": "",
            "two.txt": "",
            "three.txt": "",
            "a": {
                "0": { "q": "", "r": "", "s": "" },
                "1": { "t": "", "u": "" },
                "2": { "v": "", "w": "", "x": "", "y": "" },
            },
        }),
    )
    .await;

    fs.insert_tree(
        "/root2",
        json!({
            "one.txt": "",
            "two.txt": "",
            "four.txt": "",
            "b": {
                "3": { "Q": "" },
                "4": { "R": "", "S": "", "T": "", "U": "" },
            },
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

    select_path(&panel, "root1/three.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.cut(&Default::default(), window, cx);
    });

    select_path(&panel, "root2/one.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.select_next(&Default::default(), window, cx);
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root1",
            "    > a",
            "      one.txt",
            "      two.txt",
            "v root2",
            "    > b",
            "      four.txt",
            "      one.txt",
            "      three.txt  <== selected  <== marked",
            "      two.txt",
        ]
    );

    select_path(&panel, "root1/a", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.cut(&Default::default(), window, cx);
    });
    select_path(&panel, "root2/two.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.select_next(&Default::default(), window, cx);
        panel.paste(&Default::default(), window, cx);
    });

    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root1",
            "      one.txt",
            "      two.txt",
            "v root2",
            "    > a  <== selected",
            "    > b",
            "      four.txt",
            "      one.txt",
            "      three.txt  <== marked",
            "      two.txt",
        ]
    );
}

#[gpui::test]
async fn test_copy_paste_between_different_worktrees(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root1",
        json!({
            "one.txt": "",
            "two.txt": "",
            "three.txt": "",
            "a": {
                "0": { "q": "", "r": "", "s": "" },
                "1": { "t": "", "u": "" },
                "2": { "v": "", "w": "", "x": "", "y": "" },
            },
        }),
    )
    .await;

    fs.insert_tree(
        "/root2",
        json!({
            "one.txt": "",
            "two.txt": "",
            "four.txt": "",
            "b": {
                "3": { "Q": "" },
                "4": { "R": "", "S": "", "T": "", "U": "" },
            },
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

    select_path(&panel, "root1/three.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
    });

    select_path(&panel, "root2/one.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.select_next(&Default::default(), window, cx);
        panel.paste(&Default::default(), window, cx);
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root1",
            "    > a",
            "      one.txt",
            "      three.txt",
            "      two.txt",
            "v root2",
            "    > b",
            "      four.txt",
            "      one.txt",
            "      three.txt  <== selected  <== marked",
            "      two.txt",
        ]
    );

    select_path(&panel, "root1/three.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
    });
    select_path(&panel, "root2/two.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.select_next(&Default::default(), window, cx);
        panel.paste(&Default::default(), window, cx);
    });

    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root1",
            "    > a",
            "      one.txt",
            "      three.txt",
            "      two.txt",
            "v root2",
            "    > b",
            "      four.txt",
            "      one.txt",
            "      three.txt",
            "      [EDITOR: 'three copy.txt']  <== selected  <== marked",
            "      two.txt",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.cancel(&menu::Cancel {}, window, cx)
    });
    cx.executor().run_until_parked();

    select_path(&panel, "root1/a", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.copy(&Default::default(), window, cx);
    });
    select_path(&panel, "root2/two.txt", cx);
    panel.update_in(cx, |panel, window, cx| {
        panel.select_next(&Default::default(), window, cx);
        panel.paste(&Default::default(), window, cx);
    });

    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..50, cx),
        &[
            //
            "v root1",
            "    > a",
            "      one.txt",
            "      three.txt",
            "      two.txt",
            "v root2",
            "    > a  <== selected",
            "    > b",
            "      four.txt",
            "      one.txt",
            "      three.txt",
            "      three copy.txt",
            "      two.txt",
        ]
    );
}
