#[gpui::test]
async fn test_following_into_excluded_file(
    mut cx_a: &mut TestAppContext,
    mut cx_b: &mut TestAppContext,
) {
    let executor = cx_a.executor();
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    for cx in [&mut cx_a, &mut cx_b] {
        cx.update(|cx| {
            cx.update_global::<SettingsStore, _>(|store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.project.worktree.file_scan_exclusions =
                        Some(vec!["**/.git".to_string()]);
                });
            });
        });
    }
    server
        .create_room(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let peer_id_a = client_a.peer_id().unwrap();

    client_a
        .fs()
        .insert_tree(
            path!("/a"),
            json!({
                ".git": {
                    "COMMIT_EDITMSG": "write your commit message here",
                },
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

    // Client A opens editors for a regular file and an excluded file.
    let editor_for_regular = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("1.txt")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let editor_for_excluded_a = workspace_a
        .update_in(cx_a, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path(".git/COMMIT_EDITMSG")),
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

    // Client A updates their selections in those editors
    editor_for_regular.update_in(cx_a, |editor, window, cx| {
        editor.handle_input("a", window, cx);
        editor.handle_input("b", window, cx);
        editor.handle_input("c", window, cx);
        editor.select_left(&Default::default(), window, cx);
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(3)..MultiBufferOffset(2)]
        );
    });
    editor_for_excluded_a.update_in(cx_a, |editor, window, cx| {
        editor.select_all(&Default::default(), window, cx);
        editor.handle_input("new commit message", window, cx);
        editor.select_left(&Default::default(), window, cx);
        assert_eq!(
            editor.selections.ranges(&editor.display_snapshot(cx)),
            vec![MultiBufferOffset(18)..MultiBufferOffset(17)]
        );
    });

    // When client B starts following client A, currently visible file is replicated
    workspace_b.update_in(cx_b, |workspace, window, cx| {
        workspace.follow(peer_id_a, window, cx)
    });
    executor.advance_clock(workspace::item::LEADER_UPDATE_THROTTLE);
    executor.run_until_parked();

    let editor_for_excluded_b = workspace_b.update(cx_b, |workspace, cx| {
        workspace
            .active_item(cx)
            .unwrap()
            .downcast::<Editor>()
            .unwrap()
    });
    assert_eq!(
        cx_b.read(|cx| editor_for_excluded_b.read(cx).active_project_path(cx)),
        Some((worktree_id, rel_path(".git/COMMIT_EDITMSG")).into())
    );
    assert_eq!(
        editor_for_excluded_b.update(cx_b, |editor, cx| editor
            .selections
            .ranges(&editor.display_snapshot(cx))),
        vec![MultiBufferOffset(18)..MultiBufferOffset(17)]
    );

    editor_for_excluded_a.update_in(cx_a, |editor, window, cx| {
        editor.select_right(&Default::default(), window, cx);
    });
    executor.advance_clock(workspace::item::LEADER_UPDATE_THROTTLE);
    executor.run_until_parked();

    // Changes from B to the excluded file are replicated in A's editor
    editor_for_excluded_b.update_in(cx_b, |editor, window, cx| {
        editor.handle_input("\nCo-Authored-By: B <b@b.b>", window, cx);
    });
    executor.run_until_parked();
    editor_for_excluded_a.update(cx_a, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            "new commit message\nCo-Authored-By: B <b@b.b>"
        );
    });
}
