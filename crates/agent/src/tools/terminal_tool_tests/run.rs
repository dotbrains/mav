    #[gpui::test]
    async fn test_run_rejects_invalid_substitution_before_terminal_creation(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default()
                .with_terminal(crate::tests::FakeTerminalHandle::new_never_exits(cx))
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Confirm;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(TerminalTool::new(project, environment.clone()));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                crate::ToolInput::resolved(TerminalToolInput {
                    command: "echo $HOME".to_string(),
                    cd: "root".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        let result = task.await;
        let error = result.expect_err("expected invalid terminal command to be rejected");
        assert!(
            error.contains("does not allow shell substitutions or interpolations"),
            "expected explicit invalid-command message, got: {error}"
        );
        assert!(
            environment.terminal_creation_count() == 0,
            "terminal should not be created for invalid commands"
        );
        assert!(
            !matches!(
                rx.try_recv(),
                Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
            ),
            "invalid command should not request authorization"
        );
        assert!(
            !matches!(
                rx.try_recv(),
                Ok(Ok(crate::ThreadEvent::ToolCallUpdate(
                    acp_thread::ToolCallUpdate::UpdateFields(_)
                )))
            ),
            "invalid command should not emit a terminal card update"
        );
    }

    #[gpui::test]
    async fn test_run_allows_invalid_substitution_in_unconditional_allow_all_mode(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0),
            )
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(TerminalTool::new(project, environment.clone()));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                crate::ToolInput::resolved(TerminalToolInput {
                    command: "echo $HOME".to_string(),
                    cd: "root".to_string(),
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
                    .any(|content| matches!(content, acp::ToolCallContent::Terminal(_)))
            }),
            "expected terminal content update in unconditional allow-all mode"
        );

        let result = task
            .await
            .expect("command should proceed in unconditional allow-all mode");
        assert!(
            environment.terminal_creation_count() == 1,
            "terminal should be created exactly once"
        );
        assert!(
            !result.contains("could not be approved"),
            "unexpected invalid-command rejection output: {result}"
        );
    }

    #[gpui::test]
    async fn test_run_hardcoded_denial_still_wins_in_unconditional_allow_all_mode(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default()
                .with_terminal(crate::tests::FakeTerminalHandle::new_never_exits(cx))
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(TerminalTool::new(project, environment.clone()));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                crate::ToolInput::resolved(TerminalToolInput {
                    command: "echo $(rm -rf /)".to_string(),
                    cd: "root".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        let error = task
            .await
            .expect_err("hardcoded denial should override unconditional allow-all");
        assert!(
            error.contains("built-in security rule"),
            "expected hardcoded denial message, got: {error}"
        );
        assert!(
            environment.terminal_creation_count() == 0,
            "hardcoded denial should prevent terminal creation"
        );
        assert!(
            !matches!(
                rx.try_recv(),
                Ok(Ok(crate::ThreadEvent::ToolCallAuthorization(_)))
            ),
            "hardcoded denial should not request authorization"
        );
    }

    #[gpui::test]
    async fn test_run_env_prefixed_allow_pattern_is_used_end_to_end(cx: &mut gpui::TestAppContext) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0),
            )
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Deny;
            settings.tool_permissions.tools.insert(
                TerminalTool::NAME.into(),
                agent_settings::ToolRules {
                    default: Some(settings::ToolPermissionMode::Deny),
                    always_allow: vec![
                        agent_settings::CompiledRegex::new(r"^PAGER=blah\s+git\s+log(\s|$)", false)
                            .unwrap(),
                    ],
                    always_deny: vec![],
                    always_confirm: vec![],
                    invalid_patterns: vec![],
                },
            );
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(TerminalTool::new(project, environment.clone()));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                crate::ToolInput::resolved(TerminalToolInput {
                    command: "PAGER=blah git log --oneline".to_string(),
                    cd: "root".to_string(),
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
                    .any(|content| matches!(content, acp::ToolCallContent::Terminal(_)))
            }),
            "expected terminal content update for matching env-prefixed allow rule"
        );

        let result = task
            .await
            .expect("expected env-prefixed command to be allowed");
        assert!(
            environment.terminal_creation_count() == 1,
            "terminal should be created for allowed env-prefixed command"
        );
        assert!(
            result.contains("command output") || result.contains("Command executed successfully."),
            "unexpected terminal result: {result}"
        );
    }

    #[gpui::test]
    async fn test_run_filters_model_output_and_bypasses_byte_limit_when_head_or_tail_is_set(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let output =
            acp::TerminalOutputResponse::new("one\ntwo\nthree\nfour\nfive".to_string(), false)
                .exit_status(acp::TerminalExitStatus::new().exit_code(0));
        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0)
                    .with_output(output),
            )
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(TerminalTool::new(project, environment.clone()));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                crate::ToolInput::resolved(TerminalToolInput {
                    command: "printf lines".to_string(),
                    cd: "root".to_string(),
                    timeout_ms: None,
                    head_lines: Some(1),
                    tail_lines: Some(1),
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
                    .any(|content| matches!(content, acp::ToolCallContent::Terminal(_)))
            }),
            "expected terminal content update"
        );

        let result = task.await.expect("terminal command should succeed");
        assert_eq!(result, "```\none\n\nfive\n```");
        assert_eq!(environment.terminal_output_limits(), vec![None]);
    }

    #[gpui::test]
    async fn test_run_uses_byte_limit_when_head_and_tail_are_not_set(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let output = acp::TerminalOutputResponse::new("command output".to_string(), false)
            .exit_status(acp::TerminalExitStatus::new().exit_code(0));
        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0)
                    .with_output(output),
            )
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(TerminalTool::new(project, environment.clone()));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let task = cx.update(|cx| {
            tool.run(
                crate::ToolInput::resolved(TerminalToolInput {
                    command: "echo output".to_string(),
                    cd: "root".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        rx.expect_update_fields().await;
        let result = task.await.expect("terminal command should succeed");
        assert_eq!(result, "```\ncommand output\n```");
        assert_eq!(
            environment.terminal_output_limits(),
            vec![Some(COMMAND_OUTPUT_LIMIT)]
        );
    }

    #[gpui::test]
    async fn test_run_old_anchored_git_pattern_no_longer_auto_allows_env_prefix(
        cx: &mut gpui::TestAppContext,
    ) {
        crate::tests::init_test(cx);

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0),
            )
        }));

        cx.update(|cx| {
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Deny;
            settings.tool_permissions.tools.insert(
                TerminalTool::NAME.into(),
                agent_settings::ToolRules {
                    default: Some(settings::ToolPermissionMode::Confirm),
                    always_allow: vec![
                        agent_settings::CompiledRegex::new(r"^git\b", false).unwrap(),
                    ],
                    always_deny: vec![],
                    always_confirm: vec![],
                    invalid_patterns: vec![],
                },
            );
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(TerminalTool::new(project, environment.clone()));
        let (event_stream, mut rx) = crate::ToolCallEventStream::test();

        let _task = cx.update(|cx| {
            tool.run(
                crate::ToolInput::resolved(TerminalToolInput {
                    command: "PAGER=blah git log".to_string(),
                    cd: "root".to_string(),
                    timeout_ms: None,
                    ..Default::default()
                }),
                event_stream,
                cx,
            )
        });

        let _auth = rx.expect_authorization().await;
        assert!(
            environment.terminal_creation_count() == 0,
            "confirm flow should not create terminal before authorization"
        );
    }
