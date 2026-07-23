use super::*;

#[gpui::test]
async fn test_auto_watch_reopens_screen_share_from_returning_channel_participant(
    executor: BackgroundExecutor,
    user_a: &mut TestAppContext,
    user_b: &mut TestAppContext,
    user_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let setup = setup_auto_watch_late_joiner_test(&mut server, user_a, user_b, user_c).await;
    let (workspace_a, user_a) = setup
        .client_a
        .build_workspace(&setup.user_a_project, user_a);
    let (workspace_b, user_b) = setup
        .client_b
        .build_workspace(&setup.user_b_project, user_b);

    workspace_a.update_in(user_a, |workspace, window, cx| {
        workspace.toggle_auto_watch(window, cx);
    });
    workspace_b.update_in(user_b, |workspace, window, cx| {
        workspace.toggle_auto_watch(window, cx);
    });
    executor.run_until_parked();

    let active_call_c = user_c.read(ActiveCall::global);
    active_call_c
        .update(user_c, |call, cx| call.join_channel(setup.channel_id, cx))
        .await
        .unwrap();
    executor.run_until_parked();

    start_screen_share(user_c).await;
    executor.run_until_parked();

    workspace_a.update(user_a, |workspace, cx| {
        assert_active_item_is_screen_share_for_peer(
            workspace,
            setup.client_c.peer_id().unwrap(),
            cx,
        );
    });
    workspace_b.update(user_b, |workspace, cx| {
        assert_active_item_is_screen_share_for_peer(
            workspace,
            setup.client_c.peer_id().unwrap(),
            cx,
        );
    });

    active_call_c
        .update(user_c, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();

    workspace_a.update(user_a, |workspace, cx| {
        assert_no_screen_share_tabs_exist(
            workspace,
            "user A should stop seeing user C's screen after user C hangs up",
            cx,
        );
    });
    workspace_b.update(user_b, |workspace, cx| {
        assert_no_screen_share_tabs_exist(
            workspace,
            "user B should stop seeing user C's screen after user C hangs up",
            cx,
        );
    });

    let active_call_c = user_c.read(ActiveCall::global);
    active_call_c
        .update(user_c, |call, cx| call.join_channel(setup.channel_id, cx))
        .await
        .unwrap();
    executor.run_until_parked();

    start_screen_share(user_c).await;
    executor.run_until_parked();

    workspace_a.update(user_a, |workspace, cx| {
        assert_active_item_is_screen_share_for_peer(
            workspace,
            setup.client_c.peer_id().unwrap(),
            cx,
        );
    });
    workspace_b.update(user_b, |workspace, cx| {
        assert_active_item_is_screen_share_for_peer(
            workspace,
            setup.client_c.peer_id().unwrap(),
            cx,
        );
    });
}
