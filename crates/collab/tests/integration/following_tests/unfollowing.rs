use super::*;

#[gpui::test(iterations = 10)]
async fn test_auto_unfollowing(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    // 2 clients connect to a server.
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
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    let _editor_a1 = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("1.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Client B starts following client A.
    let pane_b = workspace_b.update(cx_b, |workspace, _| workspace.active_pane().clone());
    let leader_id = project_b.update(cx_b, |project, _| {
        project.collaborators().values().next().unwrap().peer_id
    });
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(leader_id, window, cx)
    });
    executor.run_until_parked();
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        Some(leader_id.into())
    );
    let editor_b2 = workspace_b.update(cx_b, |workspace, cx| {
        workspace
            .active_item(cx)
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
    });

    // When client B moves, it automatically stops following client A.
    editor_b2.update_in(cx_b, |editor, window, cx| {
        editor.move_right(&editor::actions::MoveRight, window, cx)
    });
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        None
    );

    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(leader_id, window, cx)
    });
    executor.run_until_parked();
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        Some(leader_id.into())
    );

    // When client B edits, it automatically stops following client A.
    editor_b2.update_in(cx_b, |editor, window, cx| editor.insert("X", window, cx));
    assert_eq!(
        workspace_b.update_in(cx_b, |workspace, _, _| workspace.leader_for_pane(&pane_b)),
        None
    );

    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(leader_id, window, cx)
    });
    executor.run_until_parked();
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        Some(leader_id.into())
    );

    // When client B scrolls, it automatically stops following client A.
    editor_b2.update_in(cx_b, |editor, window, cx| {
        editor.set_scroll_position(point(0., 3.), window, cx)
    });
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        None
    );

    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(leader_id, window, cx)
    });
    executor.run_until_parked();
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        Some(leader_id.into())
    );

    // When client B activates a different pane, it continues following client A in the original pane.
    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.split_and_clone(pane_b.clone(), SplitDirection::Right, window, cx)
        })
        .await;
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        Some(leader_id.into())
    );

    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.activate_next_pane(window, cx)
    });
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        Some(leader_id.into())
    );

    // When client B activates a different item in the original pane, it automatically stops following client A.
    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("2.txt")), None, true, window, cx)
        })
        .await
        .unwrap();
    assert_eq!(
        workspace_b.update(cx_b, |workspace, _| workspace.leader_for_pane(&pane_b)),
        None
    );
}

#[gpui::test(iterations = 10)]
async fn test_peers_simultaneously_following_each_other(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);

    cx_a.update(editor::init);
    cx_b.update(editor::init);

    client_a.fs().insert_tree("/a", json!({})).await;
    let (project_a, _) = client_a.build_local_project("/a", cx_a).await;
    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    executor.run_until_parked();
    let client_a_id = project_b.update(cx_b, |project, _| {
        project.collaborators().values().next().unwrap().peer_id
    });
    let client_b_id = project_a.update(cx_a, |project, _| {
        project.collaborators().values().next().unwrap().peer_id
    });

    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.follow(client_b_id, window, cx)
    });
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(client_a_id, window, cx)
    });
    executor.run_until_parked();

    workspace_a.update(cx_a, |workspace, _| {
        assert_eq!(
            workspace.leader_for_pane(workspace.active_pane()),
            Some(client_b_id.into())
        );
    });
    workspace_b.update(cx_b, |workspace, _| {
        assert_eq!(
            workspace.leader_for_pane(workspace.active_pane()),
            Some(client_a_id.into())
        );
    });
}

