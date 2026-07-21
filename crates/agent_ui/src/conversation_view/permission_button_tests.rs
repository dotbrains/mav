use super::tests::*;
use super::*;

fn permission_labels(
    conversation_view: &Entity<ConversationView>,
    cx: &mut VisualTestContext,
) -> Vec<SharedString> {
    conversation_view.read_with(cx, |conversation_view, cx| {
        let thread = conversation_view
            .active_thread()
            .expect("Thread should exist")
            .read(cx)
            .thread
            .clone();
        let thread = thread.read(cx);

        let tool_call = thread
            .entries()
            .iter()
            .find_map(|entry| match entry {
                acp_thread::AgentThreadEntry::ToolCall(call) => Some(call),
                _ => None,
            })
            .expect("Expected a tool call entry");

        let acp_thread::ToolCallStatus::WaitingForConfirmation { options, .. } = &tool_call.status
        else {
            panic!("Expected WaitingForConfirmation status");
        };

        let PermissionOptions::Dropdown(choices) = options else {
            panic!("Expected dropdown permission options");
        };

        choices
            .iter()
            .map(|choice| choice.allow.name.clone())
            .collect()
    })
}

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
async fn test_tool_permission_buttons_terminal_with_pattern(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("terminal-1");
    let tool_call = acp::ToolCall::new(tool_call_id.clone(), "Run `cargo build --release`")
        .kind(acp::ToolKind::Edit);
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["cargo build --release".to_string()],
    )
    .build_permission_options();
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options,
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Run cargo build", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    let labels = permission_labels(&conversation_view, cx);
    assert_eq!(labels.len(), 3);
    assert!(labels.iter().any(|label| label == "Always for terminal"));
    assert!(
        labels
            .iter()
            .any(|label| label == "Always for `cargo build` commands")
    );
    assert!(labels.iter().any(|label| label == "Only this time"));
}

#[gpui::test]
async fn test_tool_permission_buttons_edit_file_with_path_pattern(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("edit-file-1");
    let tool_call =
        acp::ToolCall::new(tool_call_id.clone(), "Edit `src/main.rs`").kind(acp::ToolKind::Edit);
    let permission_options =
        ToolPermissionContext::new(EditFileTool::NAME, vec!["src/main.rs".to_string()])
            .build_permission_options();
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options,
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Edit the main file", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    let labels = permission_labels(&conversation_view, cx);
    assert!(labels.iter().any(|label| label == "Always for edit file"));
    assert!(labels.iter().any(|label| label == "Always for `src/`"));
}

#[gpui::test]
async fn test_tool_permission_buttons_fetch_with_domain_pattern(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("fetch-1");
    let tool_call = acp::ToolCall::new(tool_call_id.clone(), "Fetch `https://docs.rs/gpui`")
        .kind(acp::ToolKind::Fetch);
    let permission_options =
        ToolPermissionContext::new(FetchTool::NAME, vec!["https://docs.rs/gpui".to_string()])
            .build_permission_options();
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options,
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Fetch the docs", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    let labels = permission_labels(&conversation_view, cx);
    assert!(labels.iter().any(|label| label == "Always for fetch"));
    assert!(labels.iter().any(|label| label == "Always for `docs.rs`"));
}

#[gpui::test]
async fn test_tool_permission_buttons_without_pattern(cx: &mut TestAppContext) {
    init_test(cx);

    let tool_call_id = acp::ToolCallId::new("terminal-no-pattern-1");
    let tool_call = acp::ToolCall::new(tool_call_id.clone(), "Run `./deploy.sh --production`")
        .kind(acp::ToolKind::Edit);
    let permission_options = ToolPermissionContext::new(
        TerminalTool::NAME,
        vec!["./deploy.sh --production".to_string()],
    )
    .build_permission_options();
    let connection = StubAgentConnection::new().with_permission_requests(HashMap::from_iter([(
        tool_call_id.clone(),
        permission_options,
    )]));
    connection.set_next_prompt_updates(vec![acp::SessionUpdate::ToolCall(tool_call)]);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(connection), cx).await;
    disable_waiting_notifications(cx);

    let message_editor = message_editor(&conversation_view, cx);
    message_editor.update_in(cx, |editor, window, cx| {
        editor.set_text("Run the deploy script", window, cx);
    });
    active_thread(&conversation_view, cx).update_in(cx, |view, window, cx| view.send(window, cx));

    cx.run_until_parked();

    let labels = permission_labels(&conversation_view, cx);
    assert_eq!(labels.len(), 2);
    assert!(labels.iter().any(|label| label == "Always for terminal"));
    assert!(labels.iter().any(|label| label == "Only this time"));
    assert!(!labels.iter().any(|label| label.contains("commands")));
}
