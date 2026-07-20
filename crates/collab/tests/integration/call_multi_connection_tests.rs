use call::ActiveCall;
use client::RECEIVE_TIMEOUT;
use collab::rpc::RECONNECT_TIMEOUT;
use futures::StreamExt as _;
use gpui::{BackgroundExecutor, TestAppContext};

use crate::TestServer;

#[gpui::test(iterations = 10)]
async fn test_calls_on_multiple_connections(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b1: &mut TestAppContext,
    cx_b2: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b1 = server.create_client(cx_b1, "user_b").await;
    let client_b2 = server.create_client(cx_b2, "user_b").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b1, cx_b1)])
        .await;

    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b1 = cx_b1.read(ActiveCall::global);
    let active_call_b2 = cx_b2.read(ActiveCall::global);

    let mut incoming_call_b1 = active_call_b1.read_with(cx_b1, |call, _| call.incoming());

    let mut incoming_call_b2 = active_call_b2.read_with(cx_b2, |call, _| call.incoming());
    assert!(incoming_call_b1.next().await.unwrap().is_none());
    assert!(incoming_call_b2.next().await.unwrap().is_none());

    // Call user B from client A, ensuring both clients for user B ring.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b1.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_some());
    assert!(incoming_call_b2.next().await.unwrap().is_some());

    // User B declines the call on one of the two connections, causing both connections
    // to stop ringing.
    active_call_b2.update(cx_b2, |call, cx| call.decline_incoming(cx).unwrap());
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_none());
    assert!(incoming_call_b2.next().await.unwrap().is_none());

    // Call user B again from client A.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b1.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_some());
    assert!(incoming_call_b2.next().await.unwrap().is_some());

    // User B accepts the call on one of the two connections, causing both connections
    // to stop ringing.
    active_call_b2
        .update(cx_b2, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_none());
    assert!(incoming_call_b2.next().await.unwrap().is_none());

    // User B disconnects the client that is not on the call. Everything should be fine.
    client_b1.disconnect(&cx_b1.to_async());
    executor.advance_clock(RECEIVE_TIMEOUT);
    client_b1
        .connect(false, &cx_b1.to_async())
        .await
        .into_response()
        .unwrap();

    // User B hangs up, and user A calls them again.
    active_call_b2
        .update(cx_b2, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b1.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_some());
    assert!(incoming_call_b2.next().await.unwrap().is_some());

    // User A cancels the call, causing both connections to stop ringing.
    active_call_a
        .update(cx_a, |call, cx| {
            call.cancel_invite(client_b1.user_id().unwrap(), cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_none());
    assert!(incoming_call_b2.next().await.unwrap().is_none());

    // User A calls user B again.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b1.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_some());
    assert!(incoming_call_b2.next().await.unwrap().is_some());

    // User A hangs up, causing both connections to stop ringing.
    active_call_a
        .update(cx_a, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_none());
    assert!(incoming_call_b2.next().await.unwrap().is_none());

    // User A calls user B again.
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b1.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_some());
    assert!(incoming_call_b2.next().await.unwrap().is_some());

    // User A disconnects, causing both connections to stop ringing.
    server.forbid_connections();
    server.disconnect_client(client_a.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);
    assert!(incoming_call_b1.next().await.unwrap().is_none());
    assert!(incoming_call_b2.next().await.unwrap().is_none());

    // User A reconnects automatically, then calls user B again.
    server.allow_connections();
    executor.advance_clock(RECONNECT_TIMEOUT);
    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b1.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert!(incoming_call_b1.next().await.unwrap().is_some());
    assert!(incoming_call_b2.next().await.unwrap().is_some());

    // User B disconnects all clients, causing user A to no longer see a pending call for them.
    server.forbid_connections();
    server.disconnect_client(client_b1.peer_id().unwrap());
    server.disconnect_client(client_b2.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);

    active_call_a.read_with(cx_a, |call, _| assert!(call.room().is_none()));
}
