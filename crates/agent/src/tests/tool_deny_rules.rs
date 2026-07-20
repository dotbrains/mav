use super::*;

#[gpui::test]
async fn test_edit_file_tool_deny_rule_blocks_edit(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({"sensitive_config.txt": "secret data"}))
        .await;
    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            EditFileTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Allow),
                always_allow: vec![],
                always_deny: vec![agent_settings::CompiledRegex::new(r"sensitive", false).unwrap()],
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
    let action_log = cx.update(|cx| thread.read(cx).action_log.clone());

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::EditFileTool::new(
        project.clone(),
        thread.downgrade(),
        action_log,
        language_registry,
    ));
    let (event_stream, _rx) = crate::ToolCallEventStream::test();

    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::EditFileToolInput {
                path: "root/sensitive_config.txt".into(),
                edits: vec![],
            }),
            event_stream,
            cx,
        )
    });

    let result = task.await;
    assert!(result.is_err(), "expected edit to be blocked");
    assert!(
        result.unwrap_err().to_string().contains("blocked"),
        "error should mention the edit was blocked"
    );
}

#[gpui::test]
async fn test_delete_path_tool_deny_rule_blocks_deletion(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree("/root", json!({"important_data.txt": "critical info"}))
        .await;
    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            DeletePathTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Allow),
                always_allow: vec![],
                always_deny: vec![agent_settings::CompiledRegex::new(r"important", false).unwrap()],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    let action_log = cx.new(|_cx| action_log::ActionLog::new(project.clone()));

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::DeletePathTool::new(project, action_log));
    let (event_stream, _rx) = crate::ToolCallEventStream::test();

    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::DeletePathToolInput {
                path: "root/important_data.txt".to_string(),
            }),
            event_stream,
            cx,
        )
    });

    let result = task.await;
    assert!(result.is_err(), "expected deletion to be blocked");
    assert!(
        result.unwrap_err().contains("blocked"),
        "error should mention the deletion was blocked"
    );
}

#[gpui::test]
async fn test_move_path_tool_denies_if_destination_denied(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "safe.txt": "content",
            "protected": {}
        }),
    )
    .await;
    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            MovePathTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Allow),
                always_allow: vec![],
                always_deny: vec![agent_settings::CompiledRegex::new(r"protected", false).unwrap()],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::MovePathTool::new(project));
    let (event_stream, _rx) = crate::ToolCallEventStream::test();

    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::MovePathToolInput {
                source_path: "root/safe.txt".to_string(),
                destination_path: "root/protected/safe.txt".to_string(),
            }),
            event_stream,
            cx,
        )
    });

    let result = task.await;
    assert!(
        result.is_err(),
        "expected move to be blocked due to destination path"
    );
    assert!(
        result.unwrap_err().contains("blocked"),
        "error should mention the move was blocked"
    );
}

#[gpui::test]
async fn test_move_path_tool_denies_if_source_denied(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "secret.txt": "secret content",
            "public": {}
        }),
    )
    .await;
    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            MovePathTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Allow),
                always_allow: vec![],
                always_deny: vec![agent_settings::CompiledRegex::new(r"secret", false).unwrap()],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::MovePathTool::new(project));
    let (event_stream, _rx) = crate::ToolCallEventStream::test();

    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::MovePathToolInput {
                source_path: "root/secret.txt".to_string(),
                destination_path: "root/public/not_secret.txt".to_string(),
            }),
            event_stream,
            cx,
        )
    });

    let result = task.await;
    assert!(
        result.is_err(),
        "expected move to be blocked due to source path"
    );
    assert!(
        result.unwrap_err().contains("blocked"),
        "error should mention the move was blocked"
    );
}

#[gpui::test]
async fn test_copy_path_tool_deny_rule_blocks_copy(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        "/root",
        json!({
            "confidential.txt": "confidential data",
            "dest": {}
        }),
    )
    .await;
    let project = Project::test(fs.clone(), ["/root".as_ref()], cx).await;

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            CopyPathTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Allow),
                always_allow: vec![],
                always_deny: vec![
                    agent_settings::CompiledRegex::new(r"confidential", false).unwrap(),
                ],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::CopyPathTool::new(project));
    let (event_stream, _rx) = crate::ToolCallEventStream::test();

    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::CopyPathToolInput {
                source_path: "root/confidential.txt".to_string(),
                destination_path: "root/dest/copy.txt".to_string(),
            }),
            event_stream,
            cx,
        )
    });

    let result = task.await;
    assert!(result.is_err(), "expected copy to be blocked");
    assert!(
        result.unwrap_err().contains("blocked"),
        "error should mention the copy was blocked"
    );
}

#[gpui::test]
async fn test_web_search_tool_deny_rule_blocks_search(cx: &mut TestAppContext) {
    init_test(cx);

    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.tools.insert(
            WebSearchTool::NAME.into(),
            agent_settings::ToolRules {
                default: Some(settings::ToolPermissionMode::Allow),
                always_allow: vec![],
                always_deny: vec![
                    agent_settings::CompiledRegex::new(r"internal\.company", false).unwrap(),
                ],
                always_confirm: vec![],
                invalid_patterns: vec![],
            },
        );
        agent_settings::AgentSettings::override_global(settings, cx);
    });

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::WebSearchTool);
    let (event_stream, _rx) = crate::ToolCallEventStream::test();

    let input: crate::WebSearchToolInput =
        serde_json::from_value(json!({"query": "internal.company.com secrets"})).unwrap();

    let task = cx.update(|cx| tool.run(ToolInput::resolved(input), event_stream, cx));

    let result = task.await;
    assert!(result.is_err(), "expected search to be blocked");
    match result.unwrap_err() {
        crate::WebSearchToolOutput::Error { error } => {
            assert!(
                error.contains("blocked"),
                "error should mention the search was blocked"
            );
        }
        other => panic!("expected Error variant, got: {other:?}"),
    }
}
