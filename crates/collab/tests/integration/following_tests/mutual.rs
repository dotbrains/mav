use super::*;

#[gpui::test(iterations = 10)]
async fn test_peers_following_each_other(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let executor = cx_a.executor();
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

    // Client A shares a project.
    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "1.txt": "one",
                "2.txt": "two",
                "3.txt": "three",
                "4.txt": "four",
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

    // Client B joins the project.
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    // Client A opens a file.
    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("1.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Client B opens a different file.
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("2.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Clients A and B follow each other in split panes
    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.split_and_clone(
                workspace.active_pane().clone(),
                SplitDirection::Right,
                window,
                cx,
            )
        })
        .await;
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.follow(client_b.peer_id().unwrap(), window, cx)
    });
    executor.run_until_parked();
    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.split_and_clone(
                workspace.active_pane().clone(),
                SplitDirection::Right,
                window,
                cx,
            )
        })
        .await;
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(client_a.peer_id().unwrap(), window, cx)
    });
    executor.run_until_parked();

    // Clients A and B return focus to the original files they had open
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.activate_next_pane(window, cx)
    });
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.activate_next_pane(window, cx)
    });
    executor.run_until_parked();

    // Both clients see the other client's focused file in their right pane.
    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: true,
                leader: None,
                items: vec![(true, "1.txt".into())]
            },
            PaneSummary {
                active: false,
                leader: client_b.peer_id(),
                items: vec![(false, "1.txt".into()), (true, "2.txt".into())]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: true,
                leader: None,
                items: vec![(true, "2.txt".into())]
            },
            PaneSummary {
                active: false,
                leader: client_a.peer_id(),
                items: vec![(false, "2.txt".into()), (true, "1.txt".into())]
            },
        ]
    );

    // Clients A and B each open a new file.
    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("3.txt")), None, true, window, cx)
        })
        .await
        .unwrap();

    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("4.txt")), None, true, window, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    // Both client's see the other client open the new file, but keep their
    // focus on their own active pane.
    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: true,
                leader: None,
                items: vec![(false, "1.txt".into()), (true, "3.txt".into())]
            },
            PaneSummary {
                active: false,
                leader: client_b.peer_id(),
                items: vec![
                    (false, "1.txt".into()),
                    (false, "2.txt".into()),
                    (true, "4.txt".into())
                ]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: true,
                leader: None,
                items: vec![(false, "2.txt".into()), (true, "4.txt".into())]
            },
            PaneSummary {
                active: false,
                leader: client_a.peer_id(),
                items: vec![
                    (false, "2.txt".into()),
                    (false, "1.txt".into()),
                    (true, "3.txt".into())
                ]
            },
        ]
    );

    // Client A focuses their right pane, in which they're following client B.
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.activate_next_pane(window, cx)
    });
    executor.run_until_parked();

    // Client B sees that client A is now looking at the same file as them.
    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "1.txt".into()), (true, "3.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: client_b.peer_id(),
                items: vec![
                    (false, "1.txt".into()),
                    (false, "2.txt".into()),
                    (true, "4.txt".into())
                ]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: true,
                leader: None,
                items: vec![(false, "2.txt".into()), (true, "4.txt".into())]
            },
            PaneSummary {
                active: false,
                leader: client_a.peer_id(),
                items: vec![
                    (false, "2.txt".into()),
                    (false, "1.txt".into()),
                    (false, "3.txt".into()),
                    (true, "4.txt".into())
                ]
            },
        ]
    );

    // Client B focuses their right pane, in which they're following client A,
    // who is following them.
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.activate_next_pane(window, cx)
    });
    executor.run_until_parked();

    // Client A sees that client B is now looking at the same file as them.
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "2.txt".into()), (true, "4.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: client_a.peer_id(),
                items: vec![
                    (false, "2.txt".into()),
                    (false, "1.txt".into()),
                    (false, "3.txt".into()),
                    (true, "4.txt".into())
                ]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "1.txt".into()), (true, "3.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: client_b.peer_id(),
                items: vec![
                    (false, "1.txt".into()),
                    (false, "2.txt".into()),
                    (true, "4.txt".into())
                ]
            },
        ]
    );

    // Client B focuses a file that they previously followed A to, breaking
    // the follow.
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.activate_previous_item(&Default::default(), window, cx);
        });
    });
    executor.run_until_parked();

    // Both clients see that client B is looking at that previous file.
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "2.txt".into()), (true, "4.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: None,
                items: vec![
                    (false, "2.txt".into()),
                    (false, "1.txt".into()),
                    (true, "3.txt".into()),
                    (false, "4.txt".into())
                ]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "1.txt".into()), (true, "3.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: client_b.peer_id(),
                items: vec![
                    (false, "1.txt".into()),
                    (false, "2.txt".into()),
                    (false, "4.txt".into()),
                    (true, "3.txt".into()),
                ]
            },
        ]
    );

    // Client B closes tabs, some of which were originally opened by client A,
    // and some of which were originally opened by client B.
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.close_other_items(&Default::default(), None, window, cx)
                .detach();
        });
    });

    executor.run_until_parked();

    // Both clients see that Client B is looking at the previous tab.
    assert_eq!(
        pane_summaries(&workspace_b, cx_b),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "2.txt".into()), (true, "4.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: None,
                items: vec![(true, "3.txt".into()),]
            },
        ]
    );
    assert_eq!(
        pane_summaries(&workspace_a, cx_a),
        &[
            PaneSummary {
                active: false,
                leader: None,
                items: vec![(false, "1.txt".into()), (true, "3.txt".into())]
            },
            PaneSummary {
                active: true,
                leader: client_b.peer_id(),
                items: vec![
                    (false, "1.txt".into()),
                    (false, "2.txt".into()),
                    (false, "4.txt".into()),
                    (true, "3.txt".into()),
                ]
            },
        ]
    );

    // Client B follows client A again.
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(client_a.peer_id().unwrap(), window, cx)
    });
    executor.run_until_parked();
    // Client A cycles through some tabs.
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.active_pane().update(cx, |pane, cx| {
            pane.activate_previous_item(&Default::default(), window, cx);
        });
    });
    executor.run_until_parked();

    assert_followed_tab_rotation(executor, &client_a, &workspace_a, cx_a, &workspace_b, cx_b);
}
