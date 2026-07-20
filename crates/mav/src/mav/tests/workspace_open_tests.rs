use super::*;

#[gpui::test]
async fn test_new_empty_workspace(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    cx.update(|cx| {
        open_new(
            Default::default(),
            app_state.clone(),
            cx,
            |workspace, window, cx| Editor::new_file(workspace, &Default::default(), window, cx),
        )
    })
    .await
    .unwrap();
    cx.run_until_parked();

    let multi_workspace = cx
        .update(|cx| cx.windows().first().unwrap().downcast::<MultiWorkspace>())
        .unwrap();

    let editor = multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let editor = workspace
                    .active_item(cx)
                    .unwrap()
                    .downcast::<editor::Editor>()
                    .unwrap();
                editor.update(cx, |editor, cx| {
                    assert!(editor.text(cx).is_empty());
                    assert!(!editor.is_dirty(cx));
                });

                editor
            })
        })
        .unwrap();

    let save_task = multi_workspace
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.save_active_item(SaveIntent::Save, window, cx)
            })
        })
        .unwrap();
    app_state.fs.create_dir(Path::new("/root")).await.unwrap();
    cx.background_executor.run_until_parked();
    cx.simulate_new_path_selection(|_| Some(PathBuf::from("/root/the-new-name")));
    save_task.await.unwrap();
    multi_workspace
        .update(cx, |_, _, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.is_dirty(cx));
                assert_eq!(editor.title(cx), "the-new-name");
            });
        })
        .unwrap();
}

#[gpui::test]
async fn test_open_entry(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "file1": "contents 1",
                    "file2": "contents 2",
                    "file3": "contents 3",
                },
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/root").as_ref()], cx).await;
    project.update(cx, |project, _cx| project.languages().add(markdown_lang()));
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project, window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let entries = cx.read(|cx| workspace.file_project_paths(cx));
    let file1 = entries[0].clone();
    let file2 = entries[1].clone();
    let file3 = entries[2].clone();

    // Open the first entry
    let entry_1 = window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |w, cx| {
                w.open_path(file1.clone(), None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        assert_eq!(
            pane.active_item().unwrap().project_path(cx),
            Some(file1.clone())
        );
        assert_eq!(pane.items_len(), 1);
    });

    // Open the second entry
    window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |w, cx| {
                w.open_path(file2.clone(), None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        assert_eq!(
            pane.active_item().unwrap().project_path(cx),
            Some(file2.clone())
        );
        assert_eq!(pane.items_len(), 2);
    });

    // Open the first entry again. The existing pane item is activated.
    let entry_1b = window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |w, cx| {
                w.open_path(file1.clone(), None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
    assert_eq!(entry_1.item_id(), entry_1b.item_id());

    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        assert_eq!(
            pane.active_item().unwrap().project_path(cx),
            Some(file1.clone())
        );
        assert_eq!(pane.items_len(), 2);
    });

    // Split the pane with the first entry, then open the second entry again.
    window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |w, cx| {
                w.split_and_clone(w.active_pane().clone(), SplitDirection::Right, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();
    window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |w, cx| {
                w.open_path(file2.clone(), None, true, window, cx)
            })
        })
        .unwrap()
        .await
        .unwrap();

    cx.read(|cx| {
        assert_eq!(
            workspace
                .read(cx)
                .active_pane()
                .read(cx)
                .active_item()
                .unwrap()
                .project_path(cx),
            Some(file2.clone())
        );
    });

    // Open the third entry twice concurrently. Only one pane item is added.
    let (t1, t2) = window
        .update(cx, |_, window, cx| {
            workspace.update(cx, |w, cx| {
                (
                    w.open_path(file3.clone(), None, true, window, cx),
                    w.open_path(file3.clone(), None, true, window, cx),
                )
            })
        })
        .unwrap();
    t1.await.unwrap();
    t2.await.unwrap();
    cx.read(|cx| {
        let pane = workspace.read(cx).active_pane().read(cx);
        assert_eq!(
            pane.active_item().unwrap().project_path(cx),
            Some(file3.clone())
        );
        let pane_entries = pane
            .items()
            .map(|i| i.project_path(cx).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(pane_entries, &[file1, file2, file3]);
    });
}

