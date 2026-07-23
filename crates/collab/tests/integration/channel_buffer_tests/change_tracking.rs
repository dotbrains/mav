use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_channel_buffer_changes(
    deterministic: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let (server, client_a, client_b, channel_id) = TestServer::start2(cx_a, cx_b).await;
    let (_, cx_a) = client_a.build_test_workspace(cx_a).await;
    let (workspace_b, cx_b) = client_b.build_test_workspace(cx_b).await;
    let channel_store_b = client_b.channel_store().clone();

    // Editing the channel notes should set them to dirty
    open_channel_notes(channel_id, cx_a).await.unwrap();
    cx_a.simulate_keystrokes("1");
    channel_store_b.read_with(cx_b, |channel_store, _| {
        assert!(channel_store.has_channel_buffer_changed(channel_id))
    });

    // Opening the buffer should clear the changed flag.
    open_channel_notes(channel_id, cx_b).await.unwrap();
    channel_store_b.read_with(cx_b, |channel_store, _| {
        assert!(!channel_store.has_channel_buffer_changed(channel_id))
    });

    // Editing the channel while the buffer is open should not show that the buffer has changed.
    cx_a.simulate_keystrokes("2");
    channel_store_b.read_with(cx_b, |channel_store, _| {
        assert!(!channel_store.has_channel_buffer_changed(channel_id))
    });

    // Test that the server is tracking things correctly, and we retain our 'not changed'
    // state across a disconnect
    deterministic.advance_clock(ACKNOWLEDGE_DEBOUNCE_INTERVAL);
    server
        .simulate_long_connection_interruption(client_b.peer_id().unwrap(), deterministic.clone());

    // Re-subscribe to channels after reconnection (simulates collab panel re-rendering)
    client_b.initialize_channel_store(cx_b);
    deterministic.run_until_parked();

    channel_store_b.read_with(cx_b, |channel_store, _| {
        assert!(!channel_store.has_channel_buffer_changed(channel_id))
    });

    // Closing the buffer should re-enable change tracking
    cx_b.update(|window, cx| {
        workspace_b.update(cx, |workspace, cx| {
            workspace.close_all_items_and_panes(&Default::default(), window, cx)
        });
    });
    deterministic.run_until_parked();

    cx_a.simulate_keystrokes("3");
    channel_store_b.read_with(cx_b, |channel_store, _| {
        assert!(channel_store.has_channel_buffer_changed(channel_id))
    });
}

#[gpui::test]
async fn test_channel_buffer_changes_persist(
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
    cx_b2: &mut TestAppContext,
) {
    let (mut server, client_a, client_b, channel_id) = TestServer::start2(cx_a, cx_b).await;
    let (_, cx_a) = client_a.build_test_workspace(cx_a).await;
    let (_, cx_b) = client_b.build_test_workspace(cx_b).await;

    // a) edits the notes
    open_channel_notes(channel_id, cx_a).await.unwrap();
    cx_a.simulate_keystrokes("1");
    // b) opens them to observe the current version
    open_channel_notes(channel_id, cx_b).await.unwrap();

    // On boot the client should get the correct state.
    let client_b2 = server.create_client(cx_b2, "user_b").await;
    let channel_store_b2 = client_b2.channel_store().clone();
    channel_store_b2.read_with(cx_b2, |channel_store, _| {
        assert!(!channel_store.has_channel_buffer_changed(channel_id))
    });
}
