use super::tests::*;
use super::*;

fn create_test_acp_thread(
    parent_session_id: Option<acp::SessionId>,
    session_id: &str,
    connection: Rc<dyn AgentConnection>,
    project: Entity<Project>,
    cx: &mut App,
) -> Entity<AcpThread> {
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    cx.new(|cx| {
        AcpThread::new(
            parent_session_id,
            None,
            None,
            connection,
            project,
            action_log,
            acp::SessionId::new(session_id),
            watch::Receiver::constant(acp::PromptCapabilities::new()),
            cx,
        )
    })
}

fn request_test_tool_authorization(
    thread: &Entity<AcpThread>,
    tool_call_id: &str,
    option_id: &str,
    cx: &mut TestAppContext,
) -> Task<acp_thread::RequestPermissionOutcome> {
    let tool_call_id = acp::ToolCallId::new(tool_call_id);
    let label = format!("Tool {tool_call_id}");
    let option_id = acp::PermissionOptionId::new(option_id);
    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread
                .request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id, label)
                        .kind(acp::ToolKind::Edit)
                        .into(),
                    PermissionOptions::Flat(vec![acp::PermissionOption::new(
                        option_id,
                        "Allow",
                        acp::PermissionOptionKind::AllowOnce,
                    )]),
                    acp_thread::AuthorizationKind::PermissionGrant,
                    cx,
                )
                .unwrap()
        })
    })
}

#[gpui::test]
async fn test_conversation_multiple_tool_calls_fifo_ordering(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection: Rc<dyn AgentConnection> = Rc::new(StubAgentConnection::new());

    let session_id = acp::SessionId::new("session-1");
    let (thread, conversation) = cx.update(|cx| {
        let thread = create_test_acp_thread(None, "session-1", connection.clone(), project, cx);
        let conversation = cx.new(|cx| {
            let mut conversation = Conversation::default();
            conversation.register_thread(thread.clone(), cx);
            conversation
        });
        (thread, conversation)
    });

    let _task1 = request_test_tool_authorization(&thread, "tc-1", "allow-1", cx);
    let _task2 = request_test_tool_authorization(&thread, "tc-2", "allow-2", cx);

    cx.read(|cx| {
        let (_, tool_call_id, _) = conversation
            .read(cx)
            .pending_tool_call(&session_id, cx)
            .expect("Expected a pending tool call");
        assert_eq!(tool_call_id, acp::ToolCallId::new("tc-1"));
    });

    cx.update(|cx| {
        conversation.update(cx, |conversation, cx| {
            conversation.authorize_tool_call(
                session_id.clone(),
                acp::ToolCallId::new("tc-1"),
                SelectedPermissionOutcome::new(
                    acp::PermissionOptionId::new("allow-1"),
                    acp::PermissionOptionKind::AllowOnce,
                ),
                cx,
            );
        });
    });

    cx.run_until_parked();

    cx.read(|cx| {
        let (_, tool_call_id, _) = conversation
            .read(cx)
            .pending_tool_call(&session_id, cx)
            .expect("Expected tc-2 to be pending after tc-1 was authorized");
        assert_eq!(tool_call_id, acp::ToolCallId::new("tc-2"));
    });

    cx.update(|cx| {
        conversation.update(cx, |conversation, cx| {
            conversation.authorize_tool_call(
                session_id.clone(),
                acp::ToolCallId::new("tc-2"),
                SelectedPermissionOutcome::new(
                    acp::PermissionOptionId::new("allow-2"),
                    acp::PermissionOptionKind::AllowOnce,
                ),
                cx,
            );
        });
    });

    cx.run_until_parked();

    cx.read(|cx| {
        assert!(
            conversation
                .read(cx)
                .pending_tool_call(&session_id, cx)
                .is_none(),
            "Expected no pending tool calls after both were authorized"
        );
    });
}

