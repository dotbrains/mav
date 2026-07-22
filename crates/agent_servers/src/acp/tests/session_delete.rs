use super::*;

async fn connect_session_delete_test_agent(
    deleted_sessions: Arc<std::sync::Mutex<Vec<acp::SessionId>>>,
    cx: &mut gpui::TestAppContext,
) -> ConnectionTo<Agent> {
    let (client_transport, agent_transport) = agent_client_protocol::Channel::duplex();

    cx.background_spawn(
        Agent
            .builder()
            .name("delete-test-agent")
            .on_receive_request(
                {
                    let deleted_sessions = deleted_sessions.clone();
                    async move |request: acp::DeleteSessionRequest, responder, _cx| {
                        deleted_sessions
                            .lock()
                            .expect("deleted sessions lock should not be poisoned")
                            .push(request.session_id);
                        responder.respond(acp::DeleteSessionResponse::default())
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(agent_transport),
    )
    .detach();

    let (connection_tx, connection_rx) = futures::channel::oneshot::channel();
    cx.background_spawn(Client.builder().name("delete-test-client").connect_with(
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
async fn session_list_delete_sends_session_delete_when_supported(cx: &mut gpui::TestAppContext) {
    let deleted_sessions = Arc::new(std::sync::Mutex::new(Vec::new()));
    let connection = connect_session_delete_test_agent(deleted_sessions.clone(), cx).await;
    let session_list = AcpSessionList::new(connection, true);
    let session_id = acp::SessionId::new("session-to-delete");

    cx.update(|cx| session_list.delete_session(&session_id, cx))
        .await
        .expect("delete_session failed");

    assert_eq!(
        *deleted_sessions
            .lock()
            .expect("deleted sessions lock should not be poisoned"),
        vec![session_id]
    );
}

#[gpui::test]
async fn session_list_delete_does_not_send_when_unsupported(cx: &mut gpui::TestAppContext) {
    let deleted_sessions = Arc::new(std::sync::Mutex::new(Vec::new()));
    let connection = connect_session_delete_test_agent(deleted_sessions.clone(), cx).await;
    let session_list = AcpSessionList::new(connection, false);
    let session_id = acp::SessionId::new("session-to-delete");

    let error = cx
        .update(|cx| session_list.delete_session(&session_id, cx))
        .await
        .expect_err("delete_session should fail when unsupported");

    assert!(
        error.to_string().contains("delete_session not supported"),
        "unexpected error: {error}"
    );
    assert!(
        deleted_sessions
            .lock()
            .expect("deleted sessions lock should not be poisoned")
            .is_empty()
    );
}
