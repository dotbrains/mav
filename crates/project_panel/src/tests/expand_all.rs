#[gpui::test]
async fn test_expand_all_entries(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/project_root",
        json!({
            "dir_1": {
                "nested_dir": {
                    "file_a.py": "# File contents",
                    "file_b.py": "# File contents",
                    "file_c.py": "# File contents",
                },
                "file_1.py": "# File contents",
                "file_2.py": "# File contents",
                "file_3.py": "# File contents",
            },
            "dir_2": {
                "file_1.py": "# File contents",
                "file_2.py": "# File contents",
                "file_3.py": "# File contents",
            }
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

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v project_root", "    > dir_1", "    > dir_2",]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.expand_all_entries(&ExpandAllEntries, window, cx)
    });
    cx.executor().run_until_parked();

    let entries = visible_entries_as_strings(&panel, 0..20, cx);
    assert_eq!(entries.len(), 13, "should show all 13 entries");
    assert!(entries[0].starts_with("v project_root"), "root expanded");
    assert!(entries[1].contains("v dir_1"), "dir_1 expanded");
    assert!(entries[2].contains("v nested_dir"), "nested_dir expanded");
    assert!(
        entries.iter().any(|e| e.contains("file_a.py")),
        "file_a visible"
    );
    assert!(
        entries.iter().any(|e| e.contains("file_c.py")),
        "file_c visible"
    );
    assert!(
        entries.iter().any(|e| e.contains("v dir_2")),
        "dir_2 expanded"
    );
    assert!(
        !entries.iter().any(|e| e.contains("> ")),
        "no collapsed dirs"
    );
}

#[gpui::test]
async fn test_expand_all_entries_multiple_worktrees(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    let worktree_content = json!({
        "dir_1": {
            "file_1.py": "# File contents",
        },
        "dir_2": {
            "file_1.py": "# File contents",
        }
    });

    fs.insert_tree("/project_root_1", worktree_content.clone())
        .await;
    fs.insert_tree("/project_root_2", worktree_content).await;

    let project = Project::test(
        fs.clone(),
        ["/project_root_1".as_ref(), "/project_root_2".as_ref()],
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

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["> project_root_1", "> project_root_2",]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.expand_all_entries(&ExpandAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root_1",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
            "v project_root_2",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
        ]
    );
}

#[gpui::test]
async fn test_expand_all_entries_via_window_dispatch(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    let worktree_content = json!({
        "dir_1": {
            "file_1.py": "# File contents",
        },
        "dir_2": {
            "file_1.py": "# File contents",
        }
    });

    fs.insert_tree("/project_root_1", worktree_content.clone())
        .await;
    fs.insert_tree("/project_root_2", worktree_content).await;

    let project = Project::test(
        fs.clone(),
        ["/project_root_1".as_ref(), "/project_root_2".as_ref()],
        cx,
    )
    .await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_reveal_entries: false,
                ..settings
            },
            cx,
        );
    });
    let panel = workspace.update_in(cx, |workspace, window, cx| {
        let panel = ProjectPanel::new(workspace, window, cx);
        workspace.add_panel(panel.clone(), window, cx);
        panel
    });
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["> project_root_1", "> project_root_2",]
    );

    panel.update_in(cx, |panel, window, cx| {
        panel.focus_handle(cx).focus(window, cx);
    });
    cx.dispatch_action(ExpandAllEntries);
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v project_root_1",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
            "v project_root_2",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
        ]
    );
}

#[gpui::test]
async fn test_expand_all_for_entry_single_worktree(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    let worktree_content = json!({
        "dir_1": {
            "file_1.py": "# File contents",
        },
        "dir_2": {
            "file_1.py": "# File contents",
        }
    });

    fs.insert_tree("/project_root_1", worktree_content.clone())
        .await;
    fs.insert_tree("/project_root_2", worktree_content).await;

    let project = Project::test(
        fs.clone(),
        ["/project_root_1".as_ref(), "/project_root_2".as_ref()],
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

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["> project_root_1", "> project_root_2",]
    );

    let root2_entry = find_project_entry(&panel, "project_root_2", cx).unwrap();
    panel.update_in(cx, |panel, window, cx| {
        let worktree_id = panel
            .project
            .read(cx)
            .worktree_id_for_entry(root2_entry, cx)
            .unwrap();
        panel.expand_all_for_entry(worktree_id, root2_entry, cx);
        panel.update_visible_entries(None, false, false, window, cx);
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "> project_root_1",
            "v project_root_2",
            "    v dir_1",
            "          file_1.py",
            "    v dir_2",
            "          file_1.py",
        ]
    );
}

#[gpui::test]
async fn test_expand_all_entries_with_auto_fold(cx: &mut gpui::TestAppContext) {
    init_test_with_editor(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "dir1": {
                "empty1": {
                    "empty2": {
                        "empty3": {
                            "file.txt": ""
                        }
                    }
                },
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

    cx.update(|_, cx| {
        let settings = *ProjectPanelSettings::get_global(cx);
        ProjectPanelSettings::override_global(
            ProjectPanelSettings {
                auto_fold_dirs: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.collapse_all_entries(&CollapseAllEntries, window, cx)
    });
    cx.executor().run_until_parked();

    panel.update_in(cx, |panel, window, cx| {
        panel.expand_all_entries(&ExpandAllEntries, window, cx)
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..20, cx),
        &[
            "v root",
            "    v dir1",
            "        v empty1",
            "            v empty2",
            "                v empty3",
            "                      file.txt",
        ],
        "expand all should unfold auto-folded directories"
    );
}
