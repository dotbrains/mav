use super::*;

#[gpui::test]
async fn test_open_workspace_with_directory(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "dir1": {
                    "file1.txt": "content1",
                    "file2.txt": "content2",
                },
            }),
        )
        .await;

    assert_eq!(cx.windows().len(), 0);

    // First open the workspace directory
    open_workspace_file(path!("/root/dir1"), <_>::default(), app_state.clone(), cx).await;

    assert_eq!(cx.windows().len(), 1);
    let multi_workspace = cx.windows()[0].downcast::<MultiWorkspace>().unwrap();
    multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                assert!(workspace.active_item_as::<Editor>(cx).is_none())
            });
        })
        .unwrap();

    // Now open a file inside that workspace
    open_workspace_file(
        path!("/root/dir1/file1.txt"),
        <_>::default(),
        app_state.clone(),
        cx,
    )
    .await;

    assert_eq!(cx.windows().len(), 1);
    multi_workspace
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                assert!(workspace.active_item_as::<Editor>(cx).is_some());
            });
        })
        .unwrap();

    // Opening a file inside the existing worktree with -n creates a new window.
    open_workspace_file(
        path!("/root/dir1/file1.txt"),
        workspace::OpenOptions {
            workspace_matching: workspace::WorkspaceMatching::None,
            ..Default::default()
        },
        app_state.clone(),
        cx,
    )
    .await;

    assert_eq!(cx.windows().len(), 2);
}

#[gpui::test]
async fn test_wait_with_directory_waits_for_window_close(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/root"),
            json!({
                "dir1": {
                    "file1.txt": "content1",
                },
            }),
        )
        .await;

    let response_sink = DiscardResponseSink;
    let workspace_paths = vec![path!("/root/dir1").to_owned()];

    let (done_tx, mut done_rx) = futures::channel::oneshot::channel();
    cx.spawn({
        let app_state = app_state.clone();
        move |mut cx| async move {
            let errored = open_local_workspace(
                workspace_paths,
                vec![],
                false,
                workspace::OpenOptions {
                    wait: true,
                    ..Default::default()
                },
                None,
                &response_sink,
                &app_state,
                &mut cx,
            )
            .await;
            let _ = done_tx.send(errored);
        }
    })
    .detach();

    cx.background_executor.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    assert!(matches!(poll!(&mut done_rx), Poll::Pending));

    let window = cx.windows()[0];
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.background_executor.run_until_parked();

    let errored = done_rx.await.unwrap();
    assert!(!errored);
}

#[gpui::test]
async fn test_open_workspace_with_nonexistent_files(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(path!("/root"), json!({}))
        .await;

    assert_eq!(cx.windows().len(), 0);

    // Test case 1: Open a single file that does not exist yet
    open_workspace_file(
        path!("/root/file5.txt"),
        <_>::default(),
        app_state.clone(),
        cx,
    )
    .await;

    assert_eq!(cx.windows().len(), 1);
    let multi_workspace_1 = cx.windows()[0].downcast::<MultiWorkspace>().unwrap();
    multi_workspace_1
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                assert!(workspace.active_item_as::<Editor>(cx).is_some())
            });
        })
        .unwrap();

    // Test case 2: Open a single file that does not exist yet,
    // but tell Mav to add it to the current workspace
    open_workspace_file(
        path!("/root/file6.txt"),
        workspace::OpenOptions {
            workspace_matching: workspace::WorkspaceMatching::MatchSubdirectory,
            ..Default::default()
        },
        app_state.clone(),
        cx,
    )
    .await;

    assert_eq!(cx.windows().len(), 1);
    multi_workspace_1
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let items = workspace.items(cx).collect::<Vec<_>>();
                assert_eq!(items.len(), 2, "Workspace should have two items");
            });
        })
        .unwrap();

    // Test case 3: Open a single file that does not exist yet,
    // but tell Mav to NOT add it to the current workspace
    open_workspace_file(
        path!("/root/file7.txt"),
        workspace::OpenOptions {
            workspace_matching: workspace::WorkspaceMatching::None,
            ..Default::default()
        },
        app_state.clone(),
        cx,
    )
    .await;

    assert_eq!(cx.windows().len(), 2);
    let multi_workspace_2 = cx.windows()[1].downcast::<MultiWorkspace>().unwrap();
    multi_workspace_2
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.workspace().update(cx, |workspace, cx| {
                let items = workspace.items(cx).collect::<Vec<_>>();
                assert_eq!(items.len(), 1, "Workspace should have two items");
            });
        })
        .unwrap();
}

pub(super) async fn open_workspace_file(
    path: &str,
    open_options: workspace::OpenOptions,
    app_state: Arc<AppState>,
    cx: &TestAppContext,
) {
    let response_sink = DiscardResponseSink;

    let workspace_paths = vec![path.to_owned()];

    let errored = cx
        .spawn(|mut cx| async move {
            open_local_workspace(
                workspace_paths,
                vec![],
                false,
                open_options,
                None,
                &response_sink,
                &app_state,
                &mut cx,
            )
            .await
        })
        .await;

    assert!(!errored);
}

#[gpui::test]
async fn test_reuse_flag_functionality(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    let root_dir = if cfg!(windows) { "C:\\root" } else { "/root" };
    let file1_path = if cfg!(windows) {
        "C:\\root\\file1.txt"
    } else {
        "/root/file1.txt"
    };
    let file2_path = if cfg!(windows) {
        "C:\\root\\file2.txt"
    } else {
        "/root/file2.txt"
    };

    app_state.fs.create_dir(Path::new(root_dir)).await.unwrap();
    app_state
        .fs
        .create_file(Path::new(file1_path), Default::default())
        .await
        .unwrap();
    app_state
        .fs
        .save(
            Path::new(file1_path),
            &Rope::from("content1"),
            LineEnding::Unix,
        )
        .await
        .unwrap();
    app_state
        .fs
        .create_file(Path::new(file2_path), Default::default())
        .await
        .unwrap();
    app_state
        .fs
        .save(
            Path::new(file2_path),
            &Rope::from("content2"),
            LineEnding::Unix,
        )
        .await
        .unwrap();

    // First, open a workspace normally
    let response_sink = DiscardResponseSink;
    let workspace_paths = vec![file1_path.to_string()];

    let _errored = cx
        .spawn({
            let app_state = app_state.clone();
            |mut cx| async move {
                open_local_workspace(
                    workspace_paths,
                    vec![],
                    false,
                    workspace::OpenOptions::default(),
                    None,
                    &response_sink,
                    &app_state,
                    &mut cx,
                )
                .await
            }
        })
        .await;

    // Now test the reuse functionality - should replace the existing workspace
    let workspace_paths_reuse = vec![file1_path.to_string()];
    let paths: Vec<PathBuf> = workspace_paths_reuse.iter().map(PathBuf::from).collect();
    let window_to_replace = workspace::find_existing_workspace(
        &paths,
        &workspace::OpenOptions::default(),
        &workspace::SerializedWorkspaceLocation::Local,
        &mut cx.to_async(),
    )
    .await
    .0
    .unwrap()
    .0;

    let errored_reuse = cx
        .spawn({
            let app_state = app_state.clone();
            |mut cx| async move {
                let response_sink = DiscardResponseSink;
                open_local_workspace(
                    workspace_paths_reuse,
                    vec![],
                    false,
                    workspace::OpenOptions {
                        requesting_window: Some(window_to_replace),
                        ..Default::default()
                    },
                    None,
                    &response_sink,
                    &app_state,
                    &mut cx,
                )
                .await
            }
        })
        .await;

    assert!(!errored_reuse);
}
