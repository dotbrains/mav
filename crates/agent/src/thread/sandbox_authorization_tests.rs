use super::*;
use gpui::TestAppContext;

#[gpui::test]
async fn test_authorize_sandbox_allow_always_does_not_cache_thread_grant(cx: &mut TestAppContext) {
    crate::tests::init_test(cx);

    let (event_stream, mut receiver) = ToolCallEventStream::test();
    let request = SandboxRequest {
        network: crate::sandboxing::NetworkRequest::None,
        allow_git_access: false,
        allow_fs_write_all: false,
        unsandboxed: false,
        write_paths: vec![
            PathBuf::from("/tmp/build"),
            PathBuf::from("/tmp/cache"),
            PathBuf::from("/tmp/logs"),
            PathBuf::from("/tmp/secret"),
        ],
    };

    let authorize = cx.update(|cx| {
        event_stream.authorize_sandbox(
            request.clone(),
            "needs to write build artifacts".to_string(),
            cx,
        )
    });
    let authorization = receiver.expect_authorization().await;
    let details =
        acp_thread::sandbox_authorization_details_from_meta(&authorization.tool_call.meta)
            .expect("sandbox authorization should include request details");
    assert!(details.network_hosts.is_empty());
    assert!(!details.network_all_hosts);
    assert_eq!(details.allow_git_access, request.allow_git_access);
    assert_eq!(details.allow_fs_write_all, request.allow_fs_write_all);
    assert_eq!(details.unsandboxed, request.unsandboxed);
    assert_eq!(details.write_paths, request.write_paths);
    assert!(authorization.tool_call.fields.content.is_none());

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

    let send_result = authorization
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow_always"),
            acp::PermissionOptionKind::AllowAlways,
        ));
    assert!(send_result.is_ok());
    authorize.await.unwrap();

    let effective = event_stream.effective_sandbox_request(
        &SandboxRequest::default(),
        &agent_settings::SandboxPermissions::default(),
    );
    assert!(
        effective.write_paths.is_empty(),
        "allow always should not record an in-memory thread grant: {:?}",
        effective.write_paths
    );
}

#[cfg(target_os = "linux")]
#[gpui::test]
async fn test_authorize_sandbox_fallback_options_and_details(cx: &mut TestAppContext) {
    crate::tests::init_test(cx);

    let (event_stream, mut receiver) = ToolCallEventStream::test();
    let authorize = cx.update(|cx| {
        event_stream.authorize_sandbox_fallback(
            Some("cargo build".to_string()),
            "bwrap not found on PATH".to_string(),
            0,
            cx,
        )
    });
    let authorization = receiver.expect_authorization().await;
    let details =
        acp_thread::sandbox_fallback_authorization_details_from_meta(&authorization.tool_call.meta)
            .expect("fallback authorization should include details");
    assert_eq!(details.command.as_deref(), Some("cargo build"));
    assert_eq!(details.reason, "bwrap not found on PATH");

    let acp_thread::PermissionOptions::Flat(options) = &authorization.options else {
        panic!("expected flat fallback permission options");
    };
    let options = options
        .iter()
        .map(|option| (option.option_id.0.as_ref(), option.name.as_ref()))
        .collect::<Vec<_>>();
    assert_eq!(
        options,
        vec![
            ("retry", "Retry"),
            ("allow", "Run without sandbox once"),
            ("allow_thread", "Run without sandbox for this thread"),
            ("allow_always", "Always run without sandbox"),
            ("deny", "Deny"),
        ]
    );

    authorization
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new(acp_thread::SANDBOX_FALLBACK_RETRY_OPTION_ID),
            acp::PermissionOptionKind::RejectAlways,
        ))
        .unwrap();
    assert_eq!(authorize.await.unwrap(), SandboxFallbackDecision::Retry);
}

