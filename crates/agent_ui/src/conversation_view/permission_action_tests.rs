use super::tests::*;
use super::*;

fn disable_waiting_notifications(cx: &mut VisualTestContext) {
    cx.update(|_window, cx| {
        AgentSettings::override_global(
            AgentSettings {
                notify_when_agent_waiting: NotifyWhenAgentWaiting::Never,
                ..AgentSettings::get_global(cx).clone()
            },
            cx,
        );
    });
}

#[gpui::test]
async fn test_authorize_tool_call_action_triggers_authorization(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("action-test-1");
    let tool_call =
        acp::ToolCall::new(tool_call_id.clone(), "Run `cargo test`").kind(acp::ToolKind::Edit);
    let permission_options =
        ToolPermissionContext::new(TerminalTool::NAME, vec!["cargo test".to_string()])
            .build_permission_options();
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options,
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(conversation_view.clone(), cx);
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Run tests", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    conversation_view.read_with(cx, |conversation_view, cx| {
        assert!(conversation_view.pending_tool_call(cx).is_some());
    });

    conversation_view.update_in(cx, |_, window, cx| {
        window.dispatch_action(
            crate::AuthorizeToolCall {
                tool_call_id: "action-test-1".to_string(),
                option_id: "allow".to_string(),
                option_kind: "AllowOnce".to_string(),
            }
            .boxed_clone(),
            cx,
        );
    });

    cx.run_until_parked();

    conversation_view.read_with(cx, |conversation_view, cx| {
        assert!(conversation_view.pending_tool_call(cx).is_none());
    });
}

#[gpui::test]
async fn test_authorize_tool_call_action_with_pattern_option(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("pattern-action-test-1");
    let tool_call =
        acp::ToolCall::new(tool_call_id.clone(), "Run `npm install`").kind(acp::ToolKind::Edit);
    let permission_options =
        ToolPermissionContext::new(TerminalTool::NAME, vec!["npm install".to_string()])
            .build_permission_options();
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options.clone(),
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(conversation_view.clone(), cx);
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Install dependencies", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    let pattern_option = match &permission_options {
        PermissionOptions::Dropdown(choices) => choices
            .iter()
            .find(|choice| !choice.sub_patterns.is_empty())
            .map(|choice| &choice.allow)
            .expect("Should have a pattern option for npm command"),
        _ => panic!("Expected dropdown permission options"),
    };

    conversation_view.update_in(cx, |_, window, cx| {
        window.dispatch_action(
            crate::AuthorizeToolCall {
                tool_call_id: "pattern-action-test-1".to_string(),
                option_id: pattern_option.option_id.0.to_string(),
                option_kind: "AllowAlways".to_string(),
            }
            .boxed_clone(),
            cx,
        );
    });

    cx.run_until_parked();

    conversation_view.read_with(cx, |conversation_view, cx| {
        assert!(conversation_view.pending_tool_call(cx).is_none());
    });
}

#[gpui::test]
async fn test_granularity_selection_updates_state(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("granularity-test-1");
    let tool_call =
        acp::ToolCall::new(tool_call_id.clone(), "Run `cargo build`").kind(acp::ToolKind::Edit);
    let permission_options =
        ToolPermissionContext::new(TerminalTool::NAME, vec!["cargo build".to_string()])
            .build_permission_options();
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options.clone(),
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (thread_view, cx) = setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(thread_view.clone(), cx);
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&thread_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Build the project", window, cx);
    });
    active_thread(&thread_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    thread_view.read_with(cx, |thread_view, cx| {
        let state = thread_view.active_thread().unwrap();
        let selected = state.read(cx).permission_selections.get(&tool_call_id);
        assert!(selected.is_none());
    });

    thread_view.update_in(cx, |_, window, cx| {
        window.dispatch_action(
            crate::SelectPermissionGranularity {
                tool_call_id: "granularity-test-1".to_string(),
                index: 0,
            }
            .boxed_clone(),
            cx,
        );
    });

    cx.run_until_parked();

    thread_view.read_with(cx, |thread_view, cx| {
        let state = thread_view.active_thread().unwrap();
        let selected = state.read(cx).permission_selections.get(&tool_call_id);
        assert_eq!(selected.and_then(|s| s.choice_index()), Some(0));
    });
}

#[gpui::test]
async fn test_allow_button_uses_selected_granularity(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("allow-granularity-test-1");
    let tool_call =
        acp::ToolCall::new(tool_call_id.clone(), "Run `npm install`").kind(acp::ToolKind::Edit);
    let permission_options =
        ToolPermissionContext::new(TerminalTool::NAME, vec!["npm install".to_string()])
            .build_permission_options();
    let PermissionOptions::Dropdown(choices) = &permission_options else {
        panic!("Expected dropdown permission options");
    };
    assert_eq!(choices.len(), 3);
    assert!(
        choices[0]
            .allow
            .option_id
            .0
            .contains("always_allow:terminal")
    );
    assert!(
        choices[1]
            .allow
            .option_id
            .0
            .contains("always_allow:terminal")
    );
    assert!(!choices[1].sub_patterns.is_empty());
    assert_eq!(choices[2].allow.option_id.0.as_ref(), "allow");

    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options.clone(),
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (thread_view, cx) = setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(thread_view.clone(), cx);
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&thread_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Install dependencies", window, cx);
    });
    active_thread(&thread_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    thread_view.update_in(cx, |_, window, cx| {
        window.dispatch_action(
            crate::SelectPermissionGranularity {
                tool_call_id: "allow-granularity-test-1".to_string(),
                index: 1,
            }
            .boxed_clone(),
            cx,
        );
    });

    cx.run_until_parked();

    active_thread(&thread_view, cx).update_in(cx, |view, window, cx| {
        view.allow_once(&AllowOnce, window, cx)
    });

    cx.run_until_parked();

    thread_view.read_with(cx, |thread_view, cx| {
        assert!(thread_view.pending_tool_call(cx).is_none());
    });
}

#[gpui::test]
async fn test_deny_button_uses_selected_granularity(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("deny-granularity-test-1");
    let tool_call =
        acp::ToolCall::new(tool_call_id.clone(), "Run `git push`").kind(acp::ToolKind::Edit);
    let permission_options =
        ToolPermissionContext::new(TerminalTool::NAME, vec!["git push".to_string()])
            .build_permission_options();
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options.clone(),
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    add_to_workspace(conversation_view.clone(), cx);
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Push changes", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| {
        view.reject_once(&RejectOnce, window, cx)
    });

    cx.run_until_parked();

    conversation_view.read_with(cx, |conversation_view, cx| {
        assert!(conversation_view.pending_tool_call(cx).is_none());
    });
}
