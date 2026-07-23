use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_channel_jumping(executor: BackgroundExecutor, cx_a: &mut TestAppContext) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;

    let mav_id = server
        .make_channel("mav", None, (&client_a, cx_a), &mut [])
        .await;
    let rust_id = server
        .make_channel("rust", None, (&client_a, cx_a), &mut [])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);

    active_call_a
        .update(cx_a, |active_call, cx| active_call.join_channel(mav_id, cx))
        .await
        .unwrap();

    // Give everything a chance to observe user A joining
    executor.run_until_parked();

    cx_a.read(|cx| {
        client_a.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(
                channels.channel_participants(mav_id),
                &[client_a.user_id().unwrap()],
            );
            assert_participants_eq(channels.channel_participants(rust_id), &[]);
        })
    });

    active_call_a
        .update(cx_a, |active_call, cx| {
            active_call.join_channel(rust_id, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    cx_a.read(|cx| {
        client_a.channel_store().read_with(cx, |channels, _| {
            assert_participants_eq(channels.channel_participants(mav_id), &[]);
            assert_participants_eq(
                channels.channel_participants(rust_id),
                &[client_a.user_id().unwrap()],
            );
        })
    });
}
