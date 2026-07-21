use super::*;

#[gpui::test]
async fn test_duplicate_tool_call_update_preserves_open_permission_request_until_authorized(
    cx: &mut TestAppContext,
) {
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

    let tool_call_id = acp::ToolCallId::new("toolu_01duplicate");
    let allow_option_id = acp::PermissionOptionId::new("allow");
    let permission_task = thread
        .update(cx, |thread, cx| {
            thread.request_tool_call_authorization(
                acp::ToolCall::new(tool_call_id.clone(), "Original title")
                    .kind(acp::ToolKind::Execute)
                    .status(acp::ToolCallStatus::Pending)
                    .content(vec!["original content".into()])
                    .into(),
                PermissionOptions::Flat(vec![acp::PermissionOption::new(
                    allow_option_id.clone(),
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                )]),
                AuthorizationKind::PermissionGrant,
                cx,
            )
        })
        .unwrap();

    thread
        .update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::ToolCall(
                    acp::ToolCall::new(tool_call_id.clone(), "Updated title")
                        .kind(acp::ToolKind::Execute)
                        .status(acp::ToolCallStatus::Pending)
                        .content(vec!["updated content".into()]),
                ),
                cx,
            )
        })
        .unwrap();

    thread.read_with(cx, |thread, cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert_eq!(tool_call.label.read(cx).source(), "Updated title");
        assert!(matches!(
            tool_call.status,
            ToolCallStatus::WaitingForConfirmation { .. }
        ));
        assert_eq!(tool_call.content.len(), 1);
        assert_eq!(tool_call.content[0].to_markdown(cx), "updated content");
    });

    thread
        .update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                    tool_call_id.clone(),
                    acp::ToolCallUpdateFields::new()
                        .status(acp::ToolCallStatus::InProgress)
                        .title("Updated again")
                        .content(vec!["updated again".into()]),
                )),
                cx,
            )
        })
        .unwrap();

    thread.read_with(cx, |thread, cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert_eq!(tool_call.label.read(cx).source(), "Updated again");
        assert!(matches!(
            tool_call.status,
            ToolCallStatus::WaitingForConfirmation { .. }
        ));
        assert_eq!(tool_call.content.len(), 1);
        assert_eq!(tool_call.content[0].to_markdown(cx), "updated again");
    });

    let selected_outcome = SelectedPermissionOutcome::new(
        allow_option_id.clone(),
        acp::PermissionOptionKind::AllowOnce,
    );
    thread.update(cx, |thread, cx| {
        thread.authorize_tool_call(tool_call_id.clone(), selected_outcome, cx);
    });

    thread.read_with(cx, |thread, _cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert!(matches!(tool_call.status, ToolCallStatus::InProgress));
    });

    match permission_task.await {
        RequestPermissionOutcome::Selected(outcome) => {
            assert_eq!(outcome.option_id, allow_option_id);
            assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
        }
        RequestPermissionOutcome::Cancelled => {
            panic!("permission request should remain open after duplicate tool call update")
        }
    }

    thread
        .update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                    tool_call_id.clone(),
                    acp::ToolCallUpdateFields::new()
                        .status(acp::ToolCallStatus::Completed)
                        .title("Completed")
                        .content(vec!["done".into()]),
                )),
                cx,
            )
        })
        .unwrap();

    thread.read_with(cx, |thread, cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert_eq!(tool_call.label.read(cx).source(), "Completed");
        assert!(matches!(tool_call.status, ToolCallStatus::Completed));
        assert_eq!(tool_call.content.len(), 1);
        assert_eq!(tool_call.content[0].to_markdown(cx), "done");
    });
}

#[gpui::test]
async fn test_permission_request_tracks_agent_status_until_resolved(cx: &mut TestAppContext) {
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

    let tool_call_id = acp::ToolCallId::new("toolu_01auto_resolve");
    let permission_task = thread
        .update(cx, |thread, cx| {
            thread.request_tool_call_authorization(
                acp::ToolCall::new(tool_call_id.clone(), "Original title")
                    .kind(acp::ToolKind::Execute)
                    .status(acp::ToolCallStatus::Pending)
                    .into(),
                PermissionOptions::Flat(vec![acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                )]),
                AuthorizationKind::PermissionGrant,
                cx,
            )
        })
        .unwrap();

    thread
        .update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                    tool_call_id.clone(),
                    acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::InProgress),
                )),
                cx,
            )
        })
        .unwrap();

    thread.read_with(cx, |thread, _cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert!(matches!(
            tool_call.status,
            ToolCallStatus::WaitingForConfirmation {
                current_status: acp::ToolCallStatus::InProgress,
                ..
            }
        ));
    });

    thread.update(cx, |thread, cx| {
        thread.authorize_tool_call(
            tool_call_id.clone(),
            SelectedPermissionOutcome::new(
                acp::PermissionOptionId::new("allow"),
                acp::PermissionOptionKind::AllowOnce,
            ),
            cx,
        );
    });

    thread.read_with(cx, |thread, _cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert!(matches!(tool_call.status, ToolCallStatus::InProgress));
    });

    match permission_task.await {
        RequestPermissionOutcome::Selected(outcome) => {
            assert_eq!(outcome.option_id, acp::PermissionOptionId::new("allow"));
            assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
        }
        RequestPermissionOutcome::Cancelled => {
            panic!("resolved permission request should select an outcome")
        }
    }
}

