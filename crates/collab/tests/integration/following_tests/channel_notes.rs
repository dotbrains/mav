use super::*;

#[gpui::test(iterations = 10)]
async fn test_following_to_channel_notes_without_a_shared_project(
    deterministic: BackgroundExecutor,
    mut cx_a: &mut TestAppContext,
    mut cx_b: &mut TestAppContext,
    mut cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(deterministic.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;

    cx_a.update(editor::init);
    cx_b.update(editor::init);
    cx_c.update(editor::init);
    cx_a.update(collab_ui::channel_view::init);
    cx_b.update(collab_ui::channel_view::init);
    cx_c.update(collab_ui::channel_view::init);

    let channel_1_id = server
        .make_channel(
            "channel-1",
            None,
            (&client_a, cx_a),
            &mut [(&client_b, cx_b), (&client_c, cx_c)],
        )
        .await;
    let channel_2_id = server
        .make_channel(
            "channel-2",
            None,
            (&client_a, cx_a),
            &mut [(&client_b, cx_b), (&client_c, cx_c)],
        )
        .await;

    // Clients A, B, and C join a channel.
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let active_call_c = cx_c.read(ActiveCall::global);
    for (call, cx) in [
        (&active_call_a, &mut cx_a),
        (&active_call_b, &mut cx_b),
        (&active_call_c, &mut cx_c),
    ] {
        call.update(*cx, |call, cx| call.join_channel(channel_1_id, cx))
            .await
            .unwrap();
    }
    deterministic.run_until_parked();

    // Clients A, B, and C all open their own unshared projects.
    client_a
        .fs()
        .insert_tree("/a", json!({ "1.txt": "" }))
        .await;
    client_b.fs().insert_tree("/b", json!({})).await;
    client_c.fs().insert_tree("/c", json!({})).await;
    let (project_a, worktree_id) = client_a.build_local_project("/a", cx_a).await;
    let (project_b, _) = client_b.build_local_project("/b", cx_b).await;
    let (project_c, _) = client_b.build_local_project("/c", cx_c).await;
    let (workspace_a, cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);
    let (_workspace_c, _cx_c) = client_c.build_workspace(&project_c, cx_c);

    active_call_a
        .update(cx_a, |call, cx| call.set_location(Some(&project_a), cx))
        .await
        .unwrap();

    // Client A opens the notes for channel 1.
    let channel_notes_1_a = cx_a
        .update(|window, cx| ChannelView::open(channel_1_id, None, workspace_a.clone(), window, cx))
        .await
        .unwrap();
    channel_notes_1_a.update_in(cx_a, |notes, window, cx| {
        assert_eq!(notes.channel(cx).unwrap().name, "channel-1");
        notes.editor.update(cx, |editor, cx| {
            editor.insert("Hello from A.", window, cx);
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                selections.select_ranges(vec![MultiBufferOffset(3)..MultiBufferOffset(4)]);
            });
        });
    });

    // Ensure client A's edits are synced to the server before client B starts following.
    deterministic.run_until_parked();

    // Client B follows client A.
    workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace
                .start_following(client_a.peer_id().unwrap(), window, cx)
                .unwrap()
        })
        .await
        .unwrap();

    // Client B is taken to the notes for channel 1, with the same
    // text selected as client A.
    deterministic.run_until_parked();
    let channel_notes_1_b = workspace_b.update(cx_b, |workspace, cx| {
        assert_eq!(
            workspace.leader_for_pane(workspace.active_pane()),
            Some(client_a.peer_id().unwrap().into())
        );
        workspace
            .active_item(cx)
            .expect("no active item")
            .downcast::<ChannelView>()
            .expect("active item is not a channel view")
    });
    channel_notes_1_b.update(cx_b, |notes, cx| {
        assert_eq!(notes.channel(cx).unwrap().name, "channel-1");
        notes.editor.update(cx, |editor, cx| {
            assert_eq!(editor.text(cx), "Hello from A.");
            assert_eq!(
                editor
                    .selections
                    .ranges::<MultiBufferOffset>(&editor.display_snapshot(cx)),
                &[MultiBufferOffset(3)..MultiBufferOffset(4)]
            );
        })
    });

    //  Client A opens the notes for channel 2.
    let channel_notes_2_a = cx_a
        .update(|window, cx| ChannelView::open(channel_2_id, None, workspace_a.clone(), window, cx))
        .await
        .unwrap();
    channel_notes_2_a.update(cx_a, |notes, cx| {
        assert_eq!(notes.channel(cx).unwrap().name, "channel-2");
    });

    // Client B is taken to the notes for channel 2.
    deterministic.run_until_parked();
    let channel_notes_2_b = workspace_b.update(cx_b, |workspace, cx| {
        assert_eq!(
            workspace.leader_for_pane(workspace.active_pane()),
            Some(client_a.peer_id().unwrap().into())
        );
        workspace
            .active_item(cx)
            .expect("no active item")
            .downcast::<ChannelView>()
            .expect("active item is not a channel view")
    });
    channel_notes_2_b.update(cx_b, |notes, cx| {
        assert_eq!(notes.channel(cx).unwrap().name, "channel-2");
    });

    // Client A opens a local buffer in their unshared project.
    let _unshared_editor_a1 = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("1.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    // This does not send any leader update message to client B.
    // If it did, an error would occur on client B, since this buffer
    // is not shared with them.
    deterministic.run_until_parked();
    workspace_b.update(cx_b, |workspace, cx| {
        assert_eq!(
            workspace.active_item(cx).expect("no active item").item_id(),
            channel_notes_2_b.entity_id()
        );
    });
}
