use super::loaded_harness::connect_fake_agent;
use super::*;

#[gpui::test]
async fn test_loaded_sessions_keep_state_until_last_close(cx: &mut gpui::TestAppContext) {
    let (
        connection,
        project,
        load_count,
        close_count,
        _load_session_updates,
        _load_session_gate,
        _keep_agent_alive,
    ) = connect_fake_agent(cx).await;

    let session_id = acp::SessionId::new("session-1");
    let work_dirs = util::path_list::PathList::new(&[std::path::Path::new("/a")]);

    // Load the same session twice concurrently — the second call should join
    // the pending task rather than issuing a second ACP load_session RPC.
    let first_load = cx.update(|cx| {
        connection.clone().load_session(
            session_id.clone(),
            project.clone(),
            work_dirs.clone(),
            None,
            cx,
        )
    });
    let second_load = cx.update(|cx| {
        connection.clone().load_session(
            session_id.clone(),
            project.clone(),
            work_dirs.clone(),
            None,
            cx,
        )
    });

    let first_thread = first_load.await.expect("first load failed");
    let second_thread = second_load.await.expect("second load failed");
    cx.run_until_parked();

    assert_eq!(
        first_thread.entity_id(),
        second_thread.entity_id(),
        "concurrent loads for the same session should share one AcpThread"
    );
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        1,
        "underlying ACP load_session should be called exactly once for concurrent loads"
    );

    // The session has ref_count 2. The first close should not send the ACP
    // close_session RPC — the session is still referenced.
    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .expect("first close failed");

    assert_eq!(
        close_count.load(Ordering::SeqCst),
        0,
        "ACP close_session should not be sent while ref_count > 0"
    );
    assert!(
        connection.sessions.borrow().contains_key(&session_id),
        "session should still be tracked after first close"
    );

    // The second close drops ref_count to 0 — now the ACP RPC must be sent.
    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .expect("second close failed");
    cx.run_until_parked();

    assert_eq!(
        close_count.load(Ordering::SeqCst),
        1,
        "ACP close_session should be sent exactly once when ref_count reaches 0"
    );
    assert!(
        !connection.sessions.borrow().contains_key(&session_id),
        "session should be removed after final close"
    );
}

// Regression test: per the ACP spec, an agent replays the entire conversation
// history as `session/update` notifications *before* responding to the
// `session/load` request. These notifications must be applied to the
// reconstructed thread, not dropped because the session hasn't been
// registered yet.
#[gpui::test]
async fn test_load_session_replays_notifications_sent_before_response(
    cx: &mut gpui::TestAppContext,
) {
    let (
        connection,
        project,
        _load_count,
        _close_count,
        load_session_updates,
        _load_session_gate,
        _keep_agent_alive,
    ) = connect_fake_agent(cx).await;

    // Queue up some history updates that the fake agent will stream to
    // the client during the `load_session` call, before responding.
    *load_session_updates
        .lock()
        .expect("load_session_updates mutex poisoned") = vec![
        acp::SessionUpdate::UserMessageChunk(acp::ContentChunk::new(acp::ContentBlock::Text(
            acp::TextContent::new(String::from("hello agent")),
        ))),
        acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(acp::ContentBlock::Text(
            acp::TextContent::new(String::from("hi user")),
        ))),
    ];

    let session_id = acp::SessionId::new("session-replay");
    let work_dirs = util::path_list::PathList::new(&[std::path::Path::new("/a")]);

    let thread = cx
        .update(|cx| {
            connection.clone().load_session(
                session_id.clone(),
                project.clone(),
                work_dirs,
                None,
                cx,
            )
        })
        .await
        .expect("load_session failed");
    cx.run_until_parked();

    let entries = thread.read_with(cx, |thread, _| {
        thread
            .entries()
            .iter()
            .map(|entry| match entry {
                acp_thread::AgentThreadEntry::UserMessage(_) => "user",
                acp_thread::AgentThreadEntry::AssistantMessage(_) => "assistant",
                acp_thread::AgentThreadEntry::ToolCall(_) => "tool_call",
                acp_thread::AgentThreadEntry::CompletedPlan(_) => "plan",
                acp_thread::AgentThreadEntry::ContextCompaction(_) => "compaction",
            })
            .collect::<Vec<_>>()
    });

    assert_eq!(
        entries,
        vec!["user", "assistant"],
        "replayed notifications should be applied to the thread"
    );
}

