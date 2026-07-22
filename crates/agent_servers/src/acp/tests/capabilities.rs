use super::*;

#[test]
fn cursor_client_capabilities_include_parameterized_model_picker_meta() {
    let capabilities = client_capabilities_for_agent(&AgentId::new(CURSOR_ID), false);
    let meta = capabilities
        .meta
        .expect("expected client capabilities meta");

    assert_eq!(
        meta.get(PARAMETERIMAV_MODEL_PICKER_META_KEY),
        Some(&serde_json::json!(true))
    );
    assert_eq!(meta.get("terminal_output"), Some(&serde_json::json!(true)));
    assert_eq!(meta.get("terminal-auth"), Some(&serde_json::json!(true)));
}

#[test]
fn non_cursor_client_capabilities_do_not_include_parameterized_model_picker_meta() {
    let capabilities = client_capabilities_for_agent(&AgentId::new("codex-acp"), false);
    let meta = capabilities
        .meta
        .expect("expected client capabilities meta");

    assert!(!meta.contains_key(PARAMETERIMAV_MODEL_PICKER_META_KEY));
}

#[test]
fn client_capabilities_include_boolean_config_options_when_supported() {
    let capabilities = client_capabilities_for_agent(&AgentId::new("codex-acp"), true);

    assert!(
        capabilities
            .session
            .and_then(|session| session.config_options)
            .and_then(|config_options| config_options.boolean)
            .is_some()
    );
}

#[test]
fn client_capabilities_omit_boolean_config_options_when_unsupported() {
    let capabilities = client_capabilities_for_agent(&AgentId::new("codex-acp"), false);

    assert!(capabilities.session.is_none());
}

#[test]
fn terminal_auth_task_builds_spawn_from_prebuilt_command() {
    let command = AgentServerCommand {
        path: "/path/to/agent".into(),
        args: vec!["--acp".into(), "--verbose".into(), "/auth".into()],
        env: Some(HashMap::from_iter([
            ("BASE".into(), "1".into()),
            ("SHARED".into(), "override".into()),
            ("EXTRA".into(), "2".into()),
        ])),
    };
    let method = acp::AuthMethodTerminal::new("login", "Login");

    let task = terminal_auth_task(&command, &AgentId::new("test-agent"), &method);

    assert_eq!(task.command.as_deref(), Some("/path/to/agent"));
    assert_eq!(task.args, vec!["--acp", "--verbose", "/auth"]);
    assert_eq!(
        task.env,
        HashMap::from_iter([
            ("BASE".into(), "1".into()),
            ("SHARED".into(), "override".into()),
            ("EXTRA".into(), "2".into()),
        ])
    );
    assert_eq!(task.label, "Login");
    assert_eq!(task.command_label, "Login");
}

#[test]
fn legacy_terminal_auth_task_parses_meta_and_retries_session() {
    let method_id = acp::AuthMethodId::new("legacy-login");
    let method = acp::AuthMethod::Agent(
        acp::AuthMethodAgent::new(method_id.clone(), "Login").meta(acp::Meta::from_iter([(
            "terminal-auth".to_string(),
            serde_json::json!({
                "label": "legacy /auth",
                "command": "legacy-agent",
                "args": ["auth", "--interactive"],
                "env": {
                    "AUTH_MODE": "interactive",
                },
            }),
        )])),
    );

    let task = meta_terminal_auth_task(&AgentId::new("test-agent"), &method_id, &method)
        .expect("expected legacy terminal auth task");

    assert_eq!(task.id.0, "external-agent-test-agent-legacy-login-login");
    assert_eq!(task.command.as_deref(), Some("legacy-agent"));
    assert_eq!(task.args, vec!["auth", "--interactive"]);
    assert_eq!(
        task.env,
        HashMap::from_iter([("AUTH_MODE".into(), "interactive".into())])
    );
    assert_eq!(task.label, "legacy /auth");
}

#[test]
fn legacy_terminal_auth_task_returns_none_for_invalid_meta() {
    let method_id = acp::AuthMethodId::new("legacy-login");
    let method = acp::AuthMethod::Agent(
        acp::AuthMethodAgent::new(method_id.clone(), "Login").meta(acp::Meta::from_iter([(
            "terminal-auth".to_string(),
            serde_json::json!({
                "label": "legacy /auth",
            }),
        )])),
    );

    assert!(meta_terminal_auth_task(&AgentId::new("test-agent"), &method_id, &method).is_none());
}

