use call::ActiveCall;
use client::RECEIVE_TIMEOUT;
use collab::rpc::RECONNECT_TIMEOUT;
use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;

use crate::{TestClient, TestServer};

#[gpui::test(iterations = 10)]
async fn test_contacts(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_c: &mut TestAppContext,
    cx_d: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    let client_d = server.create_client(cx_d, "user_d").await;
    server
        .make_contacts(&mut [(&client_a, cx_a), (&client_b, cx_b), (&client_c, cx_c)])
        .await;
    let active_call_a = cx_a.read(ActiveCall::global);
    let active_call_b = cx_b.read(ActiveCall::global);
    let active_call_c = cx_c.read(ActiveCall::global);
    let _active_call_d = cx_d.read(ActiveCall::global);

    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(contacts(&client_d, cx_d), []);

    server.disconnect_client(client_c.peer_id().unwrap());
    server.forbid_connections();
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "offline", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "offline", "free")
        ]
    );
    assert_eq!(contacts(&client_c, cx_c), []);
    assert_eq!(contacts(&client_d, cx_d), []);

    server.allow_connections();
    client_c
        .connect(false, &cx_c.to_async())
        .await
        .into_response()
        .unwrap();

    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(contacts(&client_d, cx_d), []);

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "busy")
        ]
    );
    assert_eq!(contacts(&client_d, cx_d), []);

    // Client B and client D become contacts while client B is being called.
    server
        .make_contacts(&mut [(&client_b, cx_b), (&client_d, cx_d)])
        .await;
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free"),
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "busy")]
    );

    active_call_b.update(cx_b, |call, cx| call.decline_incoming(cx).unwrap());
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    active_call_c
        .update(cx_c, |call, cx| {
            call.invite(client_a.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "busy"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    active_call_a
        .update(cx_a, |call, cx| call.accept_incoming(cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "busy"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "busy"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "busy")]
    );

    active_call_a
        .update(cx_a, |call, cx| call.hang_up(cx))
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    active_call_a
        .update(cx_a, |call, cx| {
            call.invite(client_b.user_id().unwrap(), None, cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_a, cx_a),
        [
            ("user_b".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "online", "busy"),
            ("user_b".to_string(), "online", "busy")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "busy")]
    );

    server.forbid_connections();
    server.disconnect_client(client_a.peer_id().unwrap());
    executor.advance_clock(RECEIVE_TIMEOUT + RECONNECT_TIMEOUT);
    assert_eq!(contacts(&client_a, cx_a), []);
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "offline", "free"),
            ("user_c".to_string(), "online", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [
            ("user_a".to_string(), "offline", "free"),
            ("user_b".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_d, cx_d),
        [("user_b".to_string(), "online", "free")]
    );

    // Test removing a contact
    client_b
        .user_store()
        .update(cx_b, |store, cx| {
            store.remove_contact(client_c.user_id().unwrap(), cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();
    assert_eq!(
        contacts(&client_b, cx_b),
        [
            ("user_a".to_string(), "offline", "free"),
            ("user_d".to_string(), "online", "free")
        ]
    );
    assert_eq!(
        contacts(&client_c, cx_c),
        [("user_a".to_string(), "offline", "free"),]
    );

    fn contacts(
        client: &TestClient,
        cx: &TestAppContext,
    ) -> Vec<(String, &'static str, &'static str)> {
        client.user_store().read_with(cx, |store, _| {
            store
                .contacts()
                .iter()
                .map(|contact| {
                    (
                        contact.user.username.clone().to_string(),
                        if contact.online { "online" } else { "offline" },
                        if contact.busy { "busy" } else { "free" },
                    )
                })
                .collect()
        })
    }
}
