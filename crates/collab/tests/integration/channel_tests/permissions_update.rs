use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_permissions_update_while_invited(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    let rust_id = server
        .make_channel("rust", None, (&client_a, cx_a), &mut [])
        .await;

    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.invite_member(
                rust_id,
                client_b.user_id().unwrap(),
                proto::ChannelRole::Member,
                cx,
            )
        })
        .await
        .unwrap();

    executor.run_until_parked();

    assert_channel_invitations(
        client_b.channel_store(),
        cx_b,
        &[ExpectedChannel {
            depth: 0,
            id: rust_id,
            name: "rust".into(),
        }],
    );
    assert_channels(client_b.channel_store(), cx_b, &[]);

    // Update B's invite before they've accepted it
    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.set_member_role(
                rust_id,
                client_b.user_id().unwrap(),
                proto::ChannelRole::Admin,
                cx,
            )
        })
        .await
        .unwrap();

    executor.run_until_parked();

    assert_channel_invitations(
        client_b.channel_store(),
        cx_b,
        &[ExpectedChannel {
            depth: 0,
            id: rust_id,
            name: "rust".into(),
        }],
    );
    assert_channels(client_b.channel_store(), cx_b, &[]);
}
