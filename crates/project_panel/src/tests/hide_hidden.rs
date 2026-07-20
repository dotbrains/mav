#[gpui::test]
async fn test_hide_hidden_entries(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            ".hidden-file.txt": "hidden file content",
            "visible-file.txt": "visible file content",
            ".hidden-parent-dir": {
                "nested-dir": {
                    "file.txt": "file content",
                }
            },
            "visible-dir": {
                "file-in-visible.txt": "file content",
                "nested": {
                    ".hidden-nested-dir": {
                        ".double-hidden-dir": {
                            "deep-file-1.txt": "deep content 1",
                            "deep-file-2.txt": "deep content 2"
                        },
                        "hidden-nested-file-1.txt": "hidden nested 1",
                        "hidden-nested-file-2.txt": "hidden nested 2"
                    },
                    "visible-nested-file.txt": "visible nested content"
                }
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

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_hidden: false,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    toggle_expand_dir(&panel, "root/.hidden-parent-dir", cx);
    toggle_expand_dir(&panel, "root/.hidden-parent-dir/nested-dir", cx);
    toggle_expand_dir(&panel, "root/visible-dir", cx);
    toggle_expand_dir(&panel, "root/visible-dir/nested", cx);
    toggle_expand_dir(&panel, "root/visible-dir/nested/.hidden-nested-dir", cx);
    toggle_expand_dir(
        &panel,
        "root/visible-dir/nested/.hidden-nested-dir/.double-hidden-dir",
        cx,
    );

    let expanded = [
        "v root",
        "    v .hidden-parent-dir",
        "        v nested-dir",
        "              file.txt",
        "    v visible-dir",
        "        v nested",
        "            v .hidden-nested-dir",
        "                v .double-hidden-dir  <== selected",
        "                      deep-file-1.txt",
        "                      deep-file-2.txt",
        "                  hidden-nested-file-1.txt",
        "                  hidden-nested-file-2.txt",
        "              visible-nested-file.txt",
        "          file-in-visible.txt",
        "      .hidden-file.txt",
        "      visible-file.txt",
    ];

    assert_eq!(
        visible_entries_as_strings(&panel, 0..30, cx),
        &expanded,
        "With hide_hidden=false, contents of hidden nested directory should be visible"
    );

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_hidden: true,
                ..settings
            },
            cx,
        );
    });

    panel.update_in(cx, |panel, window, cx| {
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..30, cx),
        &[
            "v root",
            "    v visible-dir",
            "        v nested",
            "              visible-nested-file.txt",
            "          file-in-visible.txt",
            "      visible-file.txt",
        ],
        "With hide_hidden=false, contents of hidden nested directory should be visible"
    );

    panel.update_in(cx, |panel, window, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_hidden: false,
                ..settings
            },
            cx,
        );
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..30, cx),
        &expanded,
        "With hide_hidden=false, deeply nested hidden directories and their contents should be visible"
    );
}
