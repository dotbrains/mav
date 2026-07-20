use super::*;

#[gpui::test]
async fn test_cleanup_retained_threads_keeps_five_most_recent_idle_loadable_threads(
    cx: &mut TestAppContext,
) {
    let (panel, mut cx) = setup_panel(cx).await;
    let connection = StubAgentConnection::new()
        .with_supports_load_session(true)
        .with_agent_id("loadable-stub".into())
        .with_telemetry_id("loadable-stub".into());
    let mut session_ids = Vec::new();
    let mut thread_ids = Vec::new();

    for _ in 0..7 {
        let (session_id, thread_id) =
            open_generating_thread_with_loadable_connection(&panel, &connection, &mut cx);
        session_ids.push(session_id);
        thread_ids.push(thread_id);
    }

    let base_time = Instant::now();

    for session_id in session_ids.iter().take(6) {
        connection.end_turn(session_id.clone(), acp::StopReason::EndTurn);
    }
    cx.run_until_parked();

    panel.update(&mut cx, |panel, cx| {
        for (index, thread_id) in thread_ids.iter().take(6).enumerate() {
            let conversation_view = panel
                .retained_threads
                .get(thread_id)
                .expect("retained thread should exist")
                .clone();
            conversation_view.update(cx, |view, cx| {
                view.set_updated_at(base_time + Duration::from_secs(index as u64), cx);
            });
        }
        panel.cleanup_retained_threads(cx);
    });

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(
            panel.retained_threads.len(),
            5,
            "cleanup should keep at most five idle loadable retained threads"
        );
        assert!(
            !panel.retained_threads.contains_key(&thread_ids[0]),
            "oldest idle loadable retained thread should be removed"
        );
        for thread_id in &thread_ids[1..6] {
            assert!(
                panel.retained_threads.contains_key(thread_id),
                "more recent idle loadable retained threads should be retained"
            );
        }
        assert!(
            !panel.retained_threads.contains_key(&thread_ids[6]),
            "the active thread should not also be stored as a retained thread"
        );
    });
}

#[gpui::test]
async fn test_cleanup_retained_threads_preserves_idle_non_loadable_threads(
    cx: &mut TestAppContext,
) {
    let (panel, mut cx) = setup_panel(cx).await;

    let non_loadable_connection = StubAgentConnection::new();
    let (_non_loadable_session_id, non_loadable_thread_id) =
        open_idle_thread_with_non_loadable_connection(&panel, &non_loadable_connection, &mut cx);

    let loadable_connection = StubAgentConnection::new()
        .with_supports_load_session(true)
        .with_agent_id("loadable-stub".into())
        .with_telemetry_id("loadable-stub".into());
    let mut loadable_session_ids = Vec::new();
    let mut loadable_thread_ids = Vec::new();

    for _ in 0..7 {
        let (session_id, thread_id) =
            open_generating_thread_with_loadable_connection(&panel, &loadable_connection, &mut cx);
        loadable_session_ids.push(session_id);
        loadable_thread_ids.push(thread_id);
    }

    let base_time = Instant::now();

    for session_id in loadable_session_ids.iter().take(6) {
        loadable_connection.end_turn(session_id.clone(), acp::StopReason::EndTurn);
    }
    cx.run_until_parked();

    panel.update(&mut cx, |panel, cx| {
        for (index, thread_id) in loadable_thread_ids.iter().take(6).enumerate() {
            let conversation_view = panel
                .retained_threads
                .get(thread_id)
                .expect("retained thread should exist")
                .clone();
            conversation_view.update(cx, |view, cx| {
                view.set_updated_at(base_time + Duration::from_secs(index as u64), cx);
            });
        }
        panel.cleanup_retained_threads(cx);
    });

    panel.read_with(&cx, |panel, _cx| {
        assert_eq!(
            panel.retained_threads.len(),
            6,
            "cleanup should keep the non-loadable idle thread in addition to five loadable ones"
        );
        assert!(
            panel.retained_threads.contains_key(&non_loadable_thread_id),
            "idle non-loadable retained threads should not be cleanup candidates"
        );
        assert!(
            !panel.retained_threads.contains_key(&loadable_thread_ids[0]),
            "oldest idle loadable retained thread should still be removed"
        );
        for thread_id in &loadable_thread_ids[1..6] {
            assert!(
                panel.retained_threads.contains_key(thread_id),
                "more recent idle loadable retained threads should be retained"
            );
        }
        assert!(
            !panel.retained_threads.contains_key(&loadable_thread_ids[6]),
            "the active loadable thread should not also be stored as a retained thread"
        );
    });
}