#[gpui::test]
async fn test_open_paths(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/"),
            json!({
                "dir1": {
                    "a.txt": ""
                },
                "dir2": {
                    "b.txt": ""
                },
                "dir3": {
                    "c.txt": ""
                },
                "d.txt": ""
            }),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/dir1/"))],
            app_state,
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    let window = cx.update(|cx| cx.windows()[0].downcast::<MultiWorkspace>().unwrap());
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    #[track_caller]
    fn assert_project_panel_selection(
        workspace: &Workspace,
        expected_worktree_path: &Path,
        expected_entry_path: &RelPath,
        cx: &App,
    ) {
        let project_panel = [
            workspace.left_dock().read(cx).panel::<ProjectPanel>(),
            workspace.right_dock().read(cx).panel::<ProjectPanel>(),
        ]
        .into_iter()
        .find_map(std::convert::identity)
        .expect("found no project panels")
        .read(cx);
        let (selected_worktree, selected_entry) = project_panel
            .selected_entry(cx)
            .expect("project panel should have a selected entry");
        assert_eq!(
            selected_worktree.abs_path().as_ref(),
            expected_worktree_path,
            "Unexpected project panel selected worktree path"
        );
        assert_eq!(
            selected_entry.path.as_ref(),
            expected_entry_path,
            "Unexpected project panel selected entry path"
        );
    }

    // Open a file within an existing worktree.
    window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.open_paths(
                    vec![path!("/dir1/a.txt").into()],
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    None,
                    window,
                    cx,
                )
            })
        })
        .unwrap()
        .await;
    cx.run_until_parked();
    cx.read(|cx| {
        let workspace = workspace.read(cx);
        assert_project_panel_selection(workspace, Path::new(path!("/dir1")), rel_path("a.txt"), cx);
        assert_eq!(
            workspace
                .active_pane()
                .read(cx)
                .active_item()
                .unwrap()
                .act_as::<Editor>(cx)
                .unwrap()
                .read(cx)
                .title(cx),
            "a.txt"
        );
    });

    // Open a file outside of any existing worktree.
    window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.open_paths(
                    vec![path!("/dir2/b.txt").into()],
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    None,
                    window,
                    cx,
                )
            })
        })
        .unwrap()
        .await;
    cx.run_until_parked();
    cx.read(|cx| {
        let workspace = workspace.read(cx);
        assert_project_panel_selection(
            workspace,
            Path::new(path!("/dir2/b.txt")),
            rel_path(""),
            cx,
        );
        let worktree_roots = workspace
            .worktrees(cx)
            .map(|w| w.read(cx).as_local().unwrap().abs_path().as_ref())
            .collect::<HashSet<_>>();
        assert_eq!(
            worktree_roots,
            vec![path!("/dir1"), path!("/dir2/b.txt")]
                .into_iter()
                .map(Path::new)
                .collect(),
        );
        assert_eq!(
            workspace
                .active_pane()
                .read(cx)
                .active_item()
                .unwrap()
                .act_as::<Editor>(cx)
                .unwrap()
                .read(cx)
                .title(cx),
            "b.txt"
        );
    });

    // Ensure opening a directory and one of its children only adds one worktree.
    window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.open_paths(
                    vec![path!("/dir3").into(), path!("/dir3/c.txt").into()],
                    OpenOptions {
                        visible: Some(OpenVisible::All),
                        ..Default::default()
                    },
                    None,
                    window,
                    cx,
                )
            })
        })
        .unwrap()
        .await;
    cx.run_until_parked();
    cx.read(|cx| {
        let workspace = workspace.read(cx);
        assert_project_panel_selection(workspace, Path::new(path!("/dir3")), rel_path("c.txt"), cx);
        let worktree_roots = workspace
            .worktrees(cx)
            .map(|w| w.read(cx).as_local().unwrap().abs_path().as_ref())
            .collect::<HashSet<_>>();
        assert_eq!(
            worktree_roots,
            vec![path!("/dir1"), path!("/dir2/b.txt"), path!("/dir3")]
                .into_iter()
                .map(Path::new)
                .collect(),
        );
        assert_eq!(
            workspace
                .active_pane()
                .read(cx)
                .active_item()
                .unwrap()
                .act_as::<Editor>(cx)
                .unwrap()
                .read(cx)
                .title(cx),
            "c.txt"
        );
    });

    // Ensure opening invisibly a file outside an existing worktree adds a new, invisible worktree.
    window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                workspace.open_paths(
                    vec![path!("/d.txt").into()],
                    OpenOptions {
                        visible: Some(OpenVisible::None),
                        ..Default::default()
                    },
                    None,
                    window,
                    cx,
                )
            })
        })
        .unwrap()
        .await;
    cx.run_until_parked();
    cx.read(|cx| {
        let workspace = workspace.read(cx);
        assert_project_panel_selection(workspace, Path::new(path!("/d.txt")), rel_path(""), cx);
        let worktree_roots = workspace
            .worktrees(cx)
            .map(|w| w.read(cx).as_local().unwrap().abs_path().as_ref())
            .collect::<HashSet<_>>();
        assert_eq!(
            worktree_roots,
            vec![
                path!("/dir1"),
                path!("/dir2/b.txt"),
                path!("/dir3"),
                path!("/d.txt")
            ]
            .into_iter()
            .map(Path::new)
            .collect(),
        );

        let visible_worktree_roots = workspace
            .visible_worktrees(cx)
            .map(|w| w.read(cx).as_local().unwrap().abs_path().as_ref())
            .collect::<HashSet<_>>();
        assert_eq!(
            visible_worktree_roots,
            vec![path!("/dir1"), path!("/dir2/b.txt"), path!("/dir3")]
                .into_iter()
                .map(Path::new)
                .collect(),
        );

        assert_eq!(
            workspace
                .active_pane()
                .read(cx)
                .active_item()
                .unwrap()
                .act_as::<Editor>(cx)
                .unwrap()
                .read(cx)
                .title(cx),
            "d.txt"
        );
    });
}