#[gpui::test]
async fn test_conversation_subagent_scoped_pending_tool_call(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection: Rc<dyn AgentConnection> = Rc::new(StubAgentConnection::new());

    let parent_session_id = acp::SessionId::new("parent");
    let subagent_session_id = acp::SessionId::new("subagent");
    let (parent_thread, subagent_thread, conversation) = cx.update(|cx| {
        let parent_thread =
            create_test_acp_thread(None, "parent", connection.clone(), project.clone(), cx);
        let subagent_thread = create_test_acp_thread(
            Some(acp::SessionId::new("parent")),
            "subagent",
            connection,
            project,
            cx,
        );
        let conversation = cx.new(|cx| {
            let mut conversation = Conversation::default();
            conversation.register_thread(parent_thread.clone(), cx);
            conversation.register_thread(subagent_thread.clone(), cx);
            conversation
        });
        (parent_thread, subagent_thread, conversation)
    });

    let _parent_task =
        request_test_tool_authorization(&parent_thread, "parent-tc", "allow-parent", cx);
    let _subagent_task =
        request_test_tool_authorization(&subagent_thread, "subagent-tc", "allow-subagent", cx);

    cx.read(|cx| {
        let (returned_session_id, tool_call_id, _) = conversation
            .read(cx)
            .pending_tool_call(&subagent_session_id, cx)
            .expect("Expected subagent's pending tool call");
        assert_eq!(returned_session_id, subagent_session_id);
        assert_eq!(tool_call_id, acp::ToolCallId::new("subagent-tc"));
    });

    cx.read(|cx| {
        let (returned_session_id, tool_call_id, _) = conversation
            .read(cx)
            .pending_tool_call(&parent_session_id, cx)
            .expect("Expected a pending tool call from parent query");
        assert_eq!(returned_session_id, parent_session_id);
        assert_eq!(tool_call_id, acp::ToolCallId::new("parent-tc"));
    });
}

#[gpui::test]
async fn test_conversation_parent_pending_tool_call_returns_first_across_threads(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let connection: Rc<dyn AgentConnection> = Rc::new(StubAgentConnection::new());

    let session_id_a = acp::SessionId::new("thread-a");
    let session_id_b = acp::SessionId::new("thread-b");
    let (thread_a, thread_b, conversation) = cx.update(|cx| {
        let thread_a =
            create_test_acp_thread(None, "thread-a", connection.clone(), project.clone(), cx);
        let thread_b = create_test_acp_thread(None, "thread-b", connection, project, cx);
        let conversation = cx.new(|cx| {
            let mut conversation = Conversation::default();
            conversation.register_thread(thread_a.clone(), cx);
            conversation.register_thread(thread_b.clone(), cx);
            conversation
        });
        (thread_a, thread_b, conversation)
    });

    let _task_a = request_test_tool_authorization(&thread_a, "tc-a", "allow-a", cx);
    let _task_b = request_test_tool_authorization(&thread_b, "tc-b", "allow-b", cx);

    cx.read(|cx| {
        let (returned_session_id, tool_call_id, _) = conversation
            .read(cx)
            .pending_tool_call(&session_id_a, cx)
            .expect("Expected a pending tool call");
        assert_eq!(returned_session_id, session_id_a);
        assert_eq!(tool_call_id, acp::ToolCallId::new("tc-a"));
    });

    cx.read(|cx| {
        let (returned_session_id, tool_call_id, _) = conversation
            .read(cx)
            .pending_tool_call(&session_id_b, cx)
            .expect("Expected a pending tool call from thread-b query");
        assert_eq!(returned_session_id, session_id_a);
        assert_eq!(tool_call_id, acp::ToolCallId::new("tc-a"));
    });

    cx.update(|cx| {
        conversation.update(cx, |conversation, cx| {
            conversation.authorize_tool_call(
                session_id_a.clone(),
                acp::ToolCallId::new("tc-a"),
                SelectedPermissionOutcome::new(
                    acp::PermissionOptionId::new("allow-a"),
                    acp::PermissionOptionKind::AllowOnce,
                ),
                cx,
            );
        });
    });

    cx.run_until_parked();

    cx.read(|cx| {
        let (returned_session_id, tool_call_id, _) = conversation
            .read(cx)
            .pending_tool_call(&session_id_b, cx)
            .expect("Expected thread-b's tool call after thread-a's was authorized");
        assert_eq!(returned_session_id, session_id_b);
        assert_eq!(tool_call_id, acp::ToolCallId::new("tc-b"));
    });
}