#[test]
fn first_class_terminal_auth_takes_precedence_over_legacy_meta() {
    let method_id = acp::AuthMethodId::new("login");
    let method = acp::AuthMethod::Terminal(
        acp::AuthMethodTerminal::new(method_id, "Login")
            .args(vec!["/auth".into()])
            .env(std::collections::HashMap::from_iter([(
                "AUTH_MODE".into(),
                "first-class".into(),
            )]))
            .meta(acp::Meta::from_iter([(
                "terminal-auth".to_string(),
                serde_json::json!({
                    "label": "legacy /auth",
                    "command": "legacy-agent",
                    "args": ["legacy-auth"],
                    "env": {
                        "AUTH_MODE": "legacy",
                    },
                }),
            )])),
    );

    let command = AgentServerCommand {
        path: "/path/to/agent".into(),
        args: vec!["--acp".into(), "/auth".into()],
        env: Some(HashMap::from_iter([
            ("BASE".into(), "1".into()),
            ("AUTH_MODE".into(), "first-class".into()),
        ])),
    };

    let task = match &method {
        acp::AuthMethod::Terminal(terminal) => {
            terminal_auth_task(&command, &AgentId::new("test-agent"), terminal)
        }
        _ => unreachable!(),
    };

    assert_eq!(task.command.as_deref(), Some("/path/to/agent"));
    assert_eq!(task.args, vec!["--acp", "/auth"]);
    assert_eq!(
        task.env,
        HashMap::from_iter([
            ("BASE".into(), "1".into()),
            ("AUTH_MODE".into(), "first-class".into()),
        ])
    );
    assert_eq!(task.label, "Login");
}

#[test]
fn trailing_stderr_only_uses_final_stderr_block() {
    let debug_log = AcpDebugLog::default();
    debug_log.record_line(AcpDebugMessageDirection::Stderr, "stale stderr");
    debug_log.record_line(
        AcpDebugMessageDirection::Incoming,
        r#"{"method":"initialized"}"#,
    );

    assert_eq!(debug_log.trailing_stderr(), None);

    debug_log.record_line(AcpDebugMessageDirection::Stderr, "recent stderr");
    assert_eq!(
        debug_log.trailing_stderr().as_deref(),
        Some("recent stderr")
    );
}

#[test]
fn session_directories_use_ordered_paths_when_supported() {
    let work_dirs = PathList::new(&[
        std::path::PathBuf::from("/workspace-b"),
        std::path::PathBuf::from("/workspace-a"),
        std::path::PathBuf::from("/workspace-c"),
    ]);

    let directories =
        session_directories_from_work_dirs(&work_dirs, true).expect("work dirs should convert");

    assert_eq!(
        directories,
        SessionDirectories {
            cwd: std::path::PathBuf::from("/workspace-b"),
            additional_directories: vec![
                std::path::PathBuf::from("/workspace-a"),
                std::path::PathBuf::from("/workspace-c")
            ],
        }
    );

    let session_id = acp::SessionId::new("session-1");
    let new_session_request = directories.clone().into_new_session_request(Vec::new());
    let load_session_request = directories
        .clone()
        .into_load_session_request(session_id.clone(), Vec::new());
    let resume_session_request = directories.into_resume_session_request(session_id, Vec::new());

    assert_eq!(
        new_session_request.cwd,
        std::path::PathBuf::from("/workspace-b")
    );
    assert_eq!(
        new_session_request.additional_directories,
        vec![
            std::path::PathBuf::from("/workspace-a"),
            std::path::PathBuf::from("/workspace-c")
        ]
    );
    assert_eq!(
        load_session_request.additional_directories,
        new_session_request.additional_directories
    );
    assert_eq!(
        resume_session_request.additional_directories,
        new_session_request.additional_directories
    );
}

#[test]
fn session_directories_drop_additional_paths_when_unsupported() {
    let work_dirs = PathList::new(&[
        std::path::PathBuf::from("/workspace-b"),
        std::path::PathBuf::from("/workspace-a"),
    ]);

    let directories =
        session_directories_from_work_dirs(&work_dirs, false).expect("work dirs should convert");

    assert_eq!(
        directories,
        SessionDirectories {
            cwd: std::path::PathBuf::from("/workspace-b"),
            additional_directories: Vec::new(),
        }
    );
}

#[test]
fn session_info_work_dirs_preserve_cwd_then_additional_directories() {
    let work_dirs = work_dirs_from_session_info(
        std::path::PathBuf::from("/workspace-b"),
        vec![
            std::path::PathBuf::from("/workspace-a"),
            std::path::PathBuf::from("/workspace-c"),
        ],
    );

    assert_eq!(
        work_dirs.ordered_paths().cloned().collect::<Vec<_>>(),
        vec![
            std::path::PathBuf::from("/workspace-b"),
            std::path::PathBuf::from("/workspace-a"),
            std::path::PathBuf::from("/workspace-c"),
        ]
    );
}

#[test]
fn session_info_work_dirs_deduplicate_cwd_and_additional_directories() {
    let work_dirs = work_dirs_from_session_info(
        std::path::PathBuf::from("/workspace-b"),
        vec![
            std::path::PathBuf::from("/workspace-a"),
            std::path::PathBuf::from("/workspace-b"),
            std::path::PathBuf::from("/workspace-a"),
            std::path::PathBuf::from("/workspace-c"),
        ],
    );

    assert_eq!(
        work_dirs.ordered_paths().cloned().collect::<Vec<_>>(),
        vec![
            std::path::PathBuf::from("/workspace-b"),
            std::path::PathBuf::from("/workspace-a"),
            std::path::PathBuf::from("/workspace-c"),
        ]
    );
}