#[gpui::test]
async fn test_permission_request_sets_waiting_status_on_existing_tool_call(
    cx: &mut TestAppContext,
) {
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

    let tool_call_id = acp::ToolCallId::new("toolu_01existing_permission");
    thread
        .update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::ToolCall(
                    acp::ToolCall::new(tool_call_id.clone(), "Running title")
                        .kind(acp::ToolKind::Execute)
                        .status(acp::ToolCallStatus::InProgress),
                ),
                cx,
            )
        })
        .unwrap();

    let permission_task = thread
        .update(cx, |thread, cx| {
            thread.request_tool_call_authorization(
                acp::ToolCall::new(tool_call_id.clone(), "Needs permission")
                    .kind(acp::ToolKind::Execute)
                    .status(acp::ToolCallStatus::Pending)
                    .into(),
                PermissionOptions::Flat(vec![acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                )]),
                AuthorizationKind::PermissionGrant,
                cx,
            )
        })
        .unwrap();

    thread.read_with(cx, |thread, cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert_eq!(tool_call.label.read(cx).source(), "Needs permission");
        assert!(matches!(
            tool_call.status,
            ToolCallStatus::WaitingForConfirmation {
                current_status: acp::ToolCallStatus::InProgress,
                ..
            }
        ));
    });

    thread.update(cx, |thread, cx| {
        thread.authorize_tool_call(
            tool_call_id.clone(),
            SelectedPermissionOutcome::new(
                acp::PermissionOptionId::new("allow"),
                acp::PermissionOptionKind::AllowOnce,
            ),
            cx,
        );
    });

    match permission_task.await {
        RequestPermissionOutcome::Selected(outcome) => {
            assert_eq!(outcome.option_id, acp::PermissionOptionId::new("allow"));
            assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
        }
        RequestPermissionOutcome::Cancelled => {
            panic!("permission request should resolve after authorization")
        }
    }
}

#[gpui::test]
async fn test_cancel_tool_call_authorization_resolves_permission_request(cx: &mut TestAppContext) {
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

    let tool_call_id = acp::ToolCallId::new("toolu_01cancelled_permission");
    let permission_task = thread
        .update(cx, |thread, cx| {
            thread.request_tool_call_authorization(
                acp::ToolCall::new(tool_call_id.clone(), "Needs permission")
                    .kind(acp::ToolKind::Execute)
                    .status(acp::ToolCallStatus::Pending)
                    .into(),
                PermissionOptions::Flat(vec![acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                )]),
                AuthorizationKind::PermissionGrant,
                cx,
            )
        })
        .unwrap();

    thread.update(cx, |thread, cx| {
        thread.cancel_tool_call_authorization(&tool_call_id, cx);
    });

    thread.read_with(cx, |thread, _cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert!(matches!(tool_call.status, ToolCallStatus::Canceled));
    });

    match permission_task.await {
        RequestPermissionOutcome::Cancelled => {}
        RequestPermissionOutcome::Selected(_) => {
            panic!("cancelled permission request should not select an outcome")
        }
    }
}

#[gpui::test]
async fn test_terminal_tool_call_update_closes_open_permission_request(cx: &mut TestAppContext) {
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

    let tool_call_id = acp::ToolCallId::new("toolu_01completed_while_waiting");
    let permission_task = thread
        .update(cx, |thread, cx| {
            thread.request_tool_call_authorization(
                acp::ToolCall::new(tool_call_id.clone(), "Needs permission")
                    .kind(acp::ToolKind::Execute)
                    .status(acp::ToolCallStatus::Pending)
                    .into(),
                PermissionOptions::Flat(vec![acp::PermissionOption::new(
                    acp::PermissionOptionId::new("allow"),
                    "Allow",
                    acp::PermissionOptionKind::AllowOnce,
                )]),
                AuthorizationKind::PermissionGrant,
                cx,
            )
        })
        .unwrap();

    thread
        .update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                    tool_call_id.clone(),
                    acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::Completed),
                )),
                cx,
            )
        })
        .unwrap();

    thread.read_with(cx, |thread, _cx| {
        let (_, tool_call) = thread
            .tool_call(&tool_call_id)
            .expect("tool call should exist");
        assert!(matches!(tool_call.status, ToolCallStatus::Completed));
    });

    match permission_task.await {
        RequestPermissionOutcome::Cancelled => {}
        RequestPermissionOutcome::Selected(_) => {
            panic!("terminal tool call update should close pending permission request")
        }
    }
}

#[gpui::test]
async fn test_no_pending_edits_if_tool_calls_are_completed(cx: &mut TestAppContext) {
    init_test(cx);
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fs, [path!("/test").as_ref()], cx).await;

    let connection = Rc::new(FakeAgentConnection::new().on_user_message({
        move |_, thread, mut cx| {
            async move {
                thread
                    .update(&mut cx, |thread, cx| {
                        thread.handle_session_update(
                            acp::SessionUpdate::ToolCall(
                                acp::ToolCall::new("test", "Label")
                                    .kind(acp::ToolKind::Edit)
                                    .status(acp::ToolCallStatus::Completed)
                                    .content(vec![acp::ToolCallContent::Diff(acp::Diff::new(
                                        "/test/test.txt",
                                        "foo",
                                    ))]),
                            ),
                            cx,
                        )
                    })
                    .unwrap()
                    .unwrap();
                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
            .boxed_local()
        }
    }));

    let thread = cx
        .update(|cx| {
            connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
        })
        .await
        .unwrap();

    cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["Hi".into()], cx)))
        .await
        .unwrap();

    assert!(cx.read(|cx| !thread.read(cx).has_pending_edit_tool_calls()));
}
