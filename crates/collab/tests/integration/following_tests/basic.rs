use super::*;

#[gpui::test(iterations = 10)]
async fn test_basic_following(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
    cx_d: &mut TestAppContext,
) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    let client_d = server.create_client(cx_d, "user_d").await;
    server
        .create_room(&mut [
            (&client_a, cx_a),
            (&client_b, cx_b),
            (&client_c, cx_c),
            (&client_d, cx_d),
        ])
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
                "1.txt": "one\none\none",
                "2.txt": "two\ntwo\ntwo",
                "3.txt": "three\nthree\nthree",
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

    cx_b.update(|window, _| {
        assert!(window.is_window_active());
    });

    // Client A opens some editors.
    let pane_a = workspace_a.update(cx_a, |workspace, _| workspace.active_pane().clone());
    let editor_a1 = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("1.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let editor_a2 = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("2.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // Client B opens an editor.
    let editor_b1 = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("1.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let peer_id_a = client_a.peer_id().unwrap();
    let peer_id_b = client_b.peer_id().unwrap();
    let peer_id_c = client_c.peer_id().unwrap();
    let peer_id_d = client_d.peer_id().unwrap();

    // Client A updates their selections in those editors
    editor_a1.update_in(cx_a, |editor, window, cx| {
        editor.handle_input("a", window, cx);
        editor.handle_input("b", window, cx);
        editor.handle_input("c", window, cx);
        editor.select_left(&Default::default(), window, cx);
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(3)..MultiBufferOffset(2)]
        );
    });
    editor_a2.update_in(cx_a, |editor, window, cx| {
        editor.handle_input("d", window, cx);
        editor.handle_input("e", window, cx);
        editor.select_left(&Default::default(), window, cx);
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(2)..MultiBufferOffset(1)]
        );
    });

    // When client B starts following client A, only the active view state is replicated to client B.
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(peer_id_a, window, cx)
    });

    cx_c.executor().run_until_parked();
    let editor_b2 = workspace_b.update(cx_b, |workspace, cx| {
        workspace
            .active_item(cx)
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
    });
    assert_eq!(
        cx_b.read(|cx| editor_b2.read(cx).active_project_path(cx)),
        Some((worktree_id, rel_path("2.txt")).into())
    );
    assert_eq!(
        editor_b2.update(cx_b, |editor, cx| editor
            .selections
            .ranges(&editor.display_snapshot(cx))),
        vec![MultiBufferOffset(2)..MultiBufferOffset(1)]
    );
    assert_eq!(
        editor_b1.update(cx_b, |editor, cx| editor
            .selections
            .ranges(&editor.display_snapshot(cx))),
        vec![MultiBufferOffset(3)..MultiBufferOffset(3)]
    );

    executor.run_until_parked();
    let active_call_c = cx_c.read(ActiveCall::global);
    let project_c = client_c.join_remote_project(project_id, cx_c).await;
    let (workspace_c, cx_c) = client_c.build_workspace(&project_c, cx_c);
    active_call_c
        .update(cx_c, |call, cx| call.set_location(Some(&project_c), cx))
        .await
        .unwrap();
    drop(project_c);

    // Client C also follows client A.
    workspace_c.update_in(cx_c, |workspace, window, cx| {
        workspace.follow(peer_id_a, window, cx)
    });

    cx_d.executor().run_until_parked();
    let active_call_d = cx_d.read(ActiveCall::global);
    let project_d = client_d.join_remote_project(project_id, cx_d).await;
    let (workspace_d, cx_d) = client_d.build_workspace(&project_d, cx_d);
    active_call_d
        .update(cx_d, |call, cx| call.set_location(Some(&project_d), cx))
        .await
        .unwrap();
    drop(project_d);

    // All clients see that clients B and C are following client A.
    cx_c.executor().run_until_parked();
    for (name, cx) in [("A", &cx_a), ("B", &cx_b), ("C", &cx_c), ("D", &cx_d)] {
        assert_eq!(
            followers_by_leader(project_id, cx),
            &[(peer_id_a, vec![peer_id_b, peer_id_c])],
            "followers seen by {name}"
        );
    }

    // Client C unfollows client A.
    workspace_c.update_in(cx_c, |workspace, window, cx| {
        workspace.unfollow(peer_id_a, window, cx).unwrap();
    });

    // All clients see that clients B is following client A.
    cx_c.executor().run_until_parked();
    for (name, cx) in [("A", &cx_a), ("B", &cx_b), ("C", &cx_c), ("D", &cx_d)] {
        assert_eq!(
            followers_by_leader(project_id, cx),
            &[(peer_id_a, vec![peer_id_b])],
            "followers seen by {name}"
        );
    }

    // Client C re-follows client A.
    workspace_c.update_in(cx_c, |workspace, window, cx| {
        workspace.follow(peer_id_a, window, cx)
    });

    // All clients see that clients B and C are following client A.
    cx_c.executor().run_until_parked();
    for (name, cx) in [("A", &cx_a), ("B", &cx_b), ("C", &cx_c), ("D", &cx_d)] {
        assert_eq!(
            followers_by_leader(project_id, cx),
            &[(peer_id_a, vec![peer_id_b, peer_id_c])],
            "followers seen by {name}"
        );
    }

    // Client D follows client B, then switches to following client C.
    workspace_d.update_in(cx_d, |workspace, window, cx| {
        workspace.follow(peer_id_b, window, cx)
    });
    cx_a.executor().run_until_parked();
    workspace_d.update_in(cx_d, |workspace, window, cx| {
        workspace.follow(peer_id_c, window, cx)
    });

    // All clients see that D is following C
    cx_a.executor().run_until_parked();
    for (name, cx) in [("A", &cx_a), ("B", &cx_b), ("C", &cx_c), ("D", &cx_d)] {
        assert_eq!(
            followers_by_leader(project_id, cx),
            &[
                (peer_id_a, vec![peer_id_b, peer_id_c]),
                (peer_id_c, vec![peer_id_d])
            ],
            "followers seen by {name}"
        );
    }

    // Client C closes the project.
    let weak_workspace_c = workspace_c.downgrade();
    workspace_c.update_in(cx_c, |_, window, cx| {
        window.dispatch_action(Box::new(CloseWindow) as Box<dyn Action>, cx);
    });
    executor.run_until_parked();
    // are you sure you want to leave the call?
    cx_c.simulate_prompt_answer("Close window and hang up");
    cx_c.cx.update(|_| {
        drop(workspace_c);
    });
    executor.run_until_parked();
    cx_c.cx.update(|_| {});

    weak_workspace_c.assert_released();

    // Clients A and B see that client B is following A, and client C is not present in the followers.
    executor.run_until_parked();
    for (name, cx) in [("A", &cx_a), ("B", &cx_b), ("D", &cx_d)] {
        assert_eq!(
            followers_by_leader(project_id, cx),
            &[(peer_id_a, vec![peer_id_b]),],
            "followers seen by {name}"
        );
    }

    // When client A activates a different editor, client B does so as well.
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.activate_item(&editor_a1, true, true, window, cx)
    });
    executor.run_until_parked();
    workspace_b.update(cx_b, |workspace, cx| {
        assert_eq!(
            workspace.active_item(cx).unwrap().item_id(),
            editor_b1.item_id()
        );
    });

    // When client A opens a multibuffer, client B does so as well.
    let multibuffer_a = cx_a.new(|cx| {
        let buffer_a1 = project_a.update(cx, |project, cx| {
            project
                .get_open_buffer(&(worktree_id, rel_path("1.txt")).into(), cx)
                .unwrap()
        });
        let buffer_a2 = project_a.update(cx, |project, cx| {
            project
                .get_open_buffer(&(worktree_id, rel_path("2.txt")).into(), cx)
                .unwrap()
        });
        let mut result = MultiBuffer::new(Capability::ReadWrite);
        result.set_excerpts_for_path(
            PathKey::for_buffer(&buffer_a1, cx),
            buffer_a1,
            [Point::row_range(1..2)],
            1,
            cx,
        );
        result.set_excerpts_for_path(
            PathKey::for_buffer(&buffer_a2, cx),
            buffer_a2,
            [Point::row_range(5..6)],
            1,
            cx,
        );
        result
    });
    let multibuffer_editor_a = workspace_a.update_in(cx_a, |workspace, window, cx| {
        let editor = cx
            .new(|cx| Editor::for_multibuffer(multibuffer_a, Some(project_a.clone()), window, cx));
        workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
        editor
    });
    executor.run_until_parked();
    let multibuffer_editor_b = workspace_b.update(cx_b, |workspace, cx| {
        workspace
            .active_item(cx)
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
    });
    assert_eq!(
        multibuffer_editor_a.update(cx_a, |editor, cx| editor.text(cx)),
        multibuffer_editor_b.update(cx_b, |editor, cx| editor.text(cx)),
    );

    // When client A navigates back and forth, client B does so as well.
    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.go_back(workspace.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    workspace_b.update(cx_b, |workspace, cx| {
        assert_eq!(
            workspace.active_item(cx).unwrap().item_id(),
            editor_b1.item_id()
        );
    });

    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.go_back(workspace.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    workspace_b.update(cx_b, |workspace, cx| {
        assert_eq!(
            workspace.active_item(cx).unwrap().item_id(),
            editor_b2.item_id()
        );
    });

    workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.go_forward(workspace.active_pane().downgrade(), window, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    workspace_b.update(cx_b, |workspace, cx| {
        assert_eq!(
            workspace.active_item(cx).unwrap().item_id(),
            editor_b1.item_id()
        );
    });

    // Changes to client A's editor are reflected on client B.
    editor_a1.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([
                MultiBufferOffset(1)..MultiBufferOffset(1),
                MultiBufferOffset(2)..MultiBufferOffset(2),
            ])
        });
    });
    executor.advance_clock(workspace::item::LEADER_UPDATE_THROTTLE);
    executor.run_until_parked();
    cx_b.background_executor.run_until_parked();

    editor_b1.update(cx_b, |editor, cx| {
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            &[
                MultiBufferOffset(1)..MultiBufferOffset(1),
                MultiBufferOffset(2)..MultiBufferOffset(2)
            ]
        );
    });

    editor_a1.update_in(cx_a, |editor, window, cx| {
        editor.set_text("TWO", window, cx)
    });
    executor.run_until_parked();
    editor_b1.update(cx_b, |editor, cx| assert_eq!(editor.text(cx), "TWO"));

    editor_a1.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(3)..MultiBufferOffset(3)])
        });
        editor.set_scroll_position(point(0., 100.), window, cx);
    });
    executor.advance_clock(workspace::item::LEADER_UPDATE_THROTTLE);
    executor.run_until_parked();
    editor_b1.update(cx_b, |editor, cx| {
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            &[MultiBufferOffset(3)..MultiBufferOffset(3)]
        );
    });

    // After unfollowing, client B stops receiving updates from client A.
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.unfollow(peer_id_a, window, cx).unwrap()
    });
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.activate_item(&editor_a2, true, true, window, cx)
    });
    executor.run_until_parked();
    assert_eq!(
        workspace_b.update(cx_b, |workspace, cx| workspace
            .active_item(cx)
            .unwrap()
            .item_id()),
        editor_b1.item_id()
    );

    // Client A starts following client B.
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        workspace.follow(peer_id_b, window, cx)
    });
    executor.run_until_parked();
    assert_eq!(
        workspace_a.update(cx_a, |workspace, _| workspace.leader_for_pane(&pane_a)),
        Some(peer_id_b.into())
    );
    assert_eq!(
        workspace_a.update_in(cx_a, |workspace, _, cx| workspace
            .active_item(cx)
            .unwrap()
            .item_id()),
        editor_a1.item_id()
    );

    exercise_screen_share_following(
        &executor,
        &active_call_b,
        &client_b,
        &project_b,
        &workspace_a,
        cx_a,
        &workspace_b,
        cx_b,
        &editor_a1,
        &multibuffer_editor_a,
        &multibuffer_editor_b,
        &pane_a,
    )
    .await;
}