// Regression test: if `close_session` is issued while a `load_session`
// RPC is still in flight, the close must take effect cleanly — the load
// must fail with a recognizable error (not return an orphaned thread),
// no entry must remain in `sessions` or `pending_sessions`, and the ACP
// `close_session` RPC must be dispatched.
#[gpui::test]
async fn test_close_session_during_in_flight_load(cx: &mut gpui::TestAppContext) {
    let (
        connection,
        project,
        load_count,
        close_count,
        _load_session_updates,
        load_session_gate,
        _keep_agent_alive,
    ) = connect_fake_agent(cx).await;

    // Install a gate so the fake agent's `load_session` handler parks
    // before sending its response. We'll close the session while the
    // load is parked.
    let (gate_tx, gate_rx) = async_channel::bounded::<()>(1);
    *load_session_gate
        .lock()
        .expect("load_session_gate mutex poisoned") = Some(gate_rx);

    let session_id = acp::SessionId::new("session-close-during-load");
    let work_dirs = util::path_list::PathList::new(&[std::path::Path::new("/a")]);

    let load_task = cx.update(|cx| {
        connection
            .clone()
            .load_session(session_id.clone(), project.clone(), work_dirs, None, cx)
    });

    // Let the load RPC reach the agent and park on the gate.
    cx.run_until_parked();
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        1,
        "load_session RPC should have been dispatched"
    );
    assert!(
        connection
            .pending_sessions
            .borrow()
            .contains_key(&session_id),
        "pending_sessions entry should exist while load is in flight"
    );
    assert!(
        connection.sessions.borrow().contains_key(&session_id),
        "sessions entry should be pre-registered to receive replay notifications"
    );

    // Close the session while the load is still parked. This should take
    // the pending path and dispatch the ACP close RPC.
    let close_task = cx.update(|cx| connection.clone().close_session(&session_id, cx));

    // Release the gate so the load RPC can finally respond.
    gate_tx.send(()).await.expect("gate send failed");
    drop(gate_tx);

    let load_result = load_task.await;
    close_task.await.expect("close failed");
    cx.run_until_parked();

    let err = load_result.expect_err("load should fail after close-during-load");
    assert!(
        err.to_string()
            .contains("session was closed before load completed"),
        "expected close-during-load error, got: {err}"
    );

    assert_eq!(
        close_count.load(Ordering::SeqCst),
        1,
        "ACP close_session should be sent exactly once"
    );
    assert!(
        !connection.sessions.borrow().contains_key(&session_id),
        "sessions entry should be removed after close-during-load"
    );
    assert!(
        !connection
            .pending_sessions
            .borrow()
            .contains_key(&session_id),
        "pending_sessions entry should be removed after close-during-load"
    );
}

// Regression test: when two concurrent `load_session` calls share a pending
// task and one of them issues `close_session` before the load RPC
// resolves, the remaining load must still succeed and the session must
// stay live. If `close_session` incorrectly short-circuits via the
// `sessions` path (removing the entry while a load is still in flight),
// the pending task will fail and both concurrent loaders will lose
// their handle.
#[gpui::test]
async fn test_close_during_load_preserves_other_concurrent_loader(cx: &mut gpui::TestAppContext) {
    let (
        connection,
        project,
        load_count,
        close_count,
        _load_session_updates,
        load_session_gate,
        _keep_agent_alive,
    ) = connect_fake_agent(cx).await;

    let (gate_tx, gate_rx) = async_channel::bounded::<()>(1);
    *load_session_gate
        .lock()
        .expect("load_session_gate mutex poisoned") = Some(gate_rx);

    let session_id = acp::SessionId::new("session-concurrent-close");
    let work_dirs = util::path_list::PathList::new(&[std::path::Path::new("/a")]);

    // Kick off two concurrent loads; the second must join the first's pending
    // task rather than issuing a second RPC.
    let first_load = cx.update(|cx| {
        connection.clone().load_session(
            session_id.clone(),
            project.clone(),
            work_dirs.clone(),
            None,
            cx,
        )
    });
    let second_load = cx.update(|cx| {
        connection.clone().load_session(
            session_id.clone(),
            project.clone(),
            work_dirs.clone(),
            None,
            cx,
        )
    });

    cx.run_until_parked();
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        1,
        "load_session RPC should only be dispatched once for concurrent loads"
    );

    // Close one of the two handles while the shared load is still parked.
    // Because a second loader still holds a pending ref, this should be a
    // no-op on the wire.
    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .expect("close during load failed");
    assert_eq!(
        close_count.load(Ordering::SeqCst),
        0,
        "close_session RPC must not be dispatched while another load handle remains"
    );

    // Release the gate so the load RPC can finally respond.
    gate_tx.send(()).await.expect("gate send failed");
    drop(gate_tx);

    let first_thread = first_load.await.expect("first load should still succeed");
    let second_thread = second_load.await.expect("second load should still succeed");
    cx.run_until_parked();

    assert_eq!(
        first_thread.entity_id(),
        second_thread.entity_id(),
        "concurrent loads should share one AcpThread"
    );
    assert!(
        connection.sessions.borrow().contains_key(&session_id),
        "session must remain tracked while a load handle is still outstanding"
    );
    assert!(
        !connection
            .pending_sessions
            .borrow()
            .contains_key(&session_id),
        "pending_sessions entry should be cleared once the load resolves"
    );

    // Final close drops ref_count to 0 and dispatches the ACP close RPC.
    cx.update(|cx| connection.clone().close_session(&session_id, cx))
        .await
        .expect("final close failed");
    cx.run_until_parked();
    assert_eq!(
        close_count.load(Ordering::SeqCst),
        1,
        "close_session RPC should fire exactly once when the last handle is released"
    );
    assert!(
        !connection.sessions.borrow().contains_key(&session_id),
        "session should be removed after final close"
    );
}
