use super::*;

#[gpui::test]
async fn test_open_paths_switches_to_best_workspace(cx: &mut TestAppContext) {
    let app_state = init_test(cx);

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/"),
            json!({
                "dir1": {
                    "a.txt": "content a"
                },
                "dir2": {
                    "b.txt": "content b"
                },
                "dir3": {
                    "c.txt": "content c"
                }
            }),
        )
        .await;

    // Create a window with workspace 0 containing /dir1
    let project1 = Project::test(app_state.fs.clone(), [path!("/dir1").as_ref()], cx).await;

    let window = cx.add_window({
        let project = project1.clone();
        |window, cx| MultiWorkspace::test_new(project, window, cx)
    });
    window
        .update(cx, |multi_workspace, _, cx| {
            multi_workspace.open_sidebar(cx);
        })
        .unwrap();

    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1, "Should start with 1 window");

    // Create workspace 2 with /dir2
    let project2 = Project::test(app_state.fs.clone(), [path!("/dir2").as_ref()], cx).await;
    let workspace2 = window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project2.clone(), window, cx)
        })
        .unwrap();

    // Create workspace 3 with /dir3
    let project3 = Project::test(app_state.fs.clone(), [path!("/dir3").as_ref()], cx).await;
    let workspace3 = window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.test_add_workspace(project3.clone(), window, cx)
        })
        .unwrap();

    let workspace1 = window
        .read_with(cx, |multi_workspace, _| {
            multi_workspace.workspaces().next().unwrap().clone()
        })
        .unwrap();

    window
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.activate(workspace2.clone(), None, window, cx);
            multi_workspace.activate(workspace3.clone(), None, window, cx);
            // Switch back to workspace1 for test setup
            multi_workspace.activate(workspace1.clone(), None, window, cx);
            assert_eq!(multi_workspace.workspace(), &workspace1);
        })
        .unwrap();

    cx.run_until_parked();

    // Verify setup: 3 workspaces, workspace 0 active, still 1 window
    window
        .read_with(cx, |multi_workspace, _| {
            assert_eq!(multi_workspace.workspaces().count(), 3);
            assert_eq!(multi_workspace.workspace(), &workspace1);
        })
        .unwrap();
    assert_eq!(cx.windows().len(), 1);

    // Open a file in /dir3 - should switch to workspace 3 (not just "the other one")
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/dir3/c.txt"))],
            app_state.clone(),
            OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    cx.run_until_parked();

    // Verify workspace 2 is active and file opened there
    window
        .read_with(cx, |multi_workspace, cx| {
            assert_eq!(
                multi_workspace.workspace(),
                &workspace3,
                "Should have switched to workspace 3 which contains /dir3"
            );
            let active_item = multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .active_item()
                .expect("Should have an active item");
            assert_eq!(active_item.tab_content_text(0, cx), "c.txt");
        })
        .unwrap();
    assert_eq!(cx.windows().len(), 1, "Should reuse existing window");

    // Open a file in /dir2 - should switch to workspace 2
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/dir2/b.txt"))],
            app_state.clone(),
            OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    cx.run_until_parked();

    // Verify workspace 1 is active and file opened there
    window
        .read_with(cx, |multi_workspace, cx| {
            assert_eq!(
                multi_workspace.workspace(),
                &workspace2,
                "Should have switched to workspace 2 which contains /dir2"
            );
            let active_item = multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .active_item()
                .expect("Should have an active item");
            assert_eq!(active_item.tab_content_text(0, cx), "b.txt");
        })
        .unwrap();

    // Verify c.txt is still in workspace 3 (file opened in correct workspace, not active one)
    workspace3.read_with(cx, |workspace, cx| {
        let active_item = workspace
            .active_pane()
            .read(cx)
            .active_item()
            .expect("Workspace 2 should have an active item");
        assert_eq!(
            active_item.tab_content_text(0, cx),
            "c.txt",
            "c.txt should have been opened in workspace 3, not the active workspace"
        );
    });

    assert_eq!(cx.windows().len(), 1, "Should still have only 1 window");

    // Open a file in /dir1 - should switch back to workspace 0
    cx.update(|cx| {
        open_paths(
            &[PathBuf::from(path!("/dir1/a.txt"))],
            app_state.clone(),
            OpenOptions::default(),
            cx,
        )
    })
    .await
    .unwrap();

    cx.run_until_parked();

    // Verify workspace 0 is active and file opened there
    window
        .read_with(cx, |multi_workspace, cx| {
            assert_eq!(
                multi_workspace.workspace(),
                &workspace1,
                "Should have switched back to workspace 0 which contains /dir1"
            );
            let active_item = multi_workspace
                .workspace()
                .read(cx)
                .active_pane()
                .read(cx)
                .active_item()
                .expect("Should have an active item");
            assert_eq!(active_item.tab_content_text(0, cx), "a.txt");
        })
        .unwrap();
    assert_eq!(cx.windows().len(), 1, "Should still have only 1 window");
}
