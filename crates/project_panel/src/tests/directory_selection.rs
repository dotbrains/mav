use super::*;

#[gpui::test]
async fn test_select_directory(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project_root",
        json!({
            "dir_1": {
                "nested_dir": {
                    "file_a.py": "# File contents",
                }
            },
            "file_1.py": "# File contents",
            "dir_2": {

            },
            "dir_3": {

            },
            "file_2.py": "# File contents",
            "dir_4": {

            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/project_root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| panel.open(&Open, window, cx));
    cx.executor().run_until_parked();
    select_path(&panel, "project_root/dir_1", cx);
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    > dir_1  <== selected",
            "    > dir_2",
            "    > dir_3",
            "    > dir_4",
            "      file_1.py",
            "      file_2.py",
        ]
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.select_prev_directory(&SelectPrevDirectory, window, cx)
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root  <== selected",
            "    > dir_1",
            "    > dir_2",
            "    > dir_3",
            "    > dir_4",
            "      file_1.py",
            "      file_2.py",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_prev_directory(&SelectPrevDirectory, window, cx)
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    > dir_1",
            "    > dir_2",
            "    > dir_3",
            "    > dir_4  <== selected",
            "      file_1.py",
            "      file_2.py",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_next_directory(&SelectNextDirectory, window, cx)
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root  <== selected",
            "    > dir_1",
            "    > dir_2",
            "    > dir_3",
            "    > dir_4",
            "      file_1.py",
            "      file_2.py",
        ]
    );
}

#[gpui::test]
async fn test_select_first_last(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project_root",
        json!({
            "dir_1": {
                "nested_dir": {
                    "file_a.py": "# File contents",
                }
            },
            "file_1.py": "# File contents",
            "file_2.py": "# File contents",
            "zdir_2": {
                "nested_dir2": {
                    "file_b.py": "# File contents",
                }
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/project_root".as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    > dir_1",
            "    > zdir_2",
            "      file_1.py",
            "      file_2.py",
        ]
    );
    panel.update_in(cx, |panel, window, cx| {
        panel.select_first(&SelectFirst, window, cx)
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root  <== selected",
            "    > dir_1",
            "    > zdir_2",
            "      file_1.py",
            "      file_2.py",
        ]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_last(&SelectLast, window, cx)
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    > dir_1",
            "    > zdir_2",
            "      file_1.py",
            "      file_2.py  <== selected",
        ]
    );

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                hide_root: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    #[rustfmt::skip]
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "> dir_1",
            "> zdir_2",
            "  file_1.py",
            "  file_2.py",
        ],
        "With hide_root=true, root should be hidden"
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.select_first(&SelectFirst, window, cx)
    });

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "> dir_1  <== selected",
            "> zdir_2",
            "  file_1.py",
            "  file_2.py",
        ],
        "With hide_root=true, first entry should be dir_1, not the hidden root"
    );
}
