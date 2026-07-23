use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_channel_notes_participant_indices(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    cx_a.update(editor::init);
    cx_b.update(editor::init);
    cx_c.update(editor::init);

    let channel_id = server
        .make_channel(
            "the-channel",
            None,
            (&client_a, cx_a),
            &mut [(&client_b, cx_b), (&client_c, cx_c)],
        )
        .await;

    client_a
        .fs()
        .insert_tree("/root", json!({"file.txt": "123"}))
        .await;
    let (project_a, worktree_id_a) = client_a.build_local_project_with_trust("/root", cx_a).await;
    let project_b = client_b.build_empty_local_project(false, cx_b);
    let project_c = client_c.build_empty_local_project(false, cx_c);

    let (workspace_a, mut cx_a) = client_a.build_workspace(&project_a, cx_a);
    let (workspace_b, mut cx_b) = client_b.build_workspace(&project_b, cx_b);
    let (workspace_c, cx_c) = client_c.build_workspace(&project_c, cx_c);

    // Clients A, B, and C open the channel notes
    let channel_view_a = cx_a
        .update(|window, cx| ChannelView::open(channel_id, None, workspace_a.clone(), window, cx))
        .await
        .unwrap();
    let channel_view_b = cx_b
        .update(|window, cx| ChannelView::open(channel_id, None, workspace_b.clone(), window, cx))
        .await
        .unwrap();
    let channel_view_c = cx_c
        .update(|window, cx| ChannelView::open(channel_id, None, workspace_c.clone(), window, cx))
        .await
        .unwrap();

    // Clients A, B, and C all insert and select some text
    channel_view_a.update_in(cx_a, |notes, window, cx| {
        notes.editor.update(cx, |editor, cx| {
            editor.insert("a", window, cx);
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections.select_ranges(vec![MultiBufferOffset(0)..MultiBufferOffset(1)]);
            });
        });
    });
    executor.run_until_parked();
    channel_view_b.update_in(cx_b, |notes, window, cx| {
        notes.editor.update(cx, |editor, cx| {
            editor.move_down(&Default::default(), window, cx);
            editor.insert("b", window, cx);
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections.select_ranges(vec![MultiBufferOffset(1)..MultiBufferOffset(2)]);
            });
        });
    });
    executor.run_until_parked();
    channel_view_c.update_in(cx_c, |notes, window, cx| {
        notes.editor.update(cx, |editor, cx| {
            editor.move_down(&Default::default(), window, cx);
            editor.insert("c", window, cx);
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections.select_ranges(vec![MultiBufferOffset(2)..MultiBufferOffset(3)]);
            });
        });
    });

    // Client A sees clients B and C without assigned colors, because they aren't
    // in a call together.
    executor.run_until_parked();
    channel_view_a.update_in(cx_a, |notes, window, cx| {
        notes.editor.update(cx, |editor, cx| {
            assert_remote_selections(editor, &[(None, 1..2), (None, 2..3)], window, cx);
        });
    });

    // Clients A and B join the same call.
    for (call, cx) in [(&active_call_a, &mut cx_a), (&active_call_b, &mut cx_b)] {
        call.update(*cx, |call, cx| call.join_channel(channel_id, cx))
            .await
            .unwrap();
    }

    // Clients A and B see each other with two different assigned colors. Client C
    // still doesn't have a color.
    executor.run_until_parked();
    channel_view_a.update_in(cx_a, |notes, window, cx| {
        notes.editor.update(cx, |editor, cx| {
            assert_remote_selections(
                editor,
                &[(Some(ParticipantIndex(1)), 1..2), (None, 2..3)],
                window,
                cx,
            );
        });
    });
    channel_view_b.update_in(cx_b, |notes, window, cx| {
        notes.editor.update(cx, |editor, cx| {
            assert_remote_selections(
                editor,
                &[(Some(ParticipantIndex(0)), 0..1), (None, 2..3)],
                window,
                cx,
            );
        });
    });

    // Client A shares a project, and client B joins.
    let project_id = active_call_a
        .update(cx_a, |call, cx| call.share_project(project_a.clone(), cx))
        .await
        .unwrap();
    let project_b = client_b.join_remote_project(project_id, cx_b).await;
    let (workspace_b, cx_b) = client_b.build_workspace(&project_b, cx_b);

    // Clients A and B open the same file.
    let editor_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id_a, rel_path("file.txt")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let editor_b = workspace_b
        .update_in(cx_b, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id_a, rel_path("file.txt")),
                None,
                true,
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    editor_a.update_in(cx_a, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges(vec![MultiBufferOffset(0)..MultiBufferOffset(1)]);
        });
    });
    editor_b.update_in(cx_b, |editor, window, cx| {
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges(vec![MultiBufferOffset(2)..MultiBufferOffset(3)]);
        });
    });
    executor.run_until_parked();

    // Clients A and B see each other with the same colors as in the channel notes.
    editor_a.update_in(cx_a, |editor, window, cx| {
        assert_remote_selections(editor, &[(Some(ParticipantIndex(1)), 2..3)], window, cx);
    });
    editor_b.update_in(cx_b, |editor, window, cx| {
        assert_remote_selections(editor, &[(Some(ParticipantIndex(0)), 0..1)], window, cx);
    });
}
