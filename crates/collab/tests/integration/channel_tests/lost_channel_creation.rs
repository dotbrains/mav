use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_lost_channel_creation(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let channel_id = server
        .make_channel("x", None, (&client_a, cx_a), &mut [])
        .await;

    // Invite a member
    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.invite_member(
                channel_id,
                client_b.user_id().unwrap(),
                proto::ChannelRole::Member,
                cx,
            )
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // Sanity check, B has the invitation
    assert_channel_invitations(
        client_b.channel_store(),
        cx_b,
        &[ExpectedChannel {
            depth: 0,
            id: channel_id,
            name: "x".into(),
        }],
    );

    // A creates a subchannel while the invite is still pending.
    let subchannel_id = client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.create_channel("subchannel", Some(channel_id), cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // Make sure A sees their new channel
    assert_channels(
        client_a.channel_store(),
        cx_a,
        &[
            ExpectedChannel {
                depth: 0,
                id: channel_id,
                name: "x".into(),
            },
            ExpectedChannel {
                depth: 1,
                id: subchannel_id,
                name: "subchannel".into(),
            },
        ],
    );

    // Client B accepts the invite
    client_b
        .channel_store()
        .update(cx_b, |channel_store, cx| {
            channel_store.respond_to_channel_invite(channel_id, true, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // Client B should now see the channel
    assert_channels(
        client_b.channel_store(),
        cx_b,
        &[
            ExpectedChannel {
                depth: 0,
                id: channel_id,
                name: "x".into(),
            },
            ExpectedChannel {
                depth: 1,
                id: subchannel_id,
                name: "subchannel".into(),
            },
        ],
    );
}
