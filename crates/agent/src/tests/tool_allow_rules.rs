use super::*;

#[gpui::test]
async fn test_edit_file_tool_allow_rule_skips_confirmation(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({"README.md": "# Hello"}))
        .await;
    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            EditFileTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Confirm),
                always_allow: vec![agent_settings::CompiledRegex::new(r"\.md$", false).unwrap()],
                always_deny: vec![],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let context_server_registry =
        cx.new(|cx| crate::ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
    let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
    let templates = crate::Templates::new();
    let thread = cx.new(|cx| {
        crate::Thread::new(
            project.clone(),
            cx.new(|_cx| prompt_store::ProjectContext::default()),
            context_server_registry,
            templates.clone(),
            None,
            cx,
        )
    });
    let action_log = thread.read_with(cx, |thread, _cx| thread.action_log().clone());

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::EditFileTool::new(
        project,
        thread.downgrade(),
        action_log,
        language_registry,
    ));
    let (event_stream, mut rx) = crate::ToolCallEventStream::test();

    let _task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::EditFileToolInput {
                path: "root/README.md".into(),
                edits: vec![],
            }),
            event_stream,
            cx,
        )
    });

    cx.run_until_parked();

    let event = rx.try_recv();
    assert!(
        !matches!(event, Ok(Ok(ThreadEvent::ToolCallAuthorization(_)))),
        "expected no authorization request for allowed .md file"
    );
}

#[gpui::test]
async fn test_edit_file_tool_allow_still_prompts_for_local_settings(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            ".mav": {
                "settings.json": "{}"
            },
            "README.md": "# Hello"
        }),
    )
    .await;
    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let context_server_registry =
        cx.new(|cx| crate::ContextServerRegistry::new(project.read(cx).context_server_store(), cx));
    let language_registry = project.read_with(cx, |project, _cx| project.languages().clone());
    let templates = crate::Templates::new();
    let thread = cx.new(|cx| {
        crate::Thread::new(
            project.clone(),
            cx.new(|_cx| prompt_store::ProjectContext::default()),
            context_server_registry,
            templates.clone(),
            None,
            cx,
        )
    });
    let action_log = thread.read_with(cx, |thread, _cx| thread.action_log().clone());

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::EditFileTool::new(
        project,
        thread.downgrade(),
        action_log,
        language_registry,
    ));

    // Editing a file inside .mav/ should still prompt even with global default: allow,
    // because local settings paths are sensitive and require confirmation regardless.
    let (event_stream, mut rx) = crate::ToolCallEventStream::test();
    let _task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::EditFileToolInput {
                path: "root/.mav/settings.json".into(),
                edits: vec![],
            }),
            event_stream,
            cx,
        )
    });

    let _update = rx.expect_update_fields().await;
    let _auth = rx.expect_authorization().await;
}

#[gpui::test]
async fn test_fetch_tool_deny_rule_blocks_url(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            FetchTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Allow),
                always_allow: vec![],
                always_deny: vec![
                    agent_settings::CompiledRegex::new(r"internal\.company\.com", false).unwrap(),
                ],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let http_client = gpui::http_client::FakeHttpClient::with_200_response();

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::FetchTool::new(http_client));
    let (event_stream, _rx) = crate::ToolCallEventStream::test();

    let input: crate::FetchToolInput =
        serde_json::from_value(json!({"url": "https://internal.company.com/api"})).unwrap();

    let task = cx.update(|cx| tool.run(ToolInput::resolved(input), event_stream, cx));

    let result = task.await;
    assert!(result.is_err(), "expected fetch to be blocked");
    assert!(
        result.unwrap_err().contains("blocked"),
        "error should mention the fetch was blocked"
    );
}

#[gpui::test]
async fn test_fetch_tool_allow_rule_skips_confirmation(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            FetchTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Confirm),
                always_allow: vec![agent_settings::CompiledRegex::new(r"docs\.rs", false).unwrap()],
                always_deny: vec![],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let http_client = gpui::http_client::FakeHttpClient::with_200_response();

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::FetchTool::new(http_client));
    let (event_stream, mut rx) = crate::ToolCallEventStream::test();

    let input: crate::FetchToolInput =
        serde_json::from_value(json!({"url": "https://docs.rs/some-crate"})).unwrap();

    let _task = cx.update(|cx| tool.run(ToolInput::resolved(input), event_stream, cx));

    cx.run_until_parked();

    let event = rx.try_recv();
    assert!(
        !matches!(event, Ok(Ok(ThreadEvent::ToolCallAuthorization(_)))),
        "expected no authorization request for allowed docs.rs URL"
    );
}
