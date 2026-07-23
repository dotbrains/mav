use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_joining_channel_ancestor_member(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;

    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    let parent_id = server
        .make_channel("parent", None, (&client_a, cx_a), &mut [(&client_b, cx_b)])
        .await;

    let sub_id = client_a
        .channel_store()
        .update(cx_a, |channel_store, cx| {
            channel_store.create_channel("sub_channel", Some(parent_id), cx)
        })
        .await
        .unwrap();

    let active_call_b = cx_b.read(ActiveCall::global);

    assert!(
        active_call_b
            .update(cx_b, |active_call, cx| active_call.join_channel(sub_id, cx))
            .await
            .is_ok()
    );
}
