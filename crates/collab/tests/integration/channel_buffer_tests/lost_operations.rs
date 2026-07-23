use super::test_support::*;
use super::*;

#[gpui::test]
async fn test_channel_buffer_operations_lost_on_reconnect(
    executor: BackgroundExecutor,
    cx_a: &mut TestAppContext,
    cx_b: &mut TestAppContext,
) {
    let mut server = TestServer::start(executor.clone()).await;
    let client_a = server.create_client(cx_a, "user_a").await;
    let client_b = server.create_client(cx_b, "user_b").await;

    let channel_id = server
        .make_channel(
            "the-channel",
            None,
            (&client_a, cx_a),
            &mut [(&client_b, cx_b)],
        )
        .await;

    // Both clients open the channel buffer.
    let channel_buffer_a = client_a
        .channel_store()
        .update(cx_a, |store, cx| store.open_channel_buffer(channel_id, cx))
        .await
        .unwrap();
    let channel_buffer_b = client_b
        .channel_store()
        .update(cx_b, |store, cx| store.open_channel_buffer(channel_id, cx))
        .await
        .unwrap();

    // Step 1: Client A makes an initial edit that syncs to B.
    channel_buffer_a.update(cx_a, |buffer, cx| {
        buffer.buffer().update(cx, |buffer, cx| {
            buffer.edit([(0..0, "a")], None, cx);
        })
    });
    executor.run_until_parked();

    // Verify both clients see "a".
    channel_buffer_a.read_with(cx_a, |buffer, cx| {
        assert_eq!(buffer.buffer().read(cx).text(), "a");
    });
    channel_buffer_b.read_with(cx_b, |buffer, cx| {
        assert_eq!(buffer.buffer().read(cx).text(), "a");
    });

    // Step 2: Disconnect client A. Do NOT advance past RECONNECT_TIMEOUT
    // so that the buffer stays in `opened_buffers` for rejoin.
    server.forbid_connections();
    server.disconnect_client(client_a.peer_id().unwrap());
    executor.run_until_parked();

    // Step 3: While disconnected, client A makes an offline edit ("b").
    // on_buffer_update fires but client.send() fails because transport is down.
    channel_buffer_a.update(cx_a, |buffer, cx| {
        buffer.buffer().update(cx, |buffer, cx| {
            buffer.edit([(1..1, "b")], None, cx);
        })
    });
    executor.run_until_parked();

    // Client A sees "ab" locally; B still sees "a".
    channel_buffer_a.read_with(cx_a, |buffer, cx| {
        assert_eq!(buffer.buffer().read(cx).text(), "ab");
    });
    channel_buffer_b.read_with(cx_b, |buffer, cx| {
        assert_eq!(buffer.buffer().read(cx).text(), "a");
    });

    // Step 4: Reconnect and make a racing edit in parallel.
    //
    // The race condition occurs when:
    // 1. Transport reconnects, handle_connect captures version V (with "b") and sends RejoinChannelBuffers
    // 2. DURING the async gap (awaiting response), user makes edit "c"
    // 3. on_buffer_update sends UpdateChannelBuffer (succeeds because transport is up)
    // 4. Server receives BOTH messages concurrently (FuturesUnordered)
    // 5. If UpdateChannelBuffer commits first, server version is inflated to include "c"
    // 6. RejoinChannelBuffers reads inflated version and sends it back
    // 7. Client's serialize_ops(inflated_version) filters out "b" (offline edit)
    //    because the inflated version's timestamp covers "b"'s timestamp

    // Get the buffer handle for spawning
    let buffer_for_edit = channel_buffer_a.read_with(cx_a, |buffer, _| buffer.buffer());

    // Spawn the edit task - it will wait for executor to run it
    let edit_task = cx_a.spawn({
        let buffer = buffer_for_edit;
        async move |mut cx| {
            let _ = buffer.update(&mut cx, |buffer, cx| {
                buffer.edit([(2..2, "c")], None, cx);
            });
        }
    });

    // Allow connections so reconnect can succeed
    server.allow_connections();

    // Advance clock to trigger reconnection attempt
    executor.advance_clock(RECEIVE_TIMEOUT);

    // Run the edit task - this races with handle_connect
    edit_task.detach();

    // Let everything settle.
    executor.run_until_parked();

    // Step 7: Read final buffer text from both clients.
    let text_a = channel_buffer_a.read_with(cx_a, |buffer, cx| buffer.buffer().read(cx).text());
    let text_b = channel_buffer_b.read_with(cx_b, |buffer, cx| buffer.buffer().read(cx).text());

    // Both clients must see the same text containing all three edits.
    assert_eq!(
        text_a, text_b,
        "Client A and B diverged! A sees {:?}, B sees {:?}. \
         Operations were lost during reconnection.",
        text_a, text_b
    );
    assert!(
        text_a.contains('a'),
        "Initial edit 'a' missing from final text {:?}",
        text_a
    );
    assert!(
        text_a.contains('b'),
        "Offline edit 'b' missing from final text {:?}. \
         This is the reconnection race bug: the offline operation was \
         filtered out by serialize_ops because the server_version was \
         inflated by a racing UpdateChannelBuffer.",
        text_a
    );
    assert!(
        text_a.contains('c'),
        "Racing edit 'c' missing from final text {:?}",
        text_a
    );

    // Step 8: Verify the invariant directly — every operation known to
    // client A must be observed by client B's version. If any operation
    // in A's history is not covered by B's version, it was lost.
    channel_buffer_a.read_with(cx_a, |buf_a, cx_a_inner| {
        let buffer_a = buf_a.buffer().read(cx_a_inner);
        let ops_a = buffer_a.operations();
        channel_buffer_b.read_with(cx_b, |buf_b, cx_b_inner| {
            let buffer_b = buf_b.buffer().read(cx_b_inner);
            let version_b = buffer_b.version();
            for (lamport, _op) in ops_a.iter() {
                assert!(
                    version_b.observed(*lamport),
                    "Operation with lamport timestamp {:?} from client A \
                     is NOT observed by client B's version. This operation \
                     was lost during reconnection.",
                    lamport
                );
            }
        });
    });
}
