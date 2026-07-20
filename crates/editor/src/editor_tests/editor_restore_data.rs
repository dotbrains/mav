use super::*;

#[gpui::test]
async fn test_editor_restore_data_different_in_panes(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let main_text = r#"fn main() {
println!("1");
println!("2");
println!("3");
println!("4");
println!("5");
}"#;
    let lib_text = "mod foo {}";
    fs.insert_tree(
        path!("/a"),
        json!({
            "lib.rs": lib_text,
            "main.rs": main_text,
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let expected_ranges = vec![
        Point::new(0, 0)..Point::new(0, 0),
        Point::new(1, 0)..Point::new(1, 1),
        Point::new(2, 0)..Point::new(2, 2),
        Point::new(3, 0)..Point::new(3, 3),
    ];

    let pane_1 = workspace.update(cx, |workspace, _| workspace.active_pane().clone());
    let editor_1 = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane_1.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_1.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "Original main.rs text on initial open",
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter()
                    .map(|s| s.range())
                    .collect::<Vec<_>>(),
                vec![Point::zero()..Point::zero()],
                "Default selections on initial open",
            );
        })
    });
    editor_1.update_in(cx, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(expected_ranges.clone());
        });
    });

    let pane_2 = workspace.update_in(cx, |workspace, window, cx| {
        workspace.split_pane(pane_1.clone(), SplitDirection::Right, window, cx)
    });
    let editor_2 = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane_2.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_2.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "Original main.rs text on initial open in another panel",
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter()
                    .map(|s| s.range())
                    .collect::<Vec<_>>(),
                vec![Point::zero()..Point::zero()],
                "Default selections on initial open in another panel",
            );
        })
    });

    editor_2.update_in(cx, |editor, window, cx| {
        editor.fold_ranges(expected_ranges.clone(), false, window, cx);
    });

    let _other_editor_1 = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("lib.rs")),
                Some(pane_1.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_1
        .update_in(cx, |pane, window, cx| {
            pane.close_other_items(&CloseOtherItems::default(), None, window, cx)
        })
        .await
        .unwrap();
    drop(editor_1);
    pane_1.update(cx, |pane, cx| {
        pane.active_item()
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
            .update(cx, |editor, cx| {
                assert_eq!(
                    editor.display_text(cx),
                    lib_text,
                    "Other file should be open and active",
                );
            });
        assert_eq!(pane.items().count(), 1, "No other editors should be open");
    });

    let _other_editor_2 = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("lib.rs")),
                Some(pane_2.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_2
        .update_in(cx, |pane, window, cx| {
            pane.close_other_items(&CloseOtherItems::default(), None, window, cx)
        })
        .await
        .unwrap();
    drop(editor_2);
    pane_2.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                lib_text,
                "Other file should be open and active in another panel too",
            );
        });
        assert_eq!(
            pane.items().count(),
            1,
            "No other editors should be open in another pane",
        );
    });

    let _editor_1_reopened = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane_1.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    let _editor_2_reopened = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane_2.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane_1.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "Previous editor in the 1st panel had no extra text manipulations and should get none on reopen",
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter()
                    .map(|s| s.range())
                    .collect::<Vec<_>>(),
                expected_ranges,
                "Previous editor in the 1st panel had selections and should get them restored on reopen",
            );
        })
    });
    pane_2.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                r#"fn main() {
⋯rintln!("1");
⋯intln!("2");
⋯ntln!("3");
println!("4");
println!("5");
}"#,
                "Previous editor in the 2nd pane had folds and should restore those on reopen in the same pane",
            );
            assert_eq!(
                editor
                    .selections
                    .all::<Point>(&editor.display_snapshot(cx))
                    .into_iter()
                    .map(|s| s.range())
                    .collect::<Vec<_>>(),
                vec![Point::zero()..Point::zero()],
                "Previous editor in the 2nd pane had no selections changed hence should restore none",
            );
        })
    });
}

#[gpui::test]
async fn test_editor_does_not_restore_data_when_turned_off(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    let main_text = r#"fn main() {
println!("1");
println!("2");
println!("3");
println!("4");
println!("5");
}"#;
    let lib_text = "mod foo {}";
    fs.insert_tree(
        path!("/a"),
        json!({
            "lib.rs": lib_text,
            "main.rs": main_text,
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let pane = workspace.update(cx, |workspace, _| workspace.active_pane().clone());
    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "Original main.rs text on initial open",
            );
        })
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.fold_ranges(vec![Point::new(0, 0)..Point::new(0, 0)], false, window, cx);
    });

    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.workspace.restore_on_file_reopen = Some(false);
        });
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.fold_ranges(
            vec![
                Point::new(1, 0)..Point::new(1, 1),
                Point::new(2, 0)..Point::new(2, 2),
                Point::new(3, 0)..Point::new(3, 3),
            ],
            false,
            window,
            cx,
        );
    });
    pane.update_in(cx, |pane, window, cx| {
        pane.close_all_items(&CloseAllItems::default(), window, cx)
    })
    .await
    .unwrap();
    pane.update(cx, |pane, _| {
        assert!(pane.active_item().is_none());
    });
    cx.update_global(|store: &mut SettingsStore, cx| {
        store.update_user_settings(cx, |s| {
            s.workspace.restore_on_file_reopen = Some(true);
        });
    });

    let _editor_reopened = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                main_text,
                "No folds: even after enabling the restoration, previous editor's data should not be saved to be used for the restoration"
            );
        })
    });
}
