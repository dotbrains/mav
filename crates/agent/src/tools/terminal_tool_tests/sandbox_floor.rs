    /// Set up a sandboxing-enabled, auto-allowing project for the floor-
    /// enforcement tests, with the given persistent settings and thread grants.
    async fn floor_test_tool(
        cx: &mut gpui::TestAppContext,
        persistent: agent_settings::SandboxPermissions,
        grants: crate::sandboxing::ThreadSandboxGrants,
    ) -> (
        std::sync::Arc<SandboxedTerminalTool>,
        crate::ToolCallEventStream,
        crate::ToolCallEventStreamReceiver,
        std::rc::Rc<crate::tests::FakeThreadEnvironment>,
    ) {
        use feature_flags::FeatureFlagAppExt as _;

        crate::tests::init_test(cx);
        cx.update(|cx| {
            cx.update_flags(true, vec!["sandboxing".to_string()]);
            let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
            settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
            settings.tool_permissions.tools.remove(TerminalTool::NAME);
            settings.sandbox_permissions = persistent;
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
        let grants = std::rc::Rc::new(std::cell::RefCell::new(grants));
        let (event_stream, receiver) = crate::ToolCallEventStream::test_with_grants(grants);
        (tool, event_stream, receiver, environment)
    }

    /// A standing "run unsandboxed for this thread" grant makes an ordinary
    /// command (one that requests no escalation) run without a sandbox, and the
    /// model is told so in the output.
    #[gpui::test]
    async fn test_unsandboxed_thread_grant_runs_bare_command_unsandboxed(
        cx: &mut gpui::TestAppContext,
    ) {
        let mut grants = crate::sandboxing::ThreadSandboxGrants::default();
        grants.record(&crate::sandboxing::SandboxRequest {
            unsandboxed: true,
            ..Default::default()
        });
        let (tool, event_stream, _receiver, environment) =
            floor_test_tool(cx, agent_settings::SandboxPermissions::default(), grants).await;

        let input: SandboxedTerminalToolInput = serde_json::from_value(serde_json::json!({
            "command": "echo hi",
            "cd": "root",
        }))
        .unwrap();
        let task = cx.update(|cx| tool.run(crate::ToolInput::resolved(input), event_stream, cx));
        let result = task.await.expect("bare command should run");
        assert_eq!(environment.terminal_creation_count(), 1);
        assert!(
            result.contains("WITHOUT an OS sandbox"),
            "a bare command in an unsandboxed thread must run unsandboxed: {result}"
        );
    }

    /// Once the thread is unsandboxed, the model must not be able to ask for a
    /// scoped sandbox (it would silently run unsandboxed instead) — the call is
    /// rejected so the model fixes its request.
    #[gpui::test]
    async fn test_unsandboxed_thread_grant_rejects_scoping_request(cx: &mut gpui::TestAppContext) {
        let mut grants = crate::sandboxing::ThreadSandboxGrants::default();
        grants.record(&crate::sandboxing::SandboxRequest {
            unsandboxed: true,
            ..Default::default()
        });
        let (tool, event_stream, _receiver, environment) =
            floor_test_tool(cx, agent_settings::SandboxPermissions::default(), grants).await;

        let input: SandboxedTerminalToolInput = serde_json::from_value(serde_json::json!({
            "command": "touch build/out",
            "cd": "root",
            "fs_write_paths": ["build"],
            "allow_all_hosts": true,
            "reason": "write build artifacts",
        }))
        .unwrap();
        let task = cx.update(|cx| tool.run(crate::ToolInput::resolved(input), event_stream, cx));
        let error = task
            .await
            .expect_err("scoping a request in an unsandboxed thread should be rejected");
        assert!(
            error.contains("Sandboxing is disabled for this thread"),
            "unexpected error: {error}"
        );
        // The error must name exactly the fields that have no effect.
        assert!(
            error.contains("`fs_write_paths`") && error.contains("`allow_all_hosts`"),
            "error should name the ineffective fields: {error}"
        );
        assert_eq!(environment.terminal_creation_count(), 0);
    }

    /// A persistent "allow unrestricted filesystem writes" setting makes scoping
    /// writes to specific paths meaningless, so such a request is rejected.
    #[gpui::test]
    async fn test_unrestricted_fs_setting_rejects_scoped_write_paths(
        cx: &mut gpui::TestAppContext,
    ) {
        let persistent = agent_settings::SandboxPermissions {
            allow_fs_write_all: true,
            ..Default::default()
        };
        let (tool, event_stream, _receiver, environment) = floor_test_tool(
            cx,
            persistent,
            crate::sandboxing::ThreadSandboxGrants::default(),
        )
        .await;

        let input: SandboxedTerminalToolInput = serde_json::from_value(serde_json::json!({
            "command": "touch build/out",
            "cd": "root",
            "fs_write_paths": ["build"],
            "reason": "write build artifacts",
        }))
        .unwrap();
        let task = cx.update(|cx| tool.run(crate::ToolInput::resolved(input), event_stream, cx));
        let error = task
            .await
            .expect_err("scoping writes when FS is unrestricted should be rejected");
        assert!(
            error.contains("Unrestricted filesystem writes are enabled for this thread"),
            "unexpected error: {error}"
        );
        assert_eq!(environment.terminal_creation_count(), 0);
    }

    /// A standing "any host" network grant makes scoping to specific hosts
    /// meaningless, so such a request is rejected.
    #[gpui::test]
    async fn test_unrestricted_network_grant_rejects_scoped_hosts(cx: &mut gpui::TestAppContext) {
        let mut grants = crate::sandboxing::ThreadSandboxGrants::default();
        grants.record(&crate::sandboxing::SandboxRequest {
            network: NetworkRequest::AnyHost,
            ..Default::default()
        });
        let (tool, event_stream, _receiver, environment) =
            floor_test_tool(cx, agent_settings::SandboxPermissions::default(), grants).await;

        let input: SandboxedTerminalToolInput = serde_json::from_value(serde_json::json!({
            "command": "curl https://github.com",
            "cd": "root",
            "allow_hosts": ["github.com"],
            "reason": "fetch from github",
        }))
        .unwrap();
        let task = cx.update(|cx| tool.run(crate::ToolInput::resolved(input), event_stream, cx));
        let error = task
            .await
            .expect_err("scoping hosts when network is unrestricted should be rejected");
        assert!(
            error.contains("Unrestricted network access is enabled for this thread"),
            "unexpected error: {error}"
        );
        assert_eq!(environment.terminal_creation_count(), 0);
    }

    fn host_request(list: &[&str]) -> NetworkRequest {
        NetworkRequest::Hosts(
            list.iter()
                .map(|h| http_proxy::HostPattern::parse(h).unwrap())
                .collect(),
        )
    }

    #[test]
    fn test_build_network_request_validates_and_classifies() {
        // No fields -> None.
        assert_eq!(
            build_network_request(&TerminalSandboxInput::default()).unwrap(),
            NetworkRequest::None
        );
        // allow_all_hosts -> AnyHost, even alongside specific hosts.
        assert_eq!(
            build_network_request(&TerminalSandboxInput {
                allow_hosts: vec!["github.com".into()],
                allow_all_hosts: Some(true),
                ..Default::default()
            })
            .unwrap(),
            NetworkRequest::AnyHost
        );
        // Valid hosts parse to patterns.
        assert_eq!(
            build_network_request(&TerminalSandboxInput {
                allow_hosts: vec!["github.com".into(), "*.npmjs.org".into()],
                ..Default::default()
            })
            .unwrap(),
            host_request(&["github.com", "*.npmjs.org"])
        );
        // An IP literal is rejected with an actionable message.
        let err = build_network_request(&TerminalSandboxInput {
            allow_hosts: vec!["127.0.0.1".into()],
            ..Default::default()
        })
        .unwrap_err();
        assert!(err.contains("127.0.0.1"), "unexpected error: {err}");
    }

    #[test]
    fn test_network_request_to_sandbox_network_access_uses_explicit_unrestricted_variant() {
        match network_request_to_sandbox_network_access(&NetworkRequest::None) {
            acp_thread::SandboxNetworkAccess::None => {}
            other => panic!("expected no network access, got {other:?}"),
        }

        match network_request_to_sandbox_network_access(&NetworkRequest::AnyHost) {
            acp_thread::SandboxNetworkAccess::All => {}
            other => panic!("expected unrestricted network access, got {other:?}"),
        }

        // macOS and Linux confine host requests through the allowlist proxy.
        match network_request_to_sandbox_network_access(&host_request(&["github.com"])) {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            acp_thread::SandboxNetworkAccess::Restricted(allowlist) => {
                assert!(allowlist.allows("github.com"));
                assert!(!allowlist.allows("example.com"));
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            acp_thread::SandboxNetworkAccess::None => {}
            other => panic!("unexpected network access for host request, got {other:?}"),
        }
    }
