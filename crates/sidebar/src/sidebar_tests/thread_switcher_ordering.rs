use super::*;

fn thread_id_for(session_id: &acp::SessionId, cx: &mut TestAppContext) -> ThreadId {
    cx.read(|cx| {
        ThreadMetadataStore::global(cx)
            .read(cx)
            .entry_by_session(session_id)
            .map(|m| m.thread_id)
            .expect("thread metadata should exist")
    })
}

#[gpui::test]
async fn test_thread_switcher_ordering(cx: &mut TestAppContext) {
    let project = init_test_project_with_agent_panel("/my-project", cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let (sidebar, panel) = setup_sidebar_with_agent_panel(&multi_workspace, cx);

    let switcher_ids =
        |sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext| -> Vec<ThreadId> {
            sidebar.read_with(cx, |sidebar, cx| {
                let switcher = sidebar
                    .thread_switcher
                    .as_ref()
                    .expect("switcher should be open");
                switcher
                    .read(cx)
                    .entries()
                    .iter()
                    .map(|entry| entry.thread_id().expect("expected thread switcher entry"))
                    .collect()
            })
        };

    let switcher_selected_id =
        |sidebar: &Entity<Sidebar>, cx: &mut gpui::VisualTestContext| -> ThreadId {
            sidebar.read_with(cx, |sidebar, cx| {
                let switcher = sidebar
                    .thread_switcher
                    .as_ref()
                    .expect("switcher should be open");
                let s = switcher.read(cx);
                s.selected_entry()
                    .expect("should have selection")
                    .thread_id()
                    .expect("expected selected thread entry")
            })
        };

    // ── Setup: create three threads with distinct created_at times ──────
    // Thread C (oldest), Thread B, Thread A (newest) — by created_at.
    // We send messages in each so they also get last_message_sent_or_queued timestamps.
    let connection_c = StubAgentConnection::new();
    connection_c.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done C".into()),
    )]);
    open_thread_with_connection(&panel, connection_c, cx);
    send_message(&panel, cx);
    let session_id_c = active_session_id(&panel, cx);
    let thread_id_c = active_thread_id(&panel, cx);
    save_thread_metadata(
        session_id_c.clone(),
        Some("Thread C".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap(),
        Some(chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 1, 0, 0, 0).unwrap()),
        None,
        &project,
        cx,
    );

    let connection_b = StubAgentConnection::new();
    connection_b.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done B".into()),
    )]);
    open_thread_with_connection(&panel, connection_b, cx);
    send_message(&panel, cx);
    let session_id_b = active_session_id(&panel, cx);
    let thread_id_b = active_thread_id(&panel, cx);
    save_thread_metadata(
        session_id_b.clone(),
        Some("Thread B".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap(),
        Some(chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 2, 0, 0, 0).unwrap()),
        None,
        &project,
        cx,
    );

    let connection_a = StubAgentConnection::new();
    connection_a.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Done A".into()),
    )]);
    open_thread_with_connection(&panel, connection_a, cx);
    send_message(&panel, cx);
    let session_id_a = active_session_id(&panel, cx);
    let thread_id_a = active_thread_id(&panel, cx);
    save_thread_metadata(
        session_id_a.clone(),
        Some("Thread A".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap(),
        Some(chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 3, 0, 0, 0).unwrap()),
        None,
        &project,
        cx,
    );

    // All three threads are now live. Thread A was opened last, so it's
    // the one being viewed. Opening each thread called record_thread_access,
    // so all three have last_accessed_at set.
    // Access order is: A (most recent), B, C (oldest).

    // ── 1. Open switcher: threads sorted by last_accessed_at ─────────────────
    focus_sidebar(&sidebar, cx);
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    // All three have last_accessed_at, so they sort by access time.
    // A was accessed most recently (it's the currently viewed thread),
    // then B, then C.
    assert_eq!(
        switcher_ids(&sidebar, cx),
        vec![thread_id_a, thread_id_b, thread_id_c,],
    );
    // First ctrl-tab selects the second entry (B).
    assert_eq!(switcher_selected_id(&sidebar, cx), thread_id_b);

    // Dismiss the switcher without confirming.
    sidebar.update_in(cx, |sidebar, _window, cx| {
        sidebar.dismiss_thread_switcher(cx);
    });
    cx.run_until_parked();

    // ── 2. Confirm on Thread C: it becomes most-recently-accessed ──────
    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    // Cycle twice to land on Thread C (index 2).
    sidebar.read_with(cx, |sidebar, cx| {
        let switcher = sidebar.thread_switcher.as_ref().unwrap();
        assert_eq!(switcher.read(cx).selected_index(), 1);
    });
    sidebar.update_in(cx, |sidebar, _window, cx| {
        sidebar
            .thread_switcher
            .as_ref()
            .unwrap()
            .update(cx, |s, cx| s.cycle_selection(cx));
    });
    cx.run_until_parked();
    assert_eq!(switcher_selected_id(&sidebar, cx), thread_id_c);

    assert!(sidebar.update(cx, |sidebar, _cx| sidebar.thread_last_accessed.is_empty()));

    // Confirm on Thread C.
    sidebar.update_in(cx, |sidebar, window, cx| {
        let switcher = sidebar.thread_switcher.as_ref().unwrap();
        let focus = switcher.focus_handle(cx);
        focus.dispatch_action(&menu::Confirm, window, cx);
    });
    cx.run_until_parked();

    // Switcher should be dismissed after confirm.
    sidebar.read_with(cx, |sidebar, _cx| {
        assert!(
            sidebar.thread_switcher.is_none(),
            "switcher should be dismissed"
        );
    });

    sidebar.update(cx, |sidebar, _cx| {
        let last_accessed = sidebar
            .thread_last_accessed
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(last_accessed.len(), 1);
        assert!(last_accessed.contains(&thread_id_c));
        assert!(
            is_active_session(&sidebar, &session_id_c),
            "active_entry should be Thread({session_id_c:?})"
        );
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        switcher_ids(&sidebar, cx),
        vec![thread_id_c, thread_id_a, thread_id_b],
    );

    // Confirm on Thread A.
    sidebar.update_in(cx, |sidebar, window, cx| {
        let switcher = sidebar.thread_switcher.as_ref().unwrap();
        let focus = switcher.focus_handle(cx);
        focus.dispatch_action(&menu::Confirm, window, cx);
    });
    cx.run_until_parked();

    sidebar.update(cx, |sidebar, _cx| {
        let last_accessed = sidebar
            .thread_last_accessed
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(last_accessed.len(), 2);
        assert!(last_accessed.contains(&thread_id_c));
        assert!(last_accessed.contains(&thread_id_a));
        assert!(
            is_active_session(&sidebar, &session_id_a),
            "active_entry should be Thread({session_id_a:?})"
        );
    });

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        switcher_ids(&sidebar, cx),
        vec![thread_id_a, thread_id_c, thread_id_b,],
    );

    sidebar.update_in(cx, |sidebar, _window, cx| {
        let switcher = sidebar.thread_switcher.as_ref().unwrap();
        switcher.update(cx, |switcher, cx| switcher.cycle_selection(cx));
    });
    cx.run_until_parked();

    // Confirm on Thread B.
    sidebar.update_in(cx, |sidebar, window, cx| {
        let switcher = sidebar.thread_switcher.as_ref().unwrap();
        let focus = switcher.focus_handle(cx);
        focus.dispatch_action(&menu::Confirm, window, cx);
    });
    cx.run_until_parked();

    sidebar.update(cx, |sidebar, _cx| {
        let last_accessed = sidebar
            .thread_last_accessed
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(last_accessed.len(), 3);
        assert!(last_accessed.contains(&thread_id_c));
        assert!(last_accessed.contains(&thread_id_a));
        assert!(last_accessed.contains(&thread_id_b));
        assert!(
            is_active_session(&sidebar, &session_id_b),
            "active_entry should be Thread({session_id_b:?})"
        );
    });

    // ── 3. Add a historical thread (no last_accessed_at, no message sent) ──
    // This thread was never opened in a panel — it only exists in metadata.
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-historical")),
        Some("Historical Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 6, 1, 0, 0, 0).unwrap(),
        Some(chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 6, 1, 0, 0, 0).unwrap()),
        None,
        &project,
        cx,
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    // Historical Thread has no last_accessed_at and no last_message_sent_or_queued,
    // so it falls to tier 3 (sorted by created_at). It should appear after all
    // accessed threads, even though its created_at (June 2024) is much later
    // than the others.
    //
    // But the live threads (A, B, C) each had send_message called which sets
    // last_message_sent_or_queued. So for the accessed threads (tier 1) the
    // sort key is last_accessed_at; for Historical Thread (tier 3) it's created_at.
    let session_id_hist = acp::SessionId::new(Arc::from("thread-historical"));
    let thread_id_hist = thread_id_for(&session_id_hist, cx);

    let ids = switcher_ids(&sidebar, cx);
    assert_eq!(
        ids,
        vec![thread_id_b, thread_id_a, thread_id_c, thread_id_hist],
    );

    sidebar.update_in(cx, |sidebar, _window, cx| {
        sidebar.dismiss_thread_switcher(cx);
    });
    cx.run_until_parked();

    // ── 4. Add another historical thread with older created_at ─────────
    save_thread_metadata(
        acp::SessionId::new(Arc::from("thread-old-historical")),
        Some("Old Historical Thread".into()),
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2023, 6, 1, 0, 0, 0).unwrap(),
        Some(chrono::TimeZone::with_ymd_and_hms(&Utc, 2023, 6, 1, 0, 0, 0).unwrap()),
        None,
        &project,
        cx,
    );

    sidebar.update_in(cx, |sidebar, window, cx| {
        sidebar.on_toggle_thread_switcher(&ToggleThreadSwitcher::default(), window, cx);
    });
    cx.run_until_parked();

    // Both historical threads have no access or message times. They should
    // appear after accessed threads, sorted by created_at (newest first).
    let session_id_old_hist = acp::SessionId::new(Arc::from("thread-old-historical"));
    let thread_id_old_hist = thread_id_for(&session_id_old_hist, cx);
    let ids = switcher_ids(&sidebar, cx);
    assert_eq!(
        ids,
        vec![
            thread_id_b,
            thread_id_a,
            thread_id_c,
            thread_id_hist,
            thread_id_old_hist,
        ],
    );

    sidebar.update_in(cx, |sidebar, _window, cx| {
        sidebar.dismiss_thread_switcher(cx);
    });
    cx.run_until_parked();
}
