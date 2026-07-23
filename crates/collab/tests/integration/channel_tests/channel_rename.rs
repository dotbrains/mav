use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_channel_rename(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    let rust_id = server
        .make_channel("rust", None, (&client_a, cx_a), &mut [(&client_b, cx_b)])
        .await;

    // Rename the channel
    client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.rename(rust_id, "#rust-archive", cx)
        })
        .await
        .unwrap();

    executor.run_until_parked();

    // Client A sees the channel with its new name.
    assert_channels(
        client_a.channel_store(),
        cx_a,
        &[ExpectedChannel {
            depth: 0,
            id: rust_id,
            name: "rust-archive".into(),
        }],
    );

    // Client B sees the channel with its new name.
    assert_channels(
        client_b.channel_store(),
        cx_b,
        &[ExpectedChannel {
            depth: 0,
            id: rust_id,
            name: "rust-archive".into(),
        }],
    );
}
