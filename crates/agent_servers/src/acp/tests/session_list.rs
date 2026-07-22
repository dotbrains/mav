use super::*;

#[gpui::test]
async fn session_list_includes_additional_directories_in_work_dirs(cx: &mut gpui::TestAppContext) {
    let connection = connect_session_list_test_agent(
        vec![
            acp::SessionInfo::new("session-1", "/workspace-b").additional_directories(vec![
                std::path::PathBuf::from("/workspace-a"),
                std::path::PathBuf::from("/workspace-b"),
                std::path::PathBuf::from("/workspace-a"),
                std::path::PathBuf::from("/workspace-c"),
            ]),
        ],
        cx,
    )
    .await;
    let session_list = AcpSessionList::new(connection, false);

    let response = cx
        .update(|cx| session_list.list_sessions(AgentSessionListRequest::default(), cx))
        .await
        .expect("session list should load");
    let session = response
        .sessions
        .first()
        .expect("session list should include the returned session");
    let work_dirs = session
        .work_dirs
        .as_ref()
        .expect("session should include work dirs");

    assert_eq!(
        work_dirs.ordered_paths().cloned().collect::<Vec<_>>(),
        vec![
            std::path::PathBuf::from("/workspace-b"),
            std::path::PathBuf::from("/workspace-a"),
            std::path::PathBuf::from("/workspace-c"),
        ]
    );
}

async fn connect_session_list_test_agent(
    sessions: Vec<acp::SessionInfo>,
    cx: &mut gpui::TestAppContext,
) -> ConnectionTo<Agent> {
    let (client_transport, agent_transport) = agent_client_protocol::Channel::duplex();
    let sessions = Arc::new(sessions);

    cx.background_spawn(
        Agent
            .builder()
            .name("list-test-agent")
            .on_receive_request(
                {
                    let sessions = sessions.clone();
                    async move |_request: acp::ListSessionsRequest, responder, _cx| {
                        responder.respond(acp::ListSessionsResponse::new((*sessions).clone()))
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(agent_transport),
    )
    .detach();

    let (connection_tx, connection_rx) = futures::channel::oneshot::channel();
    cx.background_spawn(Client.builder().name("list-test-client").connect_with(
        client_transport,
        move |connection: ConnectionTo<Agent>| async move {
            connection_tx.send(connection).ok();
            futures::future::pending::<Result<(), acp::Error>>().await
        },
    ))
    .detach();

    connection_rx
        .await
        .expect("failed to receive ACP connection")
}

#[gpui::test]
async fn additional_directories_support_respects_agent_capability(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let store = settings::SettingsStore::test(cx);
        cx.set_global(store);
    });

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree("/", serde_json::json!({ "a": {}, "b": {} }))
        .await;
    let project = project::Project::test(fs, [std::path::Path::new("/a")], cx).await;
    let mut harness = test_support::connect_fake_acp_connection(project, cx).await;

    let work_dirs = PathList::new(&[
        std::path::PathBuf::from("/workspace-b"),
        std::path::PathBuf::from("/workspace-a"),
    ]);

    let missing_capability = harness
        .connection
        .session_directories_from_work_dirs(&work_dirs)
        .expect("work dirs should convert");
    assert!(missing_capability.additional_directories.is_empty());

    Rc::get_mut(&mut harness.connection)
        .expect("test harness should own the only ACP connection handle")
        .agent_capabilities
        .session_capabilities
        .additional_directories = Some(acp::SessionAdditionalDirectoriesCapabilities::new());

    let supported = harness
        .connection
        .session_directories_from_work_dirs(&work_dirs)
        .expect("work dirs should convert");
    assert_eq!(
        supported,
        SessionDirectories {
            cwd: std::path::PathBuf::from("/workspace-b"),
            additional_directories: vec![std::path::PathBuf::from("/workspace-a")],
        }
    );
}
