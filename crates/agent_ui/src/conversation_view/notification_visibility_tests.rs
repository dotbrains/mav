use super::tests::*;
use super::*;

#[gpui::test]
async fn test_notification_for_tool_authorization(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("1");
    let tool_call = acp::ToolCall::new(tool_call_id.clone(), "Label")
        .kind(acp::ToolKind::Edit)
        .content(vec!["hi".into()]);
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id,
        PermissionOptions::Flat(vec![acp::PermissionOption::new(
            "1",
            "Allow",
            acp::PermissionOptionKind::AllowOnce,
        )]),
    )]));

    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });

    cx.deactivate_window();

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    assert!(
        cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some())
    );
}

#[gpui::test]
async fn test_notification_when_panel_hidden(cx: &mut TestAppContext) {
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

    assert!(
        cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some()),
        "Expected notification when panel is hidden"
    );
}

#[gpui::test]
async fn test_notification_still_works_when_window_inactive(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });

    cx.deactivate_window();

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    assert!(
        cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some()),
        "Expected notification when window is inactive"
    );
}

#[gpui::test]
async fn test_notification_respects_never_setting(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        AgentSettings::override_global(
            AgentSettings {
                notify_when_agent_waiting: NotifyWhenAgentWaiting::Never,
                ..AgentSettings::get_global(cx).clone()
            },
            cx,
        );
    });

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Hello", window, cx);
    });

    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    assert!(
        !cx.windows()
            .iter()
            .any(|window| window.downcast::<AgentNotification>().is_some()),
        "Expected no notification when notify_when_agent_waiting is Never"
    );
}
