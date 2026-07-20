use super::*;

#[gpui::test]
async fn test_navigation(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "file1": "contents 1\n".repeat(20),
                    "file2": "contents 2\n".repeat(20),
                    "file3": "contents 3\n".repeat(20),
                },
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    project.update(cx, |project, _cx| project.languages().add(markdown_lang()));
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let pane = workspace.read_with(cx, |workspace, _| workspace.active_pane().clone());

    let entries = cx.read(|cx| workspace.file_project_paths(cx));
    let file1 = entries[0].clone();
    let file2 = entries[1].clone();
    let file3 = entries[2].clone();

    let editor1 = workspace
        .update_in(cx, |w, window, cx| {
            w.open_path(file1.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    workspace.update_in(cx, |_, window, cx| {
        editor1.update(cx, |editor, cx| {
            editor.change_selections(Default::default(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(10), 0)..DisplayPoint::new(DisplayRow(10), 0)
                ])
            });
        });
    });

    let editor2 = workspace
        .update_in(cx, |w, window, cx| {
            w.open_path(file2.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let editor3 = workspace
        .update_in(cx, |w, window, cx| {
            w.open_path(file3.clone(), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    workspace
        .update_in(cx, |_, window, cx| {
            editor3.update(cx, |editor, cx| {
                editor.change_selections(Default::default(), window, cx, |s| {
                    s.select_display_ranges([
                        DisplayPoint::new(DisplayRow(12), 0)..DisplayPoint::new(DisplayRow(12), 0)
                    ])
                });
                editor.newline(&Default::default(), window, cx);
                editor.newline(&Default::default(), window, cx);
                editor.move_down(&Default::default(), window, cx);
                editor.move_down(&Default::default(), window, cx);
                editor.save(
                    SaveOptions {
                        format: true,
                        force_format: false,
                        autosave: false,
                    },
                    project.clone(),
                    window,
                    cx,
                )
            })
        })
        .await
        .unwrap();
    workspace.update_in(cx, |_, window, cx| {
        editor3.update(cx, |editor, cx| {
            editor.set_scroll_position(point(0., 12.5), window, cx)
        });
    });
    assert_eq!(
        active_location(&workspace, cx),
        (file3.clone(), DisplayPoint::new(DisplayRow(16), 0), 12.5)
    );

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file3.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file2.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file1.clone(), DisplayPoint::new(DisplayRow(10), 0), 0.)
    );

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file1.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );

    // Go back one more time and ensure we don't navigate past the first item in the history.
    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file1.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_forward(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file1.clone(), DisplayPoint::new(DisplayRow(10), 0), 0.)
    );

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_forward(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file2.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );

    // Go forward to an item that has been closed, ensuring it gets re-opened at the same
    // location.
    workspace
        .update_in(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                let editor3_id = editor3.entity_id();
                drop(editor3);
                pane.close_item_by_id(editor3_id, SaveIntent::Close, window, cx)
            })
        })
        .await
        .unwrap();
    workspace
        .update_in(cx, |w, window, cx| {
            w.go_forward(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file3.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_forward(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file3.clone(), DisplayPoint::new(DisplayRow(16), 0), 12.5)
    );

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file3.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );

    // Go back to an item that has been closed and removed from disk
    workspace
        .update_in(cx, |_, window, cx| {
            pane.update(cx, |pane, cx| {
                let editor2_id = editor2.entity_id();
                drop(editor2);
                pane.close_item_by_id(editor2_id, SaveIntent::Close, window, cx)
            })
        })
        .await
        .unwrap();
    app_state
        .fs
        .remove_file(Path::new(path!("/root/a/file2")), Default::default())
        .await
        .unwrap();
    cx.background_executor.run_until_parked();

    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file2.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );
    workspace
        .update_in(cx, |w, window, cx| {
            w.go_forward(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file3.clone(), DisplayPoint::new(DisplayRow(0), 0), 0.)
    );

    // Modify file to collapse multiple nav history entries into the same location.
    // Ensure we don't visit the same location twice when navigating.
    workspace.update_in(cx, |_, window, cx| {
        editor1.update(cx, |editor, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(15), 0)..DisplayPoint::new(DisplayRow(15), 0)
                ])
            })
        });
    });
    for _ in 0..5 {
        workspace.update_in(cx, |_, window, cx| {
            editor1.update(cx, |editor, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_display_ranges([
                        DisplayPoint::new(DisplayRow(3), 0)..DisplayPoint::new(DisplayRow(3), 0)
                    ])
                });
            });
        });

        workspace.update_in(cx, |_, window, cx| {
            editor1.update(cx, |editor, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_display_ranges([
                        DisplayPoint::new(DisplayRow(13), 0)..DisplayPoint::new(DisplayRow(13), 0)
                    ])
                });
            });
        });
    }
    workspace.update_in(cx, |_, window, cx| {
        editor1.update(cx, |editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    s.select_display_ranges([
                        DisplayPoint::new(DisplayRow(2), 0)..DisplayPoint::new(DisplayRow(14), 0)
                    ])
                });
                editor.insert("", window, cx);
            })
        });
    });

    workspace.update_in(cx, |_, window, cx| {
        editor1.update(cx, |editor, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_display_ranges([
                    DisplayPoint::new(DisplayRow(1), 0)..DisplayPoint::new(DisplayRow(1), 0)
                ])
            })
        });
    });
    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file1.clone(), DisplayPoint::new(DisplayRow(2), 0), 0.)
    );
    workspace
        .update_in(cx, |w, window, cx| {
            w.go_back(w.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        active_location(&workspace, cx),
        (file1.clone(), DisplayPoint::new(DisplayRow(3), 0), 0.)
    );

    fn active_location(
        workspace: &Entity<Workspace>,
        cx: &mut VisualTestContext,
    ) -> (ProjectPath, DisplayPoint, f64) {
        workspace.update(cx, |workspace, cx| {
            let item = workspace.active_item(cx).unwrap();
            let editor = item.downcast::<Editor>().unwrap();

            editor.update(cx, |editor_ref, cx| {
                let selections = editor_ref
                    .selections
                    .display_ranges(&editor_ref.display_snapshot(cx));
                let scroll_position = editor_ref.scroll_position(cx);

                (
                    editor_ref.active_project_path(cx).unwrap(),
                    selections[0].start,
                    scroll_position.y,
                )
            })
        })
    }
}
