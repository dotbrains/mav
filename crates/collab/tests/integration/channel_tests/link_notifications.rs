use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_channel_link_notifications(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;

    let user_b = client_b.user_id().unwrap();
    let user_c = client_c.user_id().unwrap();

    let channels = server
        .make_channel_tree(&[("mav", None)], (&client_a, cx_a))
        .await;
    let mav_channel = channels[0];

    try_join_all(client_a.channel_store().update(cx_a, |channel_store, cx| {
        [
            channel_store.set_channel_visibility(mav_channel, proto::ChannelVisibility::Public, cx),
            channel_store.invite_member(mav_channel, user_b, proto::ChannelRole::Member, cx),
            channel_store.invite_member(mav_channel, user_c, proto::ChannelRole::Guest, cx),
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

    client_c
        .channel_store()
        .update(cx_c, |channel_store, cx| {
            channel_store.respond_to_channel_invite(mav_channel, true, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // we have an admin (a), member (b) and guest (c) all part of the mav channel.

    // create a new private channel, make it public, and move it under the previous one, and verify it shows for b and not c
    let active_channel = client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.create_channel("active", Some(mav_channel), cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // the new channel shows for b and not c
    assert_channels_list_shape(
        client_a.channel_store(),
        cx_a,
        &[(mav_channel, 0), (active_channel, 1)],
    );
    assert_channels_list_shape(
        client_b.channel_store(),
        cx_b,
        &[(mav_channel, 0), (active_channel, 1)],
    );
    assert_channels_list_shape(client_c.channel_store(), cx_c, &[(mav_channel, 0)]);

    let vim_channel = client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.create_channel("vim", Some(mav_channel), cx)
        })
        .await
        .unwrap();

    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.set_channel_visibility(vim_channel, proto::ChannelVisibility::Public, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // the new channel shows for b and c
    assert_channels_list_shape(
        client_a.channel_store(),
        cx_a,
        &[(mav_channel, 0), (active_channel, 1), (vim_channel, 1)],
    );
    assert_channels_list_shape(
        client_b.channel_store(),
        cx_b,
        &[(mav_channel, 0), (active_channel, 1), (vim_channel, 1)],
    );
    assert_channels_list_shape(
        client_c.channel_store(),
        cx_c,
        &[(mav_channel, 0), (vim_channel, 1)],
    );

    let helix_channel = client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.create_channel("helix", Some(mav_channel), cx)
        })
        .await
        .unwrap();

    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.move_channel(helix_channel, vim_channel, cx)
        })
        .await
        .unwrap();

    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.set_channel_visibility(
                helix_channel,
                proto::ChannelVisibility::Public,
                cx,
            )
        })
        .await
        .unwrap();
    cx_a.run_until_parked();

    // the new channel shows for b and c
    assert_channels_list_shape(
        client_b.channel_store(),
        cx_b,
        &[
            (mav_channel, 0),
            (active_channel, 1),
            (vim_channel, 1),
            (helix_channel, 2),
        ],
    );
    assert_channels_list_shape(
        client_c.channel_store(),
        cx_c,
        &[(mav_channel, 0), (vim_channel, 1), (helix_channel, 2)],
    );
}
