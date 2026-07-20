use gpui::{BackgroundExecutor, TestAppContext};
use pretty_assertions::assert_eq;

use crate::{TestClient, TestServer};

#[gpui::test(iterations = 10)]
async fn test_contact_requests(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_a2: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_b2: &mut TestAppContext,
    cx_c: &mut TestAppContext,
    cx_c2: &mut TestAppContext,
) {
    // Connect to a server as 3 clients.
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_a2 = server.create_client(cx_a2, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;
    let client_b2 = server.create_client(cx_b2, "user_b").await;
    let client_c = server.create_client(cx_c, "user_c").await;
    let client_c2 = server.create_client(cx_c2, "user_c").await;

    assert_eq!(client_a.user_id().unwrap(), client_a2.user_id().unwrap());
    assert_eq!(client_b.user_id().unwrap(), client_b2.user_id().unwrap());
    assert_eq!(client_c.user_id().unwrap(), client_c2.user_id().unwrap());

    // User A and User C request that user B become their contact.
    client_a
        .user_store()
        .update(cx_a, |store, cx| {
            store.request_contact(client_b.user_id().unwrap(), cx)
        })
        .await
        .unwrap();
    client_c
        .user_store()
        .update(cx_c, |store, cx| {
            store.request_contact(client_b.user_id().unwrap(), cx)
        })
        .await
        .unwrap();
    executor.run_until_parked();

    // All users see the pending request appear in all their clients.
    assert_eq!(
        client_a.summarize_contacts(cx_a).outgoing_requests,
        &["user_b"]
    );
    assert_eq!(
        client_a2.summarize_contacts(cx_a2).outgoing_requests,
        &["user_b"]
    );
    assert_eq!(
        client_b.summarize_contacts(cx_b).incoming_requests,
        &["user_a", "user_c"]
    );
    assert_eq!(
        client_b2.summarize_contacts(cx_b2).incoming_requests,
        &["user_a", "user_c"]
    );
    assert_eq!(
        client_c.summarize_contacts(cx_c).outgoing_requests,
        &["user_b"]
    );
    assert_eq!(
        client_c2.summarize_contacts(cx_c2).outgoing_requests,
        &["user_b"]
    );

    // Contact requests are present upon connecting (tested here via disconnect/reconnect)
    disconnect_and_reconnect(&client_a, cx_a).await;
    disconnect_and_reconnect(&client_b, cx_b).await;
    disconnect_and_reconnect(&client_c, cx_c).await;
    executor.run_until_parked();
    assert_eq!(
        client_a.summarize_contacts(cx_a).outgoing_requests,
        &["user_b"]
    );
    assert_eq!(
        client_b.summarize_contacts(cx_b).incoming_requests,
        &["user_a", "user_c"]
    );
    assert_eq!(
        client_c.summarize_contacts(cx_c).outgoing_requests,
        &["user_b"]
    );

    // User B accepts the request from user A.
    client_b
        .user_store()
        .update(cx_b, |store, cx| {
            store.respond_to_contact_request(client_a.user_id().unwrap(), true, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // User B sees user A as their contact now in all client, and the incoming request from them is removed.
    let contacts_b = client_b.summarize_contacts(cx_b);
    assert_eq!(contacts_b.current, &["user_a"]);
    assert_eq!(contacts_b.incoming_requests, &["user_c"]);
    let contacts_b2 = client_b2.summarize_contacts(cx_b2);
    assert_eq!(contacts_b2.current, &["user_a"]);
    assert_eq!(contacts_b2.incoming_requests, &["user_c"]);

    // User A sees user B as their contact now in all clients, and the outgoing request to them is removed.
    let contacts_a = client_a.summarize_contacts(cx_a);
    assert_eq!(contacts_a.current, &["user_b"]);
    assert!(contacts_a.outgoing_requests.is_empty());
    let contacts_a2 = client_a2.summarize_contacts(cx_a2);
    assert_eq!(contacts_a2.current, &["user_b"]);
    assert!(contacts_a2.outgoing_requests.is_empty());

    // Contacts are present upon connecting (tested here via disconnect/reconnect)
    disconnect_and_reconnect(&client_a, cx_a).await;
    disconnect_and_reconnect(&client_b, cx_b).await;
    disconnect_and_reconnect(&client_c, cx_c).await;
    executor.run_until_parked();
    assert_eq!(client_a.summarize_contacts(cx_a).current, &["user_b"]);
    assert_eq!(client_b.summarize_contacts(cx_b).current, &["user_a"]);
    assert_eq!(
        client_b.summarize_contacts(cx_b).incoming_requests,
        &["user_c"]
    );
    assert!(client_c.summarize_contacts(cx_c).current.is_empty());
    assert_eq!(
        client_c.summarize_contacts(cx_c).outgoing_requests,
        &["user_b"]
    );

    // User B rejects the request from user C.
    client_b
        .user_store()
        .update(cx_b, |store, cx| {
            store.respond_to_contact_request(client_c.user_id().unwrap(), false, cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // User B doesn't see user C as their contact, and the incoming request from them is removed.
    let contacts_b = client_b.summarize_contacts(cx_b);
    assert_eq!(contacts_b.current, &["user_a"]);
    assert!(contacts_b.incoming_requests.is_empty());
    let contacts_b2 = client_b2.summarize_contacts(cx_b2);
    assert_eq!(contacts_b2.current, &["user_a"]);
    assert!(contacts_b2.incoming_requests.is_empty());

    // User C doesn't see user B as their contact, and the outgoing request to them is removed.
    let contacts_c = client_c.summarize_contacts(cx_c);
    assert!(contacts_c.current.is_empty());
    assert!(contacts_c.outgoing_requests.is_empty());
    let contacts_c2 = client_c2.summarize_contacts(cx_c2);
    assert!(contacts_c2.current.is_empty());
    assert!(contacts_c2.outgoing_requests.is_empty());

    // Incoming/outgoing requests are not present upon connecting (tested here via disconnect/reconnect)
    disconnect_and_reconnect(&client_a, cx_a).await;
    disconnect_and_reconnect(&client_b, cx_b).await;
    disconnect_and_reconnect(&client_c, cx_c).await;
    executor.run_until_parked();
    assert_eq!(client_a.summarize_contacts(cx_a).current, &["user_b"]);
    assert_eq!(client_b.summarize_contacts(cx_b).current, &["user_a"]);
    assert!(
        client_b
            .summarize_contacts(cx_b)
            .incoming_requests
            .is_empty()
    );
    assert!(client_c.summarize_contacts(cx_c).current.is_empty());
    assert!(
        client_c
            .summarize_contacts(cx_c)
            .outgoing_requests
            .is_empty()
    );

    async fn disconnect_and_reconnect(client: &TestClient, cx: &mut TestAppContext) {
        client.disconnect(&cx.to_async());
        client.clear_contacts(cx).await;
        client
            .connect(false, &cx.to_async())
            .await
            .into_response()
            .unwrap();
    }
}
