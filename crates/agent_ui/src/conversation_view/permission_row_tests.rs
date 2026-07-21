use super::tests::*;
use super::*;

async fn setup_pending_permission_thread<'a>(
    tool_call_id: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<ConversationView>,
    Entity<ThreadView>,
    usize,
    &'a mut VisualTestContext,
) {
    let tool_call_id_value = acp::ToolCallId::new(tool_call_id);
    let tool_call =
        acp::ToolCall::new(tool_call_id_value.clone(), "Run something").kind(acp::ToolKind::Edit);
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id_value.clone(),
        PermissionOptions::Flat(vec![acp::PermissionOption::new(
            "allow",
            "Allow",
            acp::PermissionOptionKind::AllowOnce,
        )]),
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(conversation_view.clone(), cx);
    cx.update(|_window, cx| {
        AgentSettings::override_global(
            AgentSettings {
                notify_when_agent_waiting: NotifyWhenAgentWaiting::Never,
                ..AgentSettings::get_global(cx).clone()
            },
            cx,
        );
    });

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));
    cx.run_until_parked();

    let thread_view = active_thread(&conversation_view, cx);
    let entry_ix = thread_view.read_with(cx, |view, cx| {
        view.thread
            .read(cx)
            .entries()
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    acp_thread::AgentThreadEntry::ToolCall(call) if call.id == tool_call_id_value
                )
            })
            .expect("tool call entry should exist after run_until_parked")
    });

    (conversation_view, thread_view, entry_ix, cx)
}

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
    let option_id = acp::PermissionOptionId::new(option_id);
    cx.update(|cx| {
        thread.update(cx, |thread, cx| {
            thread
                .request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id.clone(), format!("Tool {tool_call_id}"))
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

struct TestListView {
    list_state: ListState,
}

impl Render for TestListView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        list(self.list_state.clone(), |_, _, _| {
            div().h(px(20.0)).w_full().into_any_element()
        })
        .size_full()
    }
}

fn draw_thread_list_at(
    thread_view: &Entity<ThreadView>,
    scroll_top: ListOffset,
    cx: &mut VisualTestContext,
) {
    let list_state = thread_view.read_with(cx, |view, _cx| view.list_state.clone());
    list_state.scroll_to(scroll_top);
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(100.0), px(20.0)),
        |_, cx| {
            cx.new(|_| TestListView {
                list_state: list_state.clone(),
            })
            .into_any_element()
        },
    );
}

#[gpui::test]
async fn test_permission_row_hidden_when_inline_bounds_unavailable(cx: &mut TestAppContext) {
    init_test(cx);

    let (_view, thread_view, entry_ix, cx) =
        setup_pending_permission_thread("perm-no-bounds", cx).await;
    thread_view.read_with(cx, |view, _cx| {
        view.list_state.scroll_to(ListOffset {
            item_ix: entry_ix,
            offset_in_item: px(0.0),
        });
    });
    thread_view.update_in(cx, |view, window, cx| {
        assert!(
            view.render_main_agent_awaiting_permission(window, cx)
                .is_none()
        );
    });
}

#[gpui::test]
async fn test_pending_tool_call_for_session_scopes_to_that_session(cx: &mut TestAppContext) {
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
        assert_eq!(
            conversation
                .read(cx)
                .pending_tool_call_for_session(&session_id_a, cx),
            Some(acp::ToolCallId::new("tc-a"))
        );
        assert_eq!(
            conversation
                .read(cx)
                .pending_tool_call_for_session(&session_id_b, cx),
            Some(acp::ToolCallId::new("tc-b"))
        );
    });
}

#[gpui::test]
async fn test_permission_row_scroll_to_dismisses_row(cx: &mut TestAppContext) {
    init_test(cx);

    let (_view, thread_view, entry_ix, cx) =
        setup_pending_permission_thread("perm-scroll", cx).await;
    draw_thread_list_at(
        &thread_view,
        ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        },
        cx,
    );
    thread_view.read_with(cx, |view, _cx| {
        assert!(view.list_state.bounds_for_item(entry_ix).is_some());
    });
    thread_view.update_in(cx, |view, window, cx| {
        assert!(
            view.render_main_agent_awaiting_permission(window, cx)
                .is_some()
        );
    });

    draw_thread_list_at(
        &thread_view,
        ListOffset {
            item_ix: entry_ix,
            offset_in_item: px(0.0),
        },
        cx,
    );
    thread_view.update_in(cx, |view, window, cx| {
        assert!(
            view.render_main_agent_awaiting_permission(window, cx)
                .is_none()
        );
    });
}

