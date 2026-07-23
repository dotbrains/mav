use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_channel_membership_notifications(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_c").await;

    let user_b = client_b.user_id().unwrap();

    let channels = server
        .make_channel_tree(
            &[("mav", None), ("vim", Some("mav")), ("opensource", None)],
            (&client_a, cx_a),
        )
        .await;
    let mav_channel = channels[0];
    let vim_channel = channels[1];
    let opensource_channel = channels[2];

    try_join_all(client_a.channel_store().update(cx_a, |channel_store, cx| {
        [
            channel_store.set_channel_visibility(mav_channel, proto::ChannelVisibility::Public, cx),
            channel_store.set_channel_visibility(vim_channel, proto::ChannelVisibility::Public, cx),
            channel_store.invite_member(mav_channel, user_b, proto::ChannelRole::Admin, cx),
            channel_store.invite_member(opensource_channel, user_b, proto::ChannelRole::Member, cx),
        ]
    }))
    .await
    .unwrap();

    executor.run_until_parked();

    client_b
        .channel_store()
        .update(cx_b, |channel_store, cx| {
            channel_store.respond_to_channel_invite(mav_channel, true, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // we have an admin (a), and a guest (b) with access to all of mav, and membership in vim.
    assert_channels(
        client_b.channel_store(),
        cx_b,
        &[
            ExpectedChannel {
                depth: 0,
                id: mav_channel,
                name: "mav".into(),
            },
            ExpectedChannel {
                depth: 1,
                id: vim_channel,
                name: "vim".into(),
            },
        ],
    );

    client_b.channel_store().update(cx_b, |channel_store, _| {
        channel_store.is_channel_admin(mav_channel)
    });

    client_b
        .channel_store()
        .update(cx_b, |channel_store, cx| {
            channel_store.respond_to_channel_invite(opensource_channel, true, cx)
        })
        .await
        .unwrap();

    cx_a.run_until_parked();

    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.set_member_role(opensource_channel, user_b, ChannelRole::Admin, cx)
        })
        .await
        .unwrap();

    cx_a.run_until_parked();

    client_b.channel_store().update(cx_b, |channel_store, _| {
        channel_store.is_channel_admin(opensource_channel)
    });
}
