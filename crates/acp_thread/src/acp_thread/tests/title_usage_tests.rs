use super::*;

#[gpui::test]
async fn test_provisional_title_replaced_by_real_title(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let set_title_calls = connection.set_title_calls.clone();

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    // Initial title is the default.
    thread.read_with(cx, |thread, _| {
        assert_eq!(thread.title(), None);
    });

    // Setting a provisional title updates the display title.
    thread.update(cx, |thread, cx| {
        thread.set_provisional_title("Hello, can you help…".into(), cx);
    });
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.title().as_ref().map(|s| s.as_str()),
            Some("Hello, can you help…")
        );
    });

    // The provisional title should NOT have propagated to the connection.
    assert_eq!(
        set_title_calls.borrow().len(),
        0,
        "provisional title should not propagate to the connection"
    );

    // When the real title arrives via set_title, it replaces the
    // provisional title and propagates to the connection.
    let task = thread.update(cx, |thread, cx| {
        thread.set_title("Helping with Rust question".into(), cx)
    });
    task.await.expect("set_title should succeed");
    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.title().as_ref().map(|s| s.as_str()),
            Some("Helping with Rust question")
        );
    });
    assert_eq!(
        set_title_calls.borrow().as_slice(),
        &[SharedString::from("Helping with Rust question")],
        "real title should propagate to the connection"
    );
}

#[gpui::test]
async fn test_session_info_update_replaces_provisional_title_and_emits_event(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());

    let thread = cx
        .update(|cx| {
            connection
                .clone()
                .new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    let title_updated_events = Rc::new(RefCell::new(0usize));
    let title_updated_events_for_subscription = title_updated_events.clone();
    thread.update(cx, |_thread, cx| {
        cx.subscribe(
            &thread,
            move |_thread, _event_thread, event: &AcpThreadEvent, _cx| {
                if matches!(event, AcpThreadEvent::TitleUpdated) {
                    *title_updated_events_for_subscription.borrow_mut() += 1;
                }
            },
        )
        .detach();
    });

    thread.update(cx, |thread, cx| {
        thread.set_provisional_title("Hello, can you help…".into(), cx);
    });
    assert_eq!(
        *title_updated_events.borrow(),
        1,
        "setting a provisional title should emit TitleUpdated"
    );

    let result = thread.update(cx, |thread, cx| {
        thread.handle_session_update(
            acp::SessionUpdate::SessionInfoUpdate(
                acp::SessionInfoUpdate::new().title("Helping with Rust question"),
            ),
            cx,
        )
    });
    result.expect("session info update should succeed");

    thread.read_with(cx, |thread, _| {
        assert_eq!(
            thread.title().as_ref().map(|s| s.as_str()),
            Some("Helping with Rust question")
        );
        assert!(
            !thread.has_provisional_title(),
            "session info title update should clear provisional title"
        );
    });

    assert_eq!(
        *title_updated_events.borrow(),
        2,
        "session info title update should emit TitleUpdated"
    );
    assert!(
        connection.set_title_calls.borrow().is_empty(),
        "session info title update should not propagate back to the connection"
    );
}

#[gpui::test]
async fn test_usage_update_populates_token_usage_and_cost(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::UsageUpdate(
                    acp::UsageUpdate::new(5000, 10000).cost(acp::Cost::new(0.42, "USD")),
                ),
                cx,
            )
            .unwrap();
    });

    thread.read_with(cx, |thread, _| {
        let usage = thread.token_usage().expect("token_usage should be set");
        assert_eq!(usage.max_tokens, 10000);
        assert_eq!(usage.used_tokens, 5000);

        let cost = thread.cost().expect("cost should be set");
        assert!((cost.amount - 0.42).abs() < f64::EPSILON);
        assert_eq!(cost.currency.as_ref(), "USD");
    });
}

#[gpui::test]
async fn test_context_compaction_preserves_token_usage(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::UsageUpdate(
                    acp::UsageUpdate::new(5000, 10000).cost(acp::Cost::new(0.42, "USD")),
                ),
                cx,
            )
            .unwrap();

        thread.push_context_compaction(
            ContextCompaction {
                id: ContextCompactionId("compaction-1".into()),
                status: ContextCompactionStatus::InProgress,
                summary: None,
            },
            cx,
        );
    });

    thread.read_with(cx, |thread, _| {
        let usage = thread
            .token_usage()
            .expect("context compaction should not clear token usage on its own");
        assert_eq!(usage.used_tokens, 5000);
        assert_eq!(usage.max_tokens, 10000);

        let cost = thread
            .cost()
            .expect("context compaction should not clear cost on its own");
        assert!((cost.amount - 0.42).abs() < f64::EPSILON);
    });

    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::UsageUpdate(acp::UsageUpdate::new(1000, 10000)),
                cx,
            )
            .unwrap();
    });

    thread.read_with(cx, |thread, _| {
        let usage = thread
            .token_usage()
            .expect("token_usage should be restored by the next usage update");
        assert_eq!(usage.used_tokens, 1000);
        assert_eq!(usage.max_tokens, 10000);
    });
}

