    #[test]
    fn test_input_schema_includes_sandbox_flags() {
        // The sandboxed terminal tool advertises these fields so the model can
        // request escalations when the sandbox is in effect. Guard against
        // accidentally renaming or removing them.
        let schema = serde_json::to_string(&schemars::schema_for!(SandboxedTerminalToolInput))
            .expect("input schema should serialize");
        assert!(
            schema.contains("allow_hosts"),
            "schema should advertise allow_hosts: {schema}"
        );
        assert!(
            schema.contains("allow_all_hosts"),
            "schema should advertise allow_all_hosts: {schema}"
        );
        assert!(
            schema.contains("fs_write_paths"),
            "schema should advertise fs_write_paths: {schema}"
        );
        assert!(
            schema.contains("allow_fs_write_all"),
            "schema should advertise allow_fs_write_all: {schema}"
        );
        assert!(
            schema.contains("unsandboxed"),
            "schema should advertise unsandboxed: {schema}"
        );
    }

    #[test]
    fn test_sandbox_flags_default_to_none_when_absent() {
        // The model is expected to omit the sandbox fields entirely on most
        // calls. Make sure deserialization doesn't reject the minimal
        // payload and that the fields default to empty/`None` (which the tool
        // interprets as "no escalation requested").
        let input: SandboxedTerminalToolInput = serde_json::from_value(serde_json::json!({
            "command": "echo hi",
            "cd": ".",
        }))
        .expect("minimal input should deserialize");
        assert!(input.allow_hosts.is_empty());
        assert_eq!(input.allow_all_hosts, None);
        assert!(input.fs_write_paths.is_empty());
        assert_eq!(input.allow_fs_write_all, None);
        assert_eq!(input.unsandboxed, None);
    }

    #[test]
    fn test_legacy_allow_fs_write_aliases_to_allow_fs_write_all() {
        let input: SandboxedTerminalToolInput = serde_json::from_value(serde_json::json!({
            "command": "echo hi",
            "cd": ".",
            "allow_fs_write": true,
        }))
        .expect("legacy allow_fs_write should deserialize");

        assert_eq!(input.allow_fs_write_all, Some(true));
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    async fn test_legacy_allow_fs_write_uses_sandbox_permission_options(
        cx: &mut gpui::TestAppContext,
    ) {
        use feature_flags::FeatureFlagAppExt as _;

        crate::tests::init_test(cx);
        cx.update(|cx| {
            cx.update_flags(true, vec!["sandboxing".to_string()]);
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0),
            )
        }));
        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(SandboxedTerminalTool::new(project, environment.clone()));
        let (event_stream, mut receiver) = crate::ToolCallEventStream::test();
        let input: SandboxedTerminalToolInput = serde_json::from_value(serde_json::json!({
            "command": "echo hi",
            "cd": "root",
            "allow_fs_write": true,
            "reason": "needs to write outside the project",
        }))
        .expect("legacy allow_fs_write should deserialize");

        let task = cx.update(|cx| tool.run(crate::ToolInput::resolved(input), event_stream, cx));

        let authorization = receiver.expect_authorization().await;
        let details =
            acp_thread::sandbox_authorization_details_from_meta(&authorization.tool_call.meta)
                .expect("legacy allow_fs_write should request sandbox authorization details");
        assert!(details.network_hosts.is_empty());
        assert!(!details.network_all_hosts);
        assert!(details.allow_fs_write_all);
        assert!(!details.unsandboxed);
        assert!(details.write_paths.is_empty());

        let acp_thread::PermissionOptions::Flat(options) = &authorization.options else {
            panic!("expected flat sandbox permission options");
        };
        let options = options
            .iter()
            .map(|option| {
                (
                    option.option_id.0.as_ref(),
                    option.name.as_ref(),
                    option.kind,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            options,
            vec![
                ("allow", "Allow once", acp::PermissionOptionKind::AllowOnce),
                (
                    "allow_thread",
                    "Allow for this thread",
                    acp::PermissionOptionKind::AllowAlways,
                ),
                (
                    "allow_always",
                    "Allow always",
                    acp::PermissionOptionKind::AllowAlways,
                ),
                ("deny", "Deny", acp::PermissionOptionKind::RejectOnce),
            ]
        );

        authorization
            .response
            .send(acp_thread::SelectedPermissionOutcome::new(
                acp::PermissionOptionId::new("deny"),
                acp::PermissionOptionKind::RejectOnce,
            ))
            .expect("authorization response should send");

        let result = task
            .await
            .expect("denied sandbox request returns model-readable output");
        assert!(result.contains("user denied the requested sandbox permissions"));
        assert_eq!(environment.terminal_creation_count(), 0);
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    async fn test_unsandboxed_uses_sandbox_permission_options(cx: &mut gpui::TestAppContext) {
        use feature_flags::FeatureFlagAppExt as _;

        crate::tests::init_test(cx);
        cx.update(|cx| {
            cx.update_flags(true, vec!["sandboxing".to_string()]);
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0),
            )
        }));
        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(SandboxedTerminalTool::new(project, environment.clone()));
        let (event_stream, mut receiver) = crate::ToolCallEventStream::test();
        let input: SandboxedTerminalToolInput = serde_json::from_value(serde_json::json!({
            "command": "echo hi",
            "cd": "root",
            "allow_all_hosts": true,
            "allow_fs_write_all": true,
            "unsandboxed": true,
            "reason": "needs full access for this task",
        }))
        .expect("unsandboxed input should deserialize");

        let task = cx.update(|cx| tool.run(crate::ToolInput::resolved(input), event_stream, cx));

        let authorization = receiver.expect_authorization().await;
        // The sandbox approval deliberately leaves the tool-call title untouched
        // so the card keeps showing the command being approved.
        assert_eq!(authorization.tool_call.fields.title, None);
        let details =
            acp_thread::sandbox_authorization_details_from_meta(&authorization.tool_call.meta)
                .expect("unsandboxed should request sandbox authorization details");
        assert!(details.network_hosts.is_empty());
        assert!(!details.network_all_hosts);
        assert!(!details.allow_fs_write_all);
        assert!(details.unsandboxed);
        assert!(details.write_paths.is_empty());

        let acp_thread::PermissionOptions::Flat(options) = &authorization.options else {
            panic!("expected flat sandbox permission options");
        };
        let options = options
            .iter()
            .map(|option| {
                (
                    option.option_id.0.as_ref(),
                    option.name.as_ref(),
                    option.kind,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            options,
            vec![
                ("allow", "Allow once", acp::PermissionOptionKind::AllowOnce),
                (
                    "allow_thread",
                    "Allow for this thread",
                    acp::PermissionOptionKind::AllowAlways,
                ),
                (
                    "allow_always",
                    "Allow always",
                    acp::PermissionOptionKind::AllowAlways,
                ),
                ("deny", "Deny", acp::PermissionOptionKind::RejectOnce),
            ]
        );

        authorization
            .response
            .send(acp_thread::SelectedPermissionOutcome::new(
                acp::PermissionOptionId::new("deny"),
                acp::PermissionOptionKind::RejectOnce,
            ))
            .expect("authorization response should send");

        let result = task
            .await
            .expect("denied sandbox request returns model-readable output");
        assert!(result.contains("user denied permission to run outside the sandbox"));
        assert_eq!(environment.terminal_creation_count(), 0);
    }

    /// Regression test: choosing "Allow always" on a sandbox prompt must persist
    /// the grant to settings *only* — it must not also cache an in-memory thread
    /// grant. Otherwise removing the entry from settings.json wouldn't revoke it
    /// within the same conversation, and a later identical command would run
    /// without prompting again (the bug this guards against).
    #[cfg(target_os = "macos")]
    #[gpui::test]
    async fn test_allow_always_grant_is_revocable_via_settings(cx: &mut gpui::TestAppContext) {
        use feature_flags::FeatureFlagAppExt as _;

        crate::tests::init_test(cx);
        // Auto-allow the terminal tool itself so only the *sandbox* escalation
        // prompts, and start with no persisted sandbox grants (mirroring a
        // settings.json that doesn't grant the path — e.g. after the user
        // removed it).
        cx.update(|cx| {
            cx.update_flags(true, vec!["sandboxing".to_string()]);
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            settings.sandbox_permissions = agent_settings::SandboxPermissions::default();
            agent_settings::AgentSettings::override_global(settings, cx);
        });

        let fs = fs::FakeFs::new(cx.executor());
        fs.insert_tree("/root", serde_json::json!({})).await;
        let project = project::Project::test(fs, ["/root".as_ref()], cx).await;

        // Both tool calls belong to the same conversation, so they share one set
        // of in-memory thread sandbox grants, exactly like a real `Thread`.
        let sandbox_grants = std::rc::Rc::new(std::cell::RefCell::new(
            crate::sandboxing::ThreadSandboxGrants::default(),
        ));

        let input = serde_json::json!({
            "command": "touch build/output",
            "cd": "root",
            "fs_write_paths": ["build"],
            "reason": "needs to write build artifacts",
        });

        // ---- First call: the user picks "Allow always".
        let environment = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0),
            )
        }));
        #[allow(clippy::arc_with_non_send_sync)]
        let tool = std::sync::Arc::new(SandboxedTerminalTool::new(
            project.clone(),
            environment.clone(),
        ));
        let (event_stream, mut receiver) =
            crate::ToolCallEventStream::test_with_grants(sandbox_grants.clone());
        let resolved: SandboxedTerminalToolInput = serde_json::from_value(input.clone()).unwrap();
        let task = cx.update(|cx| tool.run(crate::ToolInput::resolved(resolved), event_stream, cx));

        let authorization = receiver.expect_authorization().await;
        authorization
            .response
            .send(acp_thread::SelectedPermissionOutcome::new(
                acp::PermissionOptionId::new("allow_always"),
                acp::PermissionOptionKind::AllowAlways,
            ))
            .expect("authorization response should send");
        task.await.expect("granted command should run");
        assert_eq!(environment.terminal_creation_count(), 1);

        // The grant must NOT have been cached in the shared thread grants: with
        // empty persistent settings, the thread grants should cover nothing.
        let cached = sandbox_grants.borrow().effective_with_persistent(
            &crate::sandboxing::SandboxRequest::default(),
            &agent_settings::SandboxPermissions::default(),
        );
        assert!(
            cached.write_paths.is_empty(),
            "\"Allow always\" must not cache an in-memory thread grant: {:?}",
            cached.write_paths
        );

        // ---- Second call: the same request, with the path absent from
        // settings, must prompt again instead of being silently allowed.
        let environment2 = std::rc::Rc::new(cx.update(|cx| {
            crate::tests::FakeThreadEnvironment::default().with_terminal(
                crate::tests::FakeTerminalHandle::new_with_immediate_exit(cx, 0),
            )
        }));
        #[allow(clippy::arc_with_non_send_sync)]
        let tool2 = std::sync::Arc::new(SandboxedTerminalTool::new(
            project.clone(),
            environment2.clone(),
        ));
        let (event_stream2, mut receiver2) =
            crate::ToolCallEventStream::test_with_grants(sandbox_grants.clone());
        let resolved2: SandboxedTerminalToolInput = serde_json::from_value(input).unwrap();
        let task2 =
            cx.update(|cx| tool2.run(crate::ToolInput::resolved(resolved2), event_stream2, cx));

        let authorization2 = receiver2.expect_authorization().await;
        let details =
            acp_thread::sandbox_authorization_details_from_meta(&authorization2.tool_call.meta)
                .expect("the identical request should prompt for sandbox authorization again");
        assert!(
            details
                .write_paths
                .iter()
                .any(|path| path.ends_with("build")),
            "re-prompt should request the same write path: {:?}",
            details.write_paths
        );

        authorization2
            .response
            .send(acp_thread::SelectedPermissionOutcome::new(
                acp::PermissionOptionId::new("deny"),
                acp::PermissionOptionKind::RejectOnce,
            ))
            .expect("authorization response should send");
        let result = task2
            .await
            .expect("denied sandbox request returns model-readable output");
    assert!(result.contains("user denied the requested sandbox permissions"));
    assert_eq!(environment2.terminal_creation_count(), 0);
}
