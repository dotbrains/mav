use super::*;

#[gpui::test]
async fn test_following_tab_order(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    cx_a.update(editor::init);
    cx_b.update(editor::init);

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "1.txt": "one",
                "2.txt": "two",
                "3.txt": "three",
            }),
        )
        .await;
    let (project_a, worktree_id) = client_a.build_local_project(path!("/a"), cx_a).await;
    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();

    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let pane_a = workspace_a.update(cx_a, |workspace, _| workspace.active_pane().clone());

    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let pane_b = workspace_b.update(cx_b, |workspace, _| workspace.active_pane().clone());

    let client_b_id = project_a.update(cx_a, |project, _| {
        project.collaborators().values().next().unwrap().peer_id
    });

    //Open 1, 3 in that order on client A
    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("1.txt")), None, true, window, cx)
        })
        .await
        .unwrap();
    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("3.txt")), None, true, window, cx)
        })
        .await
        .unwrap();

    let pane_paths = |pane: &Entity<workspace::Pane>, cx: &mut VisualTestContext| {
        pane.update(cx, |pane, cx| {
            pane.items()
                .map(|item| item.project_path(cx).unwrap().path)
                .collect::<Vec<_>>()
        })
    };

    //Verify that the tabs opened in the order we expect
    assert_eq!(
        &pane_paths(&pane_a, cx_a),
        &[rel_path("1.txt").into(), rel_path("3.txt").into()]
    );

    //Follow client B as client A
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.follow(client_b_id, window, cx)
    });
    executor.run_until_parked();

    //Open just 2 on client B
    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("2.txt")), None, true, window, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    // Verify that newly opened followed file is at the end
    assert_eq!(
        &pane_paths(&pane_a, cx_a),
        &[
            rel_path("1.txt").into(),
            rel_path("3.txt").into(),
            rel_path("2.txt").into()
        ]
    );

    //Open just 1 on client B
    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("1.txt")), None, true, window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        &pane_paths(&pane_b, cx_b),
        &[rel_path("2.txt").into(), rel_path("1.txt").into()]
    );
    executor.run_until_parked();

    // Verify that following into 1 did not reorder
    assert_eq!(
        &pane_paths(&pane_a, cx_a),
        &[
            rel_path("1.txt").into(),
            rel_path("3.txt").into(),
            rel_path("2.txt").into()
        ]
    );
}