#[gpui::test]
async fn test_permission_row_does_not_flicker_when_activity_bar_squeezes_list(
    cx: &mut TestAppContext,
) {
    init_test(cx);

    let (_view, thread_view, _entry_ix, cx) =
        setup_pending_permission_thread("perm-flicker", cx).await;
    let thread = thread_view.read_with(cx, |view, _cx| view.thread.clone());
    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                    acp::ToolCallId::new("perm-flicker"),
                    acp::ToolCallUpdateFields::new().content(vec![acp::ToolCallContent::Content(
                        acp::Content::new(acp::ContentBlock::Text(acp::TextContent::new(
                            "Plan step\n\n".repeat(100),
                        ))),
                    )]),
                )),
                cx,
            )
            .expect("tool call content update should be accepted");
    });
    cx.run_until_parked();

    thread_view.read_with(cx, |view, _cx| {
        view.list_state.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        });
    });

    let mut row_visibility = Vec::new();
    for _ in 0..4 {
        thread_view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        thread_view.update_in(cx, |view, window, cx| {
            row_visibility.push(
                view.render_main_agent_awaiting_permission(window, cx)
                    .is_some(),
            );
        });
    }
    assert_eq!(row_visibility, vec![true; 4]);
}

#[gpui::test]
async fn test_permission_row_shown_when_inline_prompt_is_above_viewport(cx: &mut TestAppContext) {
    init_test(cx);

    let (_view, thread_view, entry_ix, cx) =
        setup_pending_permission_thread("perm-above", cx).await;
    let thread = thread_view.read_with(cx, |view, _cx| view.thread.clone());
    thread.update(cx, |thread, cx| {
        thread
            .handle_session_update(
                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                    "More content".into(),
                )),
                cx,
            )
            .expect("following assistant message should be accepted");
    });

    draw_thread_list_at(
        &thread_view,
        ListOffset {
            item_ix: entry_ix + 1,
            offset_in_item: px(0.0),
        },
        cx,
    );
    thread_view.update_in(cx, |view, window, cx| {
        assert!(
            view.render_main_agent_awaiting_permission(window, cx)
                .is_some()
        );
    });

    draw_thread_list_at(
        &thread_view,
        ListOffset {
            item_ix: entry_ix,
            offset_in_item: px(0.0),
        },
        cx,
    );
    thread_view.update_in(cx, |view, window, cx| {
        assert!(
            view.render_main_agent_awaiting_permission(window, cx)
                .is_none()
        );
    });
}

#[gpui::test]
async fn test_permission_row_disappears_when_authorized(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, thread_view, _entry_ix, cx) =
        setup_pending_permission_thread("perm-allow", cx).await;
    draw_thread_list_at(
        &thread_view,
        ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        },
        cx,
    );
    thread_view.update_in(cx, |view, window, cx| {
        assert!(
            view.render_main_agent_awaiting_permission(window, cx)
                .is_some()
        );
    });

    conversation_view.update_in(cx, |_, window, cx| {
        window.dispatch_action(
            crate::AuthorizeToolCall {
                tool_call_id: "perm-allow".to_string(),
                option_id: "allow".to_string(),
                option_kind: "AllowOnce".to_string(),
            }
            .boxed_clone(),
            cx,
        );
    });
    cx.run_until_parked();

    conversation_view.read_with(cx, |view, cx| {
        assert!(view.pending_tool_call(cx).is_none());
    });
    thread_view.update_in(cx, |view, window, cx| {
        assert!(
            view.render_main_agent_awaiting_permission(window, cx)
                .is_none()
        );
    });
}

#[gpui::test]
async fn test_permission_row_ignores_subagent_requests(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;
    add_to_workspace(conversation_view.clone(), cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));
    cx.run_until_parked();

    let thread_view = active_thread(&conversation_view, cx);
    let parent_session_id =
        thread_view.read_with(cx, |view, cx| view.thread.read(cx).session_id().clone());
    let conversation = thread_view.read_with(cx, |view, _cx| view.conversation.clone());

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let stub: Rc<dyn AgentConnection> = Rc::new(StubAgentConnection::new());
    let subagent_thread = cx.update(|_window, cx| {
        create_test_acp_thread(
            Some(parent_session_id.clone()),
            "subagent",
            stub,
            project,
            cx,
        )
    });
    conversation.update(cx, |conversation, cx| {
        conversation.register_thread(subagent_thread.clone(), cx);
    });
    let _subagent_task =
        request_test_tool_authorization(&subagent_thread, "sub-tc", "allow-sub", cx);
    cx.run_until_parked();

    cx.read(|cx| {
        assert!(
            conversation
                .read(cx)
                .pending_tool_call_for_session(&parent_session_id, cx)
                .is_none()
        );
        assert!(
            !conversation
                .read(cx)
                .subagents_awaiting_permission(cx)
                .is_empty()
        );
    });

    thread_view.update_in(cx, |view, window, cx| {
        assert!(
            view.render_main_agent_awaiting_permission(window, cx)
                .is_none()
        );
    });
}
