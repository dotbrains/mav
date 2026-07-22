use super::*;

#[gpui::test]
async fn test_following_after_replacement(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let (_server, client_a, client_b, channel) = TestServer::start2(cx_a, cx_b).await;

    let (workspace, cx_a) = client_a.build_test_workspace(cx_a).await;
    join_channel(channel, &client_a, cx_a).await.unwrap();
    share_workspace(&workspace, cx_a).await.unwrap();
    let buffer = workspace.update(cx_a, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.create_local_buffer(&sample_text(26, 5, 'a'), None, false, cx)
        })
    });
    let multibuffer = cx_a.new(|cx| {
        let mut mb = MultiBuffer::new(Capability::ReadWrite);
        mb.set_excerpts_for_path(
            PathKey::for_buffer(&buffer, cx),
            buffer.clone(),
            [Point::row_range(1..1), Point::row_range(5..5)],
            1,
            cx,
        );
        mb
    });
    let multibuffer_snapshot = multibuffer.update(cx_a, |mb, cx| mb.snapshot(cx));
    let snapshot = buffer.update(cx_a, |buffer, _| buffer.snapshot());
    let editor: Entity<Editor> = cx_a.new_window_entity(|window, cx| {
        Editor::for_multibuffer(
            multibuffer.clone(),
            Some(workspace.read(cx).project().clone()),
            window,
            cx,
        )
    });
    workspace.update_in(cx_a, |workspace, window, cx| {
        workspace.add_item_to_center(Box::new(editor.clone()) as _, window, cx)
    });
    editor.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([Point::row_range(4..4)]);
        })
    });
    let positions = editor.update(cx_a, |editor, _| {
        editor
            .selections
            .disjoint_anchor_ranges()
            .map(|range| {
                multibuffer_snapshot
                    .anchor_to_buffer_anchor(range.start)
                    .unwrap()
                    .0
                    .to_point(&snapshot)
            })
            .collect::<Vec<_>>()
    });
    multibuffer.update(cx_a, |multibuffer, cx| {
        multibuffer.set_excerpts_for_path(
            PathKey::for_buffer(&buffer, cx),
            buffer,
            [Point::row_range(1..5)],
            1,
            cx,
        );
    });

    let (workspace_b, cx_b) = client_b.join_workspace(channel, cx_b).await;
    cx_b.run_until_parked();
    let editor_b = workspace_b
        .update(cx_b, |workspace, cx| {
            workspace
                .active_item(cx)
                .and_then(|item| item.downcast::<Editor>())
        })
        .unwrap();

    let new_positions = editor_b.update(cx_b, |editor, _| {
        editor
            .selections
            .disjoint_anchor_ranges()
            .map(|range| {
                multibuffer_snapshot
                    .anchor_to_buffer_anchor(range.start)
                    .unwrap()
                    .0
                    .to_point(&snapshot)
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(positions, new_positions);
}

#[gpui::test]
async fn test_following_to_channel_notes_other_workspace(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let (_server, client_a, client_b, channel) = TestServer::start2(cx_a, cx_b).await;

    let mut cx_a2 = cx_a.clone();
    let (workspace_a, cx_a) = client_a.build_test_workspace(cx_a).await;
    join_channel(channel, &client_a, cx_a).await.unwrap();
    share_workspace(&workspace_a, cx_a).await.unwrap();

    // a opens 1.txt
    cx_a.simulate_keystrokes("cmd-p");
    cx_a.run_until_parked();
    cx_a.simulate_keystrokes("1 enter");
    cx_a.run_until_parked();
    workspace_a.update(cx_a, |workspace, cx| {
        let editor = workspace.active_item(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "1.txt");
    });

    // b joins channel and is following a
    join_channel(channel, &client_b, cx_b).await.unwrap();
    cx_b.run_until_parked();
    let (workspace_b, cx_b) = client_b.active_workspace(cx_b);
    workspace_b.update(cx_b, |workspace, cx| {
        let editor = workspace.active_item(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "1.txt");
    });

    // a opens a second workspace and the channel notes
    let (workspace_a2, cx_a2) = client_a.build_test_workspace(&mut cx_a2).await;
    cx_a2.update(|window, _| window.activate_window());
    cx_a2
        .update(|window, cx| ChannelView::open(channel, None, workspace_a2, window, cx))
        .await
        .unwrap();
    cx_a2.run_until_parked();

    // b should follow a to the channel notes
    workspace_b.update(cx_b, |workspace, cx| {
        let editor = workspace.active_item_as::<ChannelView>(cx).unwrap();
        assert_eq!(editor.read(cx).channel(cx).unwrap().id, channel);
    });

    // a returns to the shared project
    cx_a.update(|window, _| window.activate_window());
    cx_a.run_until_parked();

    workspace_a.update(cx_a, |workspace, cx| {
        let editor = workspace.active_item(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "1.txt");
    });

    // b should follow a back
    workspace_b.update(cx_b, |workspace, cx| {
        let editor = workspace.active_item_as::<Editor>(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "1.txt");
    });
}

#[gpui::test]
async fn test_following_while_deactivated(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let (_server, client_a, client_b, channel) = TestServer::start2(cx_a, cx_b).await;

    let mut cx_a2 = cx_a.clone();
    let (workspace_a, cx_a) = client_a.build_test_workspace(cx_a).await;
    join_channel(channel, &client_a, cx_a).await.unwrap();
    share_workspace(&workspace_a, cx_a).await.unwrap();

    // a opens 1.txt
    cx_a.simulate_keystrokes("cmd-p");
    cx_a.run_until_parked();
    cx_a.simulate_keystrokes("1 enter");
    cx_a.run_until_parked();
    workspace_a.update(cx_a, |workspace, cx| {
        let editor = workspace.active_item(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "1.txt");
    });

    // b joins channel and is following a
    join_channel(channel, &client_b, cx_b).await.unwrap();
    cx_b.run_until_parked();
    let (workspace_b, cx_b) = client_b.active_workspace(cx_b);
    workspace_b.update(cx_b, |workspace, cx| {
        let editor = workspace.active_item(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "1.txt");
    });

    // stop following
    cx_b.simulate_keystrokes("down");

    // a opens a different file while not followed
    cx_a.simulate_keystrokes("cmd-p");
    cx_a.run_until_parked();
    cx_a.simulate_keystrokes("2 enter");

    workspace_b.update(cx_b, |workspace, cx| {
        let editor = workspace.active_item_as::<Editor>(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "1.txt");
    });

    // a opens a file in a new window
    let (_, cx_a2) = client_a.build_test_workspace(&mut cx_a2).await;
    cx_a2.update(|window, _| window.activate_window());
    cx_a2.simulate_keystrokes("cmd-p");
    cx_a2.run_until_parked();
    cx_a2.simulate_keystrokes("3 enter");
    cx_a2.run_until_parked();

    // b starts following a again
    cx_b.simulate_keystrokes("cmd-ctrl-alt-f");
    cx_a.run_until_parked();

    // a returns to the shared project
    cx_a.update(|window, _| window.activate_window());
    cx_a.run_until_parked();

    workspace_a.update(cx_a, |workspace, cx| {
        let editor = workspace.active_item(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "2.js");
    });

    // b should follow a back
    workspace_b.update(cx_b, |workspace, cx| {
        let editor = workspace.active_item_as::<Editor>(cx).unwrap();
        assert_eq!(editor.tab_content_text(0, cx), "2.js");
    });
}

#[gpui::test(iterations = 10)]
async fn test_following_with_multibuffer_excerpts_at_unobserved_lamport(
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

    cx_a.update(editor::init);
    cx_b.update(editor::init);

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    client_a
        .fs()
        .insert_tree(path!("/a"), json!({ "1.txt": sample_text(20, 5, 'a') }))
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

    let buffer_a = project_a
        .update(cx_a, |p, cx| {
            p.open_buffer((worktree_id, rel_path("1.txt")), cx)
        })
        .await
        .unwrap();
    // B must already have the buffer open at a low Lamport so that A's
    // subsequent edits create anchors B hasn't observed.
    let _buffer_b = project_b
        .update(cx_b, |p, cx| {
            p.open_buffer((worktree_id, rel_path("1.txt")), cx)
        })
        .await
        .unwrap();

    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(client_a.peer_id().unwrap(), window, cx)
    });
    executor.run_until_parked();

    buffer_a.update(cx_a, |buf, cx| {
        for i in 0..30 {
            let len = buf.len();
            buf.edit([(len..len, format!("\nappended line {i}"))], None, cx);
        }
    });
    let multibuffer_a = cx_a.new(|cx| {
        let mut mb = MultiBuffer::new(Capability::ReadWrite);
        let max_row = buffer_a.read(cx).max_point().row;
        mb.set_excerpts_for_path(
            PathKey::for_buffer(&buffer_a, cx),
            buffer_a.clone(),
            [Point::row_range(max_row.saturating_sub(5)..max_row)],
            1,
            cx,
        );
        mb
    });
    workspace_a.update_in(cx_a, |workspace, window, cx| {
        let editor = cx
            .new(|cx| Editor::for_multibuffer(multibuffer_a, Some(project_a.clone()), window, cx));
        workspace.add_item_to_active_pane(Box::new(editor), None, true, window, cx);
    });

    executor.run_until_parked();

    let active_text = |workspace: &Entity<Workspace>, cx: &mut VisualTestContext| {
        workspace.update(cx, |workspace, cx| {
            workspace
                .active_item(cx)
                .unwrap()
                .downcast::<Editor>()
                .unwrap()
                .update(cx, |editor, cx| editor.text(cx))
        })
    };
    assert_eq!(
        active_text(&workspace_a, cx_a),
        active_text(&workspace_b, cx_b)
    );
}
