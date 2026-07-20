use super::*;

#[gpui::test]
async fn test_multiple_marked_entries(cx: &mut gpui::TestAppContext) {
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
        }),
    )
    .await;

    let project = Project::test(fs.clone(), ["/project_root".as_ref()], cx).await;
    let worktree_id = cx.update(|cx| project.read(cx).worktrees(cx).next().unwrap().read(cx).id());
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.select_next(&Default::default(), window, cx);
            this.expand_selected_entry(&Default::default(), window, cx);
        })
    });
    cx.run_until_parked();

    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.expand_selected_entry(&Default::default(), window, cx);
        })
    });
    cx.run_until_parked();

    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.select_next(&Default::default(), window, cx);
            this.expand_selected_entry(&Default::default(), window, cx);
        })
    });
    cx.run_until_parked();

    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.select_next(&Default::default(), window, cx);
        })
    });
    cx.run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    v dir_1",
            "        v nested_dir",
            "              file_a.py  <== selected",
            "      file_1.py",
        ]
    );
    let modifiers_with_shift = gpui::Modifiers {
        shift: true,
        ..Default::default()
    };
    cx.run_until_parked();
    cx.simulate_modifiers_change(modifiers_with_shift);
    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.select_next(&Default::default(), window, cx);
        })
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    v dir_1",
            "        v nested_dir",
            "              file_a.py",
            "      file_1.py  <== selected  <== marked",
        ]
    );
    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.select_previous(&Default::default(), window, cx);
        })
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    v dir_1",
            "        v nested_dir",
            "              file_a.py  <== selected  <== marked",
            "      file_1.py  <== marked",
        ]
    );
    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            let drag = DraggedSelection {
                active_selection: this.selection.unwrap(),
                marked_selections: this.marked_entries.clone().into(),
                source_pane: None,
                active_selection_is_file: true,
            };
            let target_entry = this
                .project
                .read(cx)
                .entry_for_path(&(worktree_id, rel_path("")).into(), cx)
                .unwrap();
            this.drag_onto(&drag, target_entry.id, false, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    v dir_1",
            "        v nested_dir",
            "      file_1.py  <== marked",
            "      file_a.py  <== selected  <== marked",
        ]
    );
    // ESC clears out all marks
    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.cancel(&menu::Cancel, window, cx);
        })
    });
    cx.executor().run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    v dir_1",
            "        v nested_dir",
            "      file_1.py",
            "      file_a.py  <== selected",
        ]
    );
    // ESC clears out all marks
    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.select_previous(&SelectPrevious, window, cx);
            this.select_next(&SelectNext, window, cx);
        })
    });
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    v dir_1",
            "        v nested_dir",
            "      file_1.py  <== marked",
            "      file_a.py  <== selected  <== marked",
        ]
    );
    cx.simulate_modifiers_change(Default::default());
    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.cut(&Cut, window, cx);
            this.select_previous(&SelectPrevious, window, cx);
            this.select_previous(&SelectPrevious, window, cx);

            this.paste(&Paste, window, cx);
            this.update_visible_entries(None, false, false, window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    v dir_1",
            "        v nested_dir",
            "              file_1.py  <== marked",
            "              file_a.py  <== selected  <== marked",
        ]
    );
    cx.simulate_modifiers_change(modifiers_with_shift);
    cx.update(|window, cx| {
        panel.update(cx, |this, cx| {
            this.expand_selected_entry(&Default::default(), window, cx);
            this.select_next(&SelectNext, window, cx);
            this.select_next(&SelectNext, window, cx);
        })
    });
    submit_deletion(&panel, cx);
    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v project_root",
            "    v dir_1",
            "        v nested_dir  <== selected",
        ]
    );
}

#[gpui::test]
async fn test_dragged_selection_resolve_entry(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "a": {
                "b": {
                    "c": {
                        "d": {}
                    }
                }
            },
            "target_destination": {}
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
                auto_fold_dirs: true,
                ..settings
            },
            cx,
        );
    });

    let panel = workspace.update_in(cx, ProjectPanel::new);
    cx.run_until_parked();

    // Case 1: Move last dir 'd' - should move only 'd', leaving 'a/b/c'
    select_path(&panel, "root/a/b/c/d", cx);
    panel.update_in(cx, |panel, window, cx| {
        let drag = DraggedSelection {
            active_selection: *panel.selection.as_ref().unwrap(),
            marked_selections: Arc::new([*panel.selection.as_ref().unwrap()]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let target_entry = panel
            .project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .entry_for_path(rel_path("target_destination"))
            .unwrap();
        panel.drag_onto(&drag, target_entry.id, false, window, cx);
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &[
            "v root",
            "    > a/b/c",
            "    > target_destination/d  <== selected"
        ],
        "Moving last empty directory 'd' should leave 'a/b/c' and move only 'd'"
    );

    // Reset
    select_path(&panel, "root/target_destination/d", cx);
    panel.update_in(cx, |panel, window, cx| {
        let drag = DraggedSelection {
            active_selection: *panel.selection.as_ref().unwrap(),
            marked_selections: Arc::new([*panel.selection.as_ref().unwrap()]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let target_entry = panel
            .project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .entry_for_path(rel_path("a/b/c"))
            .unwrap();
        panel.drag_onto(&drag, target_entry.id, false, window, cx);
    });
    cx.executor().run_until_parked();

    // Case 2: Move middle dir 'b' - should move 'b/c/d', leaving only 'a'
    select_path(&panel, "root/a/b", cx);
    panel.update_in(cx, |panel, window, cx| {
        let drag = DraggedSelection {
            active_selection: *panel.selection.as_ref().unwrap(),
            marked_selections: Arc::new([*panel.selection.as_ref().unwrap()]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let target_entry = panel
            .project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .entry_for_path(rel_path("target_destination"))
            .unwrap();
        panel.drag_onto(&drag, target_entry.id, false, window, cx);
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root", "    v a", "    > target_destination/b/c/d"],
        "Moving middle directory 'b' should leave only 'a' and move 'b/c/d'"
    );

    // Reset
    select_path(&panel, "root/target_destination/b", cx);
    panel.update_in(cx, |panel, window, cx| {
        let drag = DraggedSelection {
            active_selection: *panel.selection.as_ref().unwrap(),
            marked_selections: Arc::new([*panel.selection.as_ref().unwrap()]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let target_entry = panel
            .project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .entry_for_path(rel_path("a"))
            .unwrap();
        panel.drag_onto(&drag, target_entry.id, false, window, cx);
    });
    cx.executor().run_until_parked();

    // Case 3: Move first dir 'a' - should move whole 'a/b/c/d'
    select_path(&panel, "root/a", cx);
    panel.update_in(cx, |panel, window, cx| {
        let drag = DraggedSelection {
            active_selection: *panel.selection.as_ref().unwrap(),
            marked_selections: Arc::new([*panel.selection.as_ref().unwrap()]),
            source_pane: None,
            active_selection_is_file: true,
        };
        let target_entry = panel
            .project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .unwrap()
            .read(cx)
            .entry_for_path(rel_path("target_destination"))
            .unwrap();
        panel.drag_onto(&drag, target_entry.id, false, window, cx);
    });
    cx.executor().run_until_parked();

    assert_eq!(
        visible_entries_as_strings(&panel, 0..10, cx),
        &["v root", "    > target_destination/a/b/c/d"],
        "Moving first directory 'a' should move whole 'a/b/c/d' chain"
    );
}
