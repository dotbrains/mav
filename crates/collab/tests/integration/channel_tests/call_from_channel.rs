use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_call_from_channel(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b)])
        .await;

    let channel_id = server
        .make_channel(
            "x",
            None,
            (&client_a, cx_a),
            &mut [(&client_b, cx_b), (&client_c, cx_c)],
        )
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);

    active_call_a
        .update(cx_a, |call, cx| call.join_channel(channel_id, cx))
        .await
        .unwrap();

    // Client A calls client B while in the channel.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();

    // Client B accepts the call.
    executor.run_until_parked();
    active_call_b
        .update(cx_b, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();

    // Client B sees that they are now in the channel
    executor.run_until_parked();
    cx_b.read(|cx| {
        active_call_b.read_with(cx, |call, cx| {
            assert_eq!(call.channel_id(cx), Some(channel_id));
        })
    });
    cx_b.read(|cx| {
        client_b.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(channel_id),
                &[client_a.user_id().unwrap(), client_b.user_id().unwrap()],
            );
        })
    });

    // Clients A and C also see that client B is in the channel.
    cx_a.read(|cx| {
        client_a.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(channel_id),
                &[client_a.user_id().unwrap(), client_b.user_id().unwrap()],
            );
        })
    });
    cx_c.read(|cx| {
        client_c.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(channel_id),
                &[client_a.user_id().unwrap(), client_b.user_id().unwrap()],
            );
        })
    });
}
