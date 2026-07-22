use super::*;

#[cfg(not(windows))]
#[gpui::test]
async fn startup_returns_error_when_agent_exits_before_initialization(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let store = settings::SettingsStore::test(cx);
        cx.set_global(store);
    });
    cx.executor().allow_parking();

    let temp_dir = tempfile::tempdir().unwrap();
    let project = project::Project::example([temp_dir.path()], &mut cx.to_async()).await;
    let agent_server_store =
        project.read_with(cx, |project, _| project.agent_server_store().downgrade());
    let command = AgentServerCommand {
            path: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                r#"printf '%s\n' 'npm error code ETARGET' 'npm error notarget No matching version found for @agentclientprotocol/claude-agent-acp@0.32.0 with a date before 4/28/2026, 12:11:38 PM.' >&2; exit 1"#.into(),
            ],
            env: None,
        };

    let mut async_cx = cx.to_async();
    let startup = AcpConnection::stdio(
        AgentId::new("test-agent"),
        project,
        command,
        agent_server_store,
        None,
        HashMap::default(),
        &mut async_cx,
    )
    .fuse();
    let timeout = cx
        .background_executor
        .timer(std::time::Duration::from_secs(5))
        .fuse();
    futures::pin_mut!(startup, timeout);

    let result = futures::select! {
        result = startup => result,
        _ = timeout => panic!("timed out waiting for failed ACP startup"),
    };

    let Err(error) = result else {
        panic!("expected ACP startup to fail");
    };
    let load_error = error
        .downcast::<LoadError>()
        .expect("startup failure should preserve the typed load error");
    match load_error {
        LoadError::Exited { status, .. } => {
            assert!(!status.success(), "expected non-zero exit status");
        }
        error => panic!("expected exited load error, got: {error:?}"),
    };
}
