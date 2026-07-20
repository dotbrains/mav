use super::*;

#[gpui::test]
async fn test_running_thread_retained_when_navigating_away(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;

    let connection_a = StubAgentConnection::new();
    open_thread_with_connection(&panel, connection_a.clone(), &mut cx);
    send_message(&panel, &mut cx);

    let session_id_a = active_session_id(&panel, &cx);
    let thread_id_a = active_thread_id(&panel, &cx);

    cx.update(|_, cx| {
        connection_a.send_update(
            session_id_a.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("chunk".into())),
            cx,
        );
    });
    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let thread = panel.active_agent_thread(cx).unwrap();
        assert_eq!(thread.read(cx).status(), ThreadStatus::Generating);
        assert!(panel.retained_threads.is_empty());
    });

    let connection_b = StubAgentConnection::new();
    open_thread_with_connection(&panel, connection_b, &mut cx);

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(
            panel.retained_threads.len(),
            1,
            "Running thread A should be retained in retained_threads"
        );
        assert!(
            panel.retained_threads.contains_key(&thread_id_a),
            "Retained thread should be keyed by thread A's thread ID"
        );
    });
}

#[gpui::test]
async fn test_idle_non_loadable_thread_retained_when_navigating_away(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;

    let connection_a = StubAgentConnection::new();
    connection_a.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
        acp::ContentChunk::new("Response".into()),
    )]);
    open_thread_with_connection(&panel, connection_a, &mut cx);
    send_message(&panel, &mut cx);

    let weak_view_a = panel.read_with(&cx, |panel, _cx| {
        panel.active_conversation_view().unwrap().downgrade()
    });
    let thread_id_a = active_thread_id(&panel, &cx);

    panel.read_with(&cx, |panel, cx| {
        let thread = panel.active_agent_thread(cx).unwrap();
        assert_eq!(thread.read(cx).status(), ThreadStatus::Idle);
    });

    let connection_b = StubAgentConnection::new();
    open_thread_with_connection(&panel, connection_b, &mut cx);

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(
            panel.retained_threads.len(),
            1,
            "Idle non-loadable thread A should be retained in retained_threads"
        );
        assert!(
            panel.retained_threads.contains_key(&thread_id_a),
            "Retained thread should be keyed by thread A's thread ID"
        );
    });

    assert!(
        weak_view_a.upgrade().is_some(),
        "Idle non-loadable ConnectionView should still be retained"
    );
}

#[gpui::test]
async fn test_background_thread_promoted_via_load(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;

    let connection_a = StubAgentConnection::new();
    open_thread_with_connection(&panel, connection_a.clone(), &mut cx);
    send_message(&panel, &mut cx);

    let session_id_a = active_session_id(&panel, &cx);
    let thread_id_a = active_thread_id(&panel, &cx);

    cx.update(|_, cx| {
        connection_a.send_update(
            session_id_a.clone(),
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new("chunk".into())),
            cx,
        );
    });
    cx.run_until_parked();

    let connection_b = StubAgentConnection::new();
    open_thread_with_connection(&panel, connection_b, &mut cx);
    send_message(&panel, &mut cx);

    let thread_id_b = active_thread_id(&panel, &cx);

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(panel.retained_threads.len(), 1);
        assert!(panel.retained_threads.contains_key(&thread_id_a));
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.load_agent_thread(
            panel.selected_agent(cx),
            thread_id_a,
            None,
            None,
            true,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });

    let active_session = active_session_id(&panel, &cx);
    assert_eq!(
        active_session, session_id_a,
        "Thread A should be the active thread after promotion"
    );

    panel.read_with(&cx, |panel, _cx| {
        assert!(
            !panel.retained_threads.contains_key(&thread_id_a),
            "Promoted thread A should no longer be in retained_threads"
        );
        assert!(
            panel.retained_threads.contains_key(&thread_id_b),
            "Thread B (idle, non-loadable) should remain retained in retained_threads"
        );
    });
}

#[gpui::test]
async fn test_reopening_visible_thread_keeps_thread_usable(cx: &mut TestAppContext) {
    let (panel, mut cx) = setup_panel(cx).await;
    cx.run_until_parked();

    panel.update(&mut cx, |panel, cx| {
        panel.connection_store.update(cx, |store, cx| {
            store.restart_connection(
                Agent::NativeAgent,
                Rc::new(StubAgentServer::new(SessionTrackingConnection::new())),
                cx,
            );
        });
    });
    cx.run_until_parked();

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.external_thread(
            Some(Agent::NativeAgent),
            None,
            None,
            None,
            None,
            true,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    send_message(&panel, &mut cx);

    let session_id = active_session_id(&panel, &cx);

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.open_thread(session_id.clone(), None, None, window, cx);
    });
    cx.run_until_parked();

    send_message(&panel, &mut cx);

    panel.read_with(&cx, |panel, cx| {
        let active_view = panel
            .active_conversation_view()
            .expect("visible conversation should remain open after reopening");
        let connected = active_view
            .read(cx)
            .as_connected()
            .expect("visible conversation should still be connected in the UI");
        assert!(
            !connected.has_thread_error(cx),
            "reopening an already-visible session should keep the thread usable"
        );
    });
}
