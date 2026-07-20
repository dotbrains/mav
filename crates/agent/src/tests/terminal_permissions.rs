use super::*;

#[gpui::test]
async fn test_terminal_tool_permission_rules(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({})).await;
    let project = Project::test(fs, ["/root".as_ref()], cx).await;

    // Test 1: Deny rule blocks command
    {
        let environment = Rc::new(cx.update(|cx| {
            FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.tools.insert(
                TerminalTool::NAME.into(),
                agent_settings::ToolRules {
                    default: Some(settings::ToolPermissionMode::Confirm),
                    always_allow: vec![],
                    always_deny: vec![
                        agent_settings::CompiledRegex::new(r"rm\s+-rf", false).unwrap(),
                    ],
                    always_confirm: vec![],
                    invalid_patterns: vec![],
                },
            );
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = Arc::new(crate::TerminalTool::new(project.clone(), environment));
        let (event_stream, _rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                ToolInput::resolved(crate::TerminalToolInput {
                    command: "rm -rf /".to_string(),
                    cd: ".".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        let result = task.await;
        assert!(
            result.is_err(),
            "expected command to be blocked by deny rule"
        );
        let err_msg = result.unwrap_err().to_lowercase();
        assert!(
            err_msg.contains("blocked"),
            "error should mention the command was blocked"
        );
    }

    // Test 2: Allow rule skips confirmation (and overrides default: Deny)
    {
        let environment = Rc::new(cx.update(|cx| {
            FakeThreadEnvironment::default()
                .with_terminal(FakeTerminalHandle::new_with_immediate_exit(cx, 0))
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.tools.insert(
                TerminalTool::NAME.into(),
                agent_settings::ToolRules {
                    default: Some(settings::ToolPermissionMode::Deny),
                    always_allow: vec![
                        agent_settings::CompiledRegex::new(r"^echo\s", false).unwrap(),
                    ],
                    always_deny: vec![],
                    always_confirm: vec![],
                    invalid_patterns: vec![],
                },
            );
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = Arc::new(crate::TerminalTool::new(project.clone(), environment));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                ToolInput::resolved(crate::TerminalToolInput {
                    command: "echo hello".to_string(),
                    cd: ".".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        let update = rx.expect_update_fields().await;
        assert!(
            update.content.iter().any(|blocks| {
                blocks
                    .iter()
                    .any(|c| matches!(c, acp::ToolCallContent::Terminal(_)))
            }),
            "expected terminal content (allow rule should skip confirmation and override default deny)"
        );

        let result = task.await;
        assert!(
            result.is_ok(),
            "expected command to succeed without confirmation"
        );
    }

    // Test 3: global default: allow does NOT override always_confirm patterns
    {
        let environment = Rc::new(cx.update(|cx| {
            FakeThreadEnvironment::default()
                .with_terminal(FakeTerminalHandle::new_with_immediate_exit(cx, 0))
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.insert(
                TerminalTool::NAME.into(),
                agent_settings::ToolRules {
                    default: Some(settings::ToolPermissionMode::Allow),
                    always_allow: vec![],
                    always_deny: vec![],
                    always_confirm: vec![
                        agent_settings::CompiledRegex::new(r"sudo", false).unwrap(),
                    ],
                    invalid_patterns: vec![],
                },
            );
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = Arc::new(crate::TerminalTool::new(project.clone(), environment));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let _task = cx.update(|cx| {
            tool.run(
                ToolInput::resolved(crate::TerminalToolInput {
                    command: "sudo rm file".to_string(),
                    cd: ".".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        // With global default: allow, confirm patterns are still respected
        // The expect_authorization() call will panic if no authorization is requested,
        // which validates that the confirm pattern still triggers confirmation
        let _auth = rx.expect_authorization().await;

        drop(_task);
    }

    // Test 4: tool-specific default: deny is respected even with global default: allow
    {
        let environment = Rc::new(cx.update(|cx| {
            FakeThreadEnvironment::default()
                .with_terminal(FakeTerminalHandle::new_with_immediate_exit(cx, 0))
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.insert(
                TerminalTool::NAME.into(),
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

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = Arc::new(crate::TerminalTool::new(project.clone(), environment));
        let (event_stream, _rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                ToolInput::resolved(crate::TerminalToolInput {
                    command: "echo hello".to_string(),
                    cd: ".".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        // tool-specific default: deny is respected even with global default: allow
        let result = task.await;
        assert!(
            result.is_err(),
            "expected command to be blocked by tool-specific deny default"
        );
        let err_msg = result.unwrap_err().to_lowercase();
        assert!(
            err_msg.contains("disabled"),
            "error should mention the tool is disabled, got: {err_msg}"
        );
    }
}