#[gpui::test(iterations = 10)]
async fn test_following_across_workspaces(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    // a and b join a channel/call
    // a shares project 1
    // b shares project 2
    //
    // b follows a: causes project 2 to be joined, and b to follow a.
    // b opens a different file in project 2, a follows b
    // b opens a different file in project 1, a cannot follow b
    // b shares the project, a joins the project and follows b
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                "w.rs": "",
                "x.rs": "",
            }),
        )
        .await;

    client_b
        .fs()
        .insert_tree(
            path!("/b"),
            json!({
                "y.rs": "",
                "z.rs": "",
            }),
        )
        .await;

    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    let (project_a, worktree_id_a) = client_a.build_local_project(path!("/a"), cx_a).await;
    let (project_b, worktree_id_b) = client_b.build_local_project(path!("/b"), cx_b).await;

    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();

    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();
    active_call_b
        .update(cx_b, |call, cx| call.set_location(Some(&project_b), cx))
        .await
        .unwrap();

    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id_a, rel_path("w.rs")), None, true, window, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();
    assert_eq!(visible_push_notifications(cx_b).len(), 1);

    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(client_a.peer_id().unwrap(), window, cx)
    });

    executor.run_until_parked();
    let window_b_project_a = *cx_b
        .windows()
        .iter()
        .max_by_key(|window| window.window_id())
        .unwrap();

    let mut cx_b2 = VisualTestContext::from_window(window_b_project_a, cx_b);

    let workspace_b_project_a = window_b_project_a
        .downcast::<MultiWorkspace>()
        .unwrap()
        .read_with(cx_b, |mw, _| mw.workspace().clone())
        .unwrap();

    // assert that b is following a in project a in w.rs
    workspace_b_project_a.update(&mut cx_b2, |workspace, cx| {
        assert!(workspace.is_being_followed(client_a.peer_id().unwrap()));
        assert_eq!(
            client_a.peer_id().map(Into::into),
            workspace.leader_for_pane(workspace.active_pane())
        );
        let item = workspace.active_item(cx).unwrap();
        assert_eq!(item.tab_content_text(0, cx), SharedString::from("w.rs"));
    });

    // TODO: in app code, this would be done by the collab_ui.
    active_call_b
        .update(&mut cx_b2, |call, cx| {
            let project = workspace_b_project_a.read(cx).project().clone();
            call.set_location(Some(&project), cx)
        })
        .await
        .unwrap();

    // assert that there are no share notifications open
    assert_eq!(visible_push_notifications(cx_b).len(), 0);

    // b moves to x.rs in a's project, and a follows
    workspace_b_project_a
        .update_in(&mut cx_b2, |workspace, window, cx| {
            workspace.open_path((worktree_id_a, rel_path("x.rs")), None, true, window, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();
    workspace_b_project_a.update(&mut cx_b2, |workspace, cx| {
        let item = workspace.active_item(cx).unwrap();
        assert_eq!(item.tab_content_text(0, cx), SharedString::from("x.rs"));
    });

    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.follow(client_b.peer_id().unwrap(), window, cx)
    });

    executor.run_until_parked();
    workspace_a.update(cx_a, |workspace, cx| {
        assert!(workspace.is_being_followed(client_b.peer_id().unwrap()));
        assert_eq!(
            client_b.peer_id().map(Into::into),
            workspace.leader_for_pane(workspace.active_pane())
        );
        let item = workspace.active_pane().read(cx).active_item().unwrap();
        assert_eq!(item.tab_content_text(0, cx), "x.rs");
    });

    // b moves to y.rs in b's project, a is still following but can't yet see
    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id_b, rel_path("y.rs")), None, true, window, cx)
        })
        .await
        .unwrap();

    // TODO: in app code, this would be done by the collab_ui.
    active_call_b
        .update(cx_b, |call, cx| {
            let project = workspace_b.read(cx).project().clone();
            call.set_location(Some(&project), cx)
        })
        .await
        .unwrap();

    let project_b_id = active_call_b
        .update(cx_b, |call, cx| call.share_project(project_b.clone(), cx))
        .await
        .unwrap();

    executor.run_until_parked();
    assert_eq!(visible_push_notifications(cx_a).len(), 1);
    cx_a.update(|_, cx| {
        workspace::join_in_room_project(
            project_b_id,
            client_b.user_id().unwrap(),
            client_a.app_state.clone(),
            cx,
        )
    })
    .await
    .unwrap();

    executor.run_until_parked();

    assert_eq!(visible_push_notifications(cx_a).len(), 0);
    let window_a_project_b = *cx_a
        .windows()
        .iter()
        .max_by_key(|window| window.window_id())
        .unwrap();
    let cx_a2 = &mut VisualTestContext::from_window(window_a_project_b, cx_a);
    let workspace_a_project_b = window_a_project_b
        .downcast::<MultiWorkspace>()
        .unwrap()
        .read_with(cx_a, |mw, _| mw.workspace().clone())
        .unwrap();

    executor.run_until_parked();

    workspace_a_project_b.update(cx_a2, |workspace, cx| {
        assert_eq!(workspace.project().read(cx).remote_id(), Some(project_b_id));
        assert!(workspace.is_being_followed(client_b.peer_id().unwrap()));
        assert_eq!(
            client_b.peer_id().map(Into::into),
            workspace.leader_for_pane(workspace.active_pane())
        );
        let item = workspace.active_item(cx).unwrap();
        assert_eq!(item.tab_content_text(0, cx), SharedString::from("y.rs"));
    });
}

#[gpui::test]
async fn test_following_stops_on_unshare(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let (_server, client_a, client_b, channel_id) = TestServer::start2(cx_a, cx_b).await;

    let (workspace_a, cx_a) = client_a.build_test_workspace(cx_a).await;
    client_a
        .host_workspace(&workspace_a, channel_id, cx_a)
        .await;
    let (workspace_b, cx_b) = client_b.join_workspace(channel_id, cx_b).await;

    cx_a.simulate_keystrokes("cmd-p");
    cx_a.run_until_parked();
    cx_a.simulate_keystrokes("2 enter");

    let editor_a = workspace_a.update(cx_a, |workspace, cx| {
        workspace.active_item_as::<Editor>(cx).unwrap()
    });
    let editor_b = workspace_b.update(cx_b, |workspace, cx| {
        workspace.active_item_as::<Editor>(cx).unwrap()
    });

    // b should follow a to position 1
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(1)..MultiBufferOffset(1)])
        })
    });
    cx_a.executor()
        .advance_clock(workspace::item::LEADER_UPDATE_THROTTLE);
    cx_a.run_until_parked();
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(1)..MultiBufferOffset(1)]
        )
    });

    // a unshares the project
    cx_a.update(|_, cx| {
        let project = workspace_a.read(cx).project().clone();
        ActiveCall::global(cx).update(cx, |call, cx| {
            call.unshare_project(project, cx).unwrap();
        })
    });
    cx_a.run_until_parked();

    // b should not follow a to position 2
    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(2)..MultiBufferOffset(2)])
        })
    });
    cx_a.executor()
        .advance_clock(workspace::item::LEADER_UPDATE_THROTTLE);
    cx_a.run_until_parked();
    editor_b.update(cx_b, |editor, cx| {
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(1)..MultiBufferOffset(1)]
        )
    });
    cx_b.update(|_, cx| {
        let room = ActiveCall::global(cx).read(cx).room().unwrap().read(cx);
        let participant = room.remote_participants().get(&client_a.id()).unwrap();
        assert_eq!(participant.location, ParticipantLocation::UnsharedProject)
    })
}
