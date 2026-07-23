use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_leave_channel(cx_a: &mut TestAppContext, cx_b: &mut TestAppContext) {
    let (_server, _client_a, client_b, channel_id) = TestServer::start2(cx_a, cx_b).await;

    client_b
        .channel_store()
        .update(cx_b, |channel_store, cx| {
            channel_store.remove_member(channel_id, client_b.user_id().unwrap(), cx)
        })
        .await
        .unwrap();

    cx_a.run_until_parked();

    assert_eq!(
        client_b
            .channel_store()
            .read_with(cx_b, |store, _| store.channels().count()),
        0
    );
}

#[gpui::test]
async fn test_channel_moving(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    _cx_b: &mut TestAppContext,
    _cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;

    let channels = server
        .make_channel_tree(
            &[
                ("channel-a", None),
                ("channel-b", Some("channel-a")),
                ("channel-c", Some("channel-b")),
                ("channel-d", Some("channel-c")),
            ],
            (&client_a, cx_a),
        )
        .await;
    let channel_a_id = channels[0];
    let channel_b_id = channels[1];
    let channel_c_id = channels[2];
    let channel_d_id = channels[3];

    // Current shape:
    // a - b - c - d
    assert_channels_list_shape(
        client_a.channel_store(),
        cx_a,
        &[
            (channel_a_id, 0),
            (channel_b_id, 1),
            (channel_c_id, 2),
            (channel_d_id, 3),
        ],
    );

    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.move_channel(channel_d_id, channel_b_id, cx)
        })
        .await
        .unwrap();

    // Current shape:
    //       /- d
    // a - b -- c
    assert_channels_list_shape(
        client_a.channel_store(),
        cx_a,
        &[
            (channel_a_id, 0),
            (channel_b_id, 1),
            (channel_c_id, 2),
            (channel_d_id, 2),
        ],
    );
}
