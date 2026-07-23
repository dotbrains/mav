use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_guest_access(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    let channels = server
        .make_channel_tree(
            &[("channel-a", None), ("channel-b", Some("channel-a"))],
            (&client_a, cx_a),
        )
        .await;
    let channel_a = channels[0];
    let channel_b = channels[1];

    let active_call_b = cx_b.read(ActiveCall::global);

    // Non-members should not be allowed to join
    assert!(
        active_call_b
            .update(cx_b, |call, cx| call.join_channel(channel_a, cx))
            .await
            .is_err()
    );

    // Make channels A and B public
    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.set_channel_visibility(channel_a, proto::ChannelVisibility::Public, cx)
        })
        .await
        .unwrap();
    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.set_channel_visibility(channel_b, proto::ChannelVisibility::Public, cx)
        })
        .await
        .unwrap();

    // Client B joins channel A as a guest
    active_call_b
        .update(cx_b, |call, cx| call.join_channel(channel_a, cx))
        .await
        .unwrap();

    executor.run_until_parked();
    assert_channels_list_shape(
        client_a.channel_store(),
        cx_a,
        &[(channel_a, 0), (channel_b, 1)],
    );
    assert_channels_list_shape(
        client_b.channel_store(),
        cx_b,
        &[(channel_a, 0), (channel_b, 1)],
    );

    client_a.channel_store().update(cx_a, |channel_store, _| {
        let participants = channel_store.channel_participants(channel_a);
        assert_eq!(participants.len(), 1);
        assert_eq!(participants[0].legacy_id, client_b.user_id().unwrap());
    });
}

#[gpui::test]
async fn test_invite_access(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    let channels = server
        .make_channel_tree(
            &[("channel-a", None), ("channel-b", Some("channel-a"))],
            (&client_a, cx_a),
        )
        .await;
    let channel_a_id = channels[0];
    let channel_b_id = channels[0];

    let active_call_b = cx_b.read(ActiveCall::global);

    // should not be allowed to join
    assert!(
        active_call_b
            .update(cx_b, |call, cx| call.join_channel(channel_b_id, cx))
            .await
            .is_err()
    );

    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.invite_member(
                channel_a_id,
                client_b.user_id().unwrap(),
                ChannelRole::Member,
                cx,
            )
        })
        .await
        .unwrap();

    active_call_b
        .update(cx_b, |call, cx| call.join_channel(channel_b_id, cx))
        .await
        .unwrap();

    executor.run_until_parked();

    client_b.channel_store().update(cx_b, |channel_store, _| {
        assert!(channel_store.channel_for_id(channel_b_id).is_some());
        assert!(channel_store.channel_for_id(channel_a_id).is_some());
    });

    client_a.channel_store().update(cx_a, |channel_store, _| {
        let participants = channel_store.channel_participants(channel_b_id);
        assert_eq!(participants.len(), 1);
        assert_eq!(participants[0].legacy_id, client_b.user_id().unwrap());
    })
}
