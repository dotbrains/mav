use super::*;

#[gpui::test]
async fn test_add_flag_prefers_focused_window(cx: &mut TestAppContext) {
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

    let workspace_paths_1 = vec![file1_path.to_string()];
    let _errored = cx
        .spawn({
            let app_state = app_state.clone();
            |mut cx| async move {
                let response_sink = DiscardResponseSink;
                open_local_workspace(
                    workspace_paths_1,
                    Vec::new(),
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

    assert_eq!(cx.windows().len(), 1);
    let multi_workspace_1 = cx.windows()[0].downcast::<MultiWorkspace>().unwrap();

    let workspace_paths_2 = vec![file2_path.to_string()];
    let _errored = cx
        .spawn({
            let app_state = app_state.clone();
            |mut cx| async move {
                let response_sink = DiscardResponseSink;
                open_local_workspace(
                    workspace_paths_2,
                    Vec::new(),
                    false,
                    workspace::OpenOptions {
                        workspace_matching: workspace::WorkspaceMatching::None,
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

    assert_eq!(cx.windows().len(), 2);
    let multi_workspace_2 = cx.windows()[1].downcast::<MultiWorkspace>().unwrap();

    multi_workspace_2
        .update(cx, |_, window, _| {
            window.activate_window();
        })
        .unwrap();

    let new_file_path = if cfg!(windows) {
        "C:\\root\\new_file.txt"
    } else {
        "/root/new_file.txt"
    };
    app_state
        .fs
        .create_file(Path::new(new_file_path), Default::default())
        .await
        .unwrap();

    let workspace_paths_add = vec![new_file_path.to_string()];
    let _errored = cx
        .spawn({
            let app_state = app_state.clone();
            |mut cx| async move {
                let response_sink = DiscardResponseSink;
                open_local_workspace(
                    workspace_paths_add,
                    Vec::new(),
                    false,
                    workspace::OpenOptions {
                        workspace_matching: workspace::WorkspaceMatching::MatchSubdirectory,
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

    assert_eq!(cx.windows().len(), 2);

    multi_workspace_2
        .update(cx, |workspace, _, cx| {
            let items = workspace.workspace().read(cx).items(cx).collect::<Vec<_>>();
            assert_eq!(items.len(), 2, "Focused window should have 2 items");
        })
        .unwrap();

    multi_workspace_1
        .update(cx, |workspace, _, cx| {
            let items = workspace.workspace().read(cx).items(cx).collect::<Vec<_>>();
            assert_eq!(items.len(), 1, "Other window should still have 1 item");
        })
        .unwrap();
}
