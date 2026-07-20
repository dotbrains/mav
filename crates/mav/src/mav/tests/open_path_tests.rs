use super::*;

#[gpui::test]
async fn test_open_non_existing_file(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                },
            }),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/a/new"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);

    let multi_workspace = cx.windows()[0].downcast::<MultiWorkspace>().unwrap();
    multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                assert!(workspace.active_item_as::<Editor>(cx).is_some())
            });
        })
        .unwrap();
}

#[gpui::test]
async fn test_open_paths_action(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "a": {
                    "aa": null,
                    "ab": null,
                },
                "b": {
                    "ba": null,
                    "bb": null,
                },
                "c": {
                    "ca": null,
                    "cb": null,
                },
                "d": {
                    "da": null,
                    "db": null,
                },
                "e": {
                    "ea": null,
                    "eb": null,
                }
            }),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[
                PathBuf::from(path!("/root/a")),
                PathBuf::from(path!("/root/b")),
            ],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/a"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);
    let multi_workspace_1 = cx
        .read(|cx| cx.windows()[0].downcast::<MultiWorkspace>())
        .unwrap();
    cx.run_until_parked();
    multi_workspace_1
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                assert_eq!(workspace.worktrees(cx).count(), 2);
                assert!(workspace.right_dock().read(cx).is_open());
                assert!(
                    workspace
                        .active_pane()
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                );
            });
        })
        .unwrap();

    cx.update(|cx| {
        open_paths(
            &[
                PathBuf::from(path!("/root/c")),
                PathBuf::from(path!("/root/d")),
            ],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.read(|cx| cx.windows().len()), 1);
    cx.run_until_parked();
    multi_workspace_1
        .update(cx, |multi_workspace, _window, cx| {
            assert_eq!(multi_workspace.workspaces().count(), 2);
            assert!(multi_workspace.sidebar_open());
            let workspace = multi_workspace.workspace().read(cx);
            assert_eq!(
                workspace
                    .worktrees(cx)
                    .map(|w| w.read(cx).abs_path())
                    .collect::<Vec<_>>(),
                &[
                    Path::new(path!("/root/c")).into(),
                    Path::new(path!("/root/d")).into(),
                ]
            );
        })
        .unwrap();

    // Opening with -n (reuse_worktrees: false) still creates a new window.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/e"))],
            app_state,
            workspace::OpenOptions {
                workspace_matching: workspace::WorkspaceMatching::None,
                ..Default::default()
            },
            cx,
        )
    })
    .await
    .unwrap();
    cx.background_executor.run_until_parked();
    assert_eq!(cx.read(|cx| cx.windows().len()), 2);
}

#[gpui::test]
async fn test_open_add_new(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({"a": "hey", "b": "", "dir": {"c": "f"}}),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/dir"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/a"))],
            app_state.clone(),
            workspace::OpenOptions {
                workspace_matching: workspace::WorkspaceMatching::MatchSubdirectory,
                ..Default::default()
            },
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    // Opening a file inside the existing worktree with -n creates a new window.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/dir/c"))],
            app_state.clone(),
            workspace::OpenOptions {
                workspace_matching: workspace::WorkspaceMatching::None,
                ..Default::default()
            },
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);

    // Opening a path NOT in any existing worktree with -n creates a new window.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/b"))],
            app_state.clone(),
            workspace::OpenOptions {
                workspace_matching: workspace::WorkspaceMatching::None,
                ..Default::default()
            },
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 3);
}

#[gpui::test]
async fn test_open_file_in_many_spaces(cx: &mut TestAppContext) {
    let app_state = init_test(cx);
    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({"dir1": {"a": "b"}, "dir2": {"c": "d"}}),
        )
        .await;

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/dir1/a"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/dir2/c"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    // Opening a directory with default options adds to the existing window
    // rather than creating a new one.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/dir2"))],
            app_state.clone(),
            workspace::OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);

    // Opening a directory already in a worktree with -n creates a new window.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root/dir2"))],
            app_state.clone(),
            workspace::OpenOptions {
                workspace_matching: workspace::WorkspaceMatching::None,
                ..Default::default()
            },
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);

    // Opening a directory NOT in any worktree with -n creates a new window.
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/root"))],
            app_state.clone(),
            workspace::OpenOptions {
                workspace_matching: workspace::WorkspaceMatching::None,
                ..Default::default()
            },
            cx,
        )
    })
    .await
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 3);
}