#[cfg(target_os = "linux")]
#[gpui::test]
async fn test_authorize_sandbox_fallback_retry_label_counts_attempts(cx: &mut TestAppContext) {
    crate::tests::init_test(cx);

    async fn retry_label(cx: &mut TestAppContext, retries: usize) -> String {
        let (event_stream, mut receiver) = ToolCallEventStream::test();
        let authorize = cx.update(|cx| {
            event_stream.authorize_sandbox_fallback(None, "probe failed".to_string(), retries, cx)
        });
        let authorization = receiver.expect_authorization().await;
        let acp_thread::PermissionOptions::Flat(options) = &authorization.options else {
            panic!("expected flat fallback permission options");
        };
        let label = options
            .iter()
            .find(|option| {
                option.option_id.0.as_ref() == acp_thread::SANDBOX_FALLBACK_RETRY_OPTION_ID
            })
            .expect("retry option present")
            .name
            .to_string();
        authorization
            .response
            .send(acp_thread::SelectedPermissionOutcome::new(
                acp::PermissionOptionId::new(acp_thread::SANDBOX_FALLBACK_RETRY_OPTION_ID),
                acp::PermissionOptionKind::RejectAlways,
            ))
            .unwrap();
        authorize.await.unwrap();
        label
    }

    assert_eq!(retry_label(cx, 0).await, "Retry");
    assert_eq!(retry_label(cx, 1).await, "Retry (attempt 1)");
    assert_eq!(retry_label(cx, 2).await, "Retry (attempt 2)");
}

#[cfg(target_os = "linux")]
#[gpui::test]
async fn test_authorize_sandbox_fallback_allow_thread_records_grant(cx: &mut TestAppContext) {
    crate::tests::init_test(cx);

    let (event_stream, mut receiver) = ToolCallEventStream::test();
    assert!(!event_stream.sandbox_fallback_granted_for_thread());

    let authorize = cx.update(|cx| {
        event_stream.authorize_sandbox_fallback(
            Some("cargo build".to_string()),
            "user namespaces are disabled".to_string(),
            0,
            cx,
        )
    });
    let authorization = receiver.expect_authorization().await;
    authorization
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new(acp_thread::SandboxPermission::AllowThread.as_id()),
            acp::PermissionOptionKind::AllowAlways,
        ))
        .unwrap();
    assert_eq!(
        authorize.await.unwrap(),
        SandboxFallbackDecision::RunUnsandboxed
    );

    assert!(event_stream.sandbox_fallback_granted_for_thread());
}

#[cfg(target_os = "linux")]
#[gpui::test]
async fn test_authorize_sandbox_fallback_deny(cx: &mut TestAppContext) {
    crate::tests::init_test(cx);

    let (event_stream, mut receiver) = ToolCallEventStream::test();
    let authorize = cx.update(|cx| {
        event_stream.authorize_sandbox_fallback(None, "bwrap probe failed".to_string(), 0, cx)
    });
    let authorization = receiver.expect_authorization().await;
    authorization
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new(acp_thread::SandboxPermission::Deny.as_id()),
            acp::PermissionOptionKind::RejectOnce,
        ))
        .unwrap();
    assert_eq!(authorize.await.unwrap(), SandboxFallbackDecision::Deny);
    assert!(!event_stream.sandbox_fallback_granted_for_thread());
}

#[test]
fn test_auto_resolve_permission_outcome_uses_once_only_options() {
    let options = acp_thread::PermissionOptions::Dropdown(vec![
        acp_thread::PermissionOptionChoice {
            allow: acp::PermissionOption::new(
                acp::PermissionOptionId::new("always_allow:test_tool"),
                "Always allow",
                acp::PermissionOptionKind::AllowAlways,
            ),
            deny: acp::PermissionOption::new(
                acp::PermissionOptionId::new("always_deny:test_tool"),
                "Always deny",
                acp::PermissionOptionKind::RejectAlways,
            ),
            sub_patterns: vec![],
        },
        acp_thread::PermissionOptionChoice {
            allow: acp::PermissionOption::new(
                acp::PermissionOptionId::new("allow"),
                "Allow once",
                acp::PermissionOptionKind::AllowOnce,
            ),
            deny: acp::PermissionOption::new(
                acp::PermissionOptionId::new("deny"),
                "Deny once",
                acp::PermissionOptionKind::RejectOnce,
            ),
            sub_patterns: vec![],
        },
    ]);

    let allow = auto_resolve_permission_outcome(&options, true)
        .expect("allow auto-resolve should use once-only option");
    assert_eq!(allow.option_id, acp::PermissionOptionId::new("allow"));
    assert_eq!(allow.option_kind, acp::PermissionOptionKind::AllowOnce);

    let deny = auto_resolve_permission_outcome(&options, false)
        .expect("deny auto-resolve should use once-only option");
    assert_eq!(deny.option_id, acp::PermissionOptionId::new("deny"));
    assert_eq!(deny.option_kind, acp::PermissionOptionKind::RejectOnce);
}