#[gpui::test]
async fn test_usage_update_without_cost_preserves_existing_cost(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::UsageUpdate(
                    acp::UsageUpdate::new(1000, 10000).cost(acp::Cost::new(0.10, "USD")),
                ),
                cx,
            )
            .unwrap();

        thread
            .handle_session_update(
                acp::SessionUpdate::UsageUpdate(acp::UsageUpdate::new(2000, 10000)),
                cx,
            )
            .unwrap();
    });

    thread.read_with(cx, |thread, _| {
        let usage = thread.token_usage().expect("token_usage should be set");
        assert_eq!(usage.used_tokens, 2000);

        let cost = thread.cost().expect("cost should be preserved");
        assert!((cost.amount - 0.10).abs() < f64::EPSILON);
    });
}

#[gpui::test]
async fn test_response_usage_does_not_clobber_session_usage(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new().on_user_message(
        move |_, thread, mut cx| {
            async move {
                thread.update(&mut cx, |thread, cx| {
                    thread
                        .handle_session_update(
                            acp::SessionUpdate::UsageUpdate(
                                acp::UsageUpdate::new(3000, 10000)
                                    .cost(acp::Cost::new(0.05, "EUR")),
                            ),
                            cx,
                        )
                        .unwrap();
                })?;
                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)
                    .usage(acp::Usage::new(500, 200, 300)))
            }
            .boxed_local()
        },
    ));

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread
        .update(cx, |thread, cx| thread.send_raw("hello", cx))
        .await
        .unwrap();

    thread.read_with(cx, |thread, _| {
        let usage = thread.token_usage().expect("token_usage should be set");
        assert_eq!(usage.max_tokens, 10000, "max_tokens from UsageUpdate");
        assert_eq!(usage.used_tokens, 3000, "used_tokens from UsageUpdate");
        assert_eq!(usage.input_tokens, 200, "input_tokens from response usage");
        assert_eq!(
            usage.output_tokens, 300,
            "output_tokens from response usage"
        );

        let cost = thread.cost().expect("cost should be set");
        assert!((cost.amount - 0.05).abs() < f64::EPSILON);
        assert_eq!(cost.currency.as_ref(), "EUR");
    });
}

#[gpui::test]
async fn test_clearing_token_usage_also_clears_cost(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection = Rc::new(FakeAgentConnection::new());
    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::UsageUpdate(
                    acp::UsageUpdate::new(1000, 10000).cost(acp::Cost::new(0.25, "USD")),
                ),
                cx,
            )
            .unwrap();

        assert!(thread.token_usage().is_some());
        assert!(thread.cost().is_some());

        thread.update_token_usage(None, cx);

        assert!(thread.token_usage().is_none());
        assert!(
            thread.cost().is_none(),
            "cost should be cleared when token usage is cleared"
        );
    });
}

/// Regression test: if the inner send_task is cancelled before it can
/// fire `tx.send(...)` (e.g. because the underlying future was dropped),
/// the outer task observes `rx.await` returning `Err(Cancelled)` and
/// must still clear `running_turn` so the panel transitions out of
/// `Generating`. Without this, the agent thread is wedged in the
/// loading state until Mav restarts.
#[gpui::test]
async fn test_running_turn_cleared_when_send_task_dropped(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    // Handler hangs forever so the spawn at run_turn is parked inside
    // `f(this, cx).await` with `tx` still alive but unsent.
    let connection = Rc::new(FakeAgentConnection::new().on_user_message(
        |_params, _thread, _cx| {
            async move { futures::future::pending::<Result<acp::PromptResponse>>().await }
                .boxed_local()
        },
    ));

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    let request = thread.update(cx, |thread, cx| thread.send_raw("hello", cx));
    cx.run_until_parked();

    assert_eq!(
        thread.read_with(cx, |t, _| t.status()),
        ThreadStatus::Generating,
        "thread should be generating while the handler is parked"
    );

    // Replace the in-flight send_task with a no-op. Dropping the original
    // Task cancels its inner future, which drops `tx` without ever calling
    // `tx.send(...)`. This mirrors the production scenario where the
    // send_task future is cancelled before completion.
    thread.update(cx, |thread, _| {
        thread.running_turn.as_mut().unwrap().send_task = Task::ready(());
    });

    let result = request.await;
    assert!(
        matches!(result, Ok(None)),
        "outer task should resolve to Ok(None) on dropped tx, got {result:?}"
    );

    assert_eq!(
        thread.read_with(cx, |t, _| t.status()),
        ThreadStatus::Idle,
        "running_turn must be cleared even when tx was dropped without send"
    );
}
