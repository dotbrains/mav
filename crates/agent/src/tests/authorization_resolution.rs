use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_always_allow_resolves_pending_authorizations(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(ToolRequiringPermission);
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Two parallel tool calls, both require permission.
    for id in ["tool_id_1", "tool_id_2"] {
        fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
            LanguageModelToolUse {
                id: id.into(),
                name: ToolRequiringPermission::NAME.into(),
                raw_input: "{}".into(),
                input: json!({}),
                is_input_complete: true,
                thought_signature: None,
            },
        ));
    }
    fake_model.end_last_completion_stream();

    let tool_call_auth_1 = next_tool_call_authorization(&mut events).await;
    let tool_call_auth_2 = next_tool_call_authorization(&mut events).await;

    // Approve the first with "always allow" — this persists a setting that
    // makes the tool unconditionally allowed. The second pending
    // authorization should resolve without user interaction.
    tool_call_auth_1
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("always_allow:tool_requiring_permission"),
            acp::PermissionOptionKind::AllowAlways,
        ))
        .unwrap();
    cx.run_until_parked();

    // The second tool's receiver was dropped by the auto-resolve path, so
    // sending a late response should fail.
    let late_send = tool_call_auth_2
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ));
    assert!(
        late_send.is_err(),
        "expected tool 2's response receiver to be dropped after auto-resolve"
    );

    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    let results: Vec<_> = message
        .content
        .iter()
        .filter_map(|c| match c {
            language_model::MessageContent::ToolResult(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(
        results.len(),
        2,
        "both tool calls should have produced results"
    );
    assert!(
        results.iter().all(|r| !r.is_error),
        "both results should be successful after auto-resolve, got: {:?}",
        results
    );
}

/// Externally editing settings (e.g. the user opening settings.json and
/// adding an `always_allow` rule) resolves pending authorization prompts
/// for tool calls that match the new rule.
#[gpui::test]
async fn test_external_settings_edit_resolves_pending_authorization(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(ToolRequiringPermission);
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_1".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    let tool_call_auth = next_tool_call_authorization(&mut events).await;

    // Simulate the user editing settings.json to globally allow the tool.
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            ToolRequiringPermission::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Allow),
                always_allow: vec![],
                always_deny: vec![],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });
    cx.run_until_parked();

    // The pending prompt auto-resolves without the user clicking anything.
    let late_send = tool_call_auth
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ));
    assert!(
        late_send.is_err(),
        "response receiver should have been dropped after settings-driven auto-resolve"
    );

    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    let result = message
        .content
        .iter()
        .find_map(|c| match c {
            language_model::MessageContent::ToolResult(r) => Some(r),
            _ => None,
        })
        .expect("expected a tool result");
    assert!(!result.is_error, "tool should have been auto-allowed");
}

/// Externally adding a deny rule to settings dismisses a pending
/// authorization prompt and returns the tool call as denied.
#[gpui::test]
async fn test_external_deny_rule_resolves_pending_authorization(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(ToolRequiringPermission);
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_1".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    let tool_call_auth = next_tool_call_authorization(&mut events).await;

    // Simulate the user adding a deny default for the tool.
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            ToolRequiringPermission::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Deny),
                always_allow: vec![],
                always_deny: vec![],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });
    cx.run_until_parked();

    let late_send = tool_call_auth
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ));
    assert!(
        late_send.is_err(),
        "response receiver should have been dropped after deny auto-resolve"
    );

    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    let result = message
        .content
        .iter()
        .find_map(|c| match c {
            language_model::MessageContent::ToolResult(r) => Some(r),
            _ => None,
        })
        .expect("expected a tool result");
    assert!(
        result.is_error,
        "tool should have been auto-denied by the new rule"
    );
}

/// Unrelated settings changes must not spuriously resolve pending
/// authorizations: if the re-check still returns `Confirm`, the prompt
/// stays visible and waits for the user.
#[gpui::test]
async fn test_unrelated_settings_change_does_not_resolve_pending_authorization(
    cx: &mut TestAppContext,
) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(ToolRequiringPermission);
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_1".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    let tool_call_auth = next_tool_call_authorization(&mut events).await;

    // Touch SettingsStore with a change that doesn't affect tool
    // permissions; the pending authorization should remain pending.
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.single_file_review = !settings.single_file_review;
        agent_settings::AgentSettings::override_global(settings, cx);
    });
    cx.run_until_parked();

    // The user still has to act — resolve with an Allow Once.
    tool_call_auth
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .expect("response receiver should still be alive");
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    let result = message
        .content
        .iter()
        .find_map(|c| match c {
            language_model::MessageContent::ToolResult(r) => Some(r),
            _ => None,
        })
        .expect("expected a tool result");
    assert!(!result.is_error);
}

/// Approving one pending tool call with "Always for <tool A>" must not
/// dismiss a sibling pending authorization for a *different* tool: the
/// persisted rule is scoped to tool A, so tool B's prompt stays visible
/// and waits for the user.
#[gpui::test]
async fn test_always_allow_does_not_resolve_unrelated_tool_authorization(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(ToolRequiringPermission);
            thread.add_tool(ToolRequiringPermission2);
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Two parallel tool calls, each for a distinct tool with its own
    // permission scope.
    for (id, name) in [
        ("tool_id_1", ToolRequiringPermission::NAME),
        ("tool_id_2", ToolRequiringPermission2::NAME),
    ] {
        fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
            LanguageModelToolUse {
                id: id.into(),
                name: name.into(),
                raw_input: "{}".into(),
                input: json!({}),
                is_input_complete: true,
                thought_signature: None,
            },
        ));
    }
    fake_model.end_last_completion_stream();

    let auth_a = next_tool_call_authorization(&mut events).await;
    let auth_b = next_tool_call_authorization(&mut events).await;

    // Match prompts back to their originating tools via the authorization
    // context so the test doesn't depend on scheduling order.
    let (auth_for_tool_1, auth_for_tool_2) = {
        let a_name = auth_a
            .context
            .as_ref()
            .expect("settings-driven authorization must carry a context")
            .tool_name
            .clone();
        if a_name == ToolRequiringPermission::NAME {
            (auth_a, auth_b)
        } else {
            (auth_b, auth_a)
        }
    };

    // Approve tool 1 with "always allow". Only tool 1's rule is persisted.
    auth_for_tool_1
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("always_allow:tool_requiring_permission"),
            acp::PermissionOptionKind::AllowAlways,
        ))
        .unwrap();
    cx.run_until_parked();

    // Tool 2's receiver must still be alive: its permission is unrelated
    // to the rule that was just added, so its prompt stays pending.
    auth_for_tool_2
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .expect("tool 2's response receiver should still be alive");
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    let results: Vec<_> = message
        .content
        .iter()
        .filter_map(|c| match c {
            language_model::MessageContent::ToolResult(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(
        results.len(),
        2,
        "both tool calls should have produced results"
    );
    assert!(
        results.iter().all(|r| !r.is_error),
        "both results should be successful, got: {:?}",
        results
    );
}
