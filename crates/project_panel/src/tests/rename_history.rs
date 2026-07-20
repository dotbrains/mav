use super::*;

#[gpui::test]
#[cfg_attr(target_os = "windows", ignore)]
async fn test_rename_item_and_check_history(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/src",
        json!({
            "test": {
                "first.txt": "// First Txt file",
                "second.txt": "// Second Txt file",
                "third.txt": "// Third Txt file",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/src".as_ref()], cx).await;
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

    select_path(&panel, "src", cx);
    panel.update_in(cx, |panel, window, cx| panel.confirm(&Confirm, window, cx));
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src  <== selected",
            "    > test"
        ]
    );

    select_path(&panel, "src/test", cx);
    panel.update_in(cx, |panel, window, cx| panel.confirm(&Confirm, window, cx));
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            //
            "v src",
            "    > test  <== selected"
        ]
    );
    panel.update_in(cx, |panel, window, cx| panel.new_file(&NewFile, window, cx));
    cx.run_until_parked();
    panel.update_in(cx, |panel, window, cx| {
        assert!(panel.filename_editor.read(cx).is_focused(window));
    });

    select_path(&panel, "src/test/first.txt", cx);
    panel.update_in(cx, |panel, window, cx| panel.open(&Open, window, cx));
    cx.executor().run_until_parked();

    panel.update_in(cx, |panel, window, cx| panel.rename(&Rename, window, cx));
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v src",
            "    v test",
            "          [EDITOR: 'first.txt']  <== selected  <== marked",
            "          second.txt",
            "          third.txt"
        ],
    );

    let confirm = panel.update_in(cx, |panel, window, cx| {
        panel
            .filename_editor
            .update(cx, |editor, cx| editor.set_text("fourth.txt", window, cx));
        panel.confirm_edit(true, window, cx).unwrap()
    });
    confirm.await.unwrap();
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v src",
            "    v test",
            "          fourth.txt  <== selected",
            "          second.txt",
            "          third.txt"
        ],
        "File list should be different after rename confirmation"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.executor().run_until_parked();

    select_path(&panel, "src/test/second.txt", cx);
    panel.update_in(cx, |panel, window, cx| panel.open(&Open, window, cx));
    cx.executor().run_until_parked();

    workspace.read_with(cx, |this, cx| {
        assert!(
            this.recent_navigation_history_iter(cx)
                .any(|(project_path, abs_path)| {
                    project_path.path == Arc::from(rel_path("test/fourth.txt"))
                        && abs_path == Some(PathBuf::from(path!("/src/test/fourth.txt")))
                })
        );
    });
}
