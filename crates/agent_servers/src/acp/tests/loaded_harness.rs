use super::*;

pub(super) async fn connect_fake_agent(
    cx: &mut gpui::TestAppContext,
) -> (
    Rc<AcpConnection>,
    Entity<project::Project>,
    Arc<AtomicUsize>,
    Arc<AtomicUsize>,
    Arc<std::sync::Mutex<Vec<acp::SessionUpdate>>>,
    Arc<std::sync::Mutex<Option<async_channel::Receiver<()>>>>,
    Task<anyhow::Result<()>>,
) {
    cx.update(|cx| {
        let store = settings::SettingsStore::test(cx);
        cx.set_global(store);
    });

    let fs = fs::FakeFs::new(cx.executor());
    fs.insert_tree("/", serde_json::json!({ "a": {} })).await;
    let project = project::Project::test(fs, [std::path::Path::new("/a")], cx).await;

    let load_count = Arc::new(AtomicUsize::new(0));
    let close_count = Arc::new(AtomicUsize::new(0));
    let load_session_updates: Arc<std::sync::Mutex<Vec<acp::SessionUpdate>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let load_session_gate: Arc<std::sync::Mutex<Option<async_channel::Receiver<()>>>> =
        Arc::new(std::sync::Mutex::new(None));

    let (client_transport, agent_transport) = agent_client_protocol::Channel::duplex();

    let sessions: Rc<RefCell<HashMap<acp::SessionId, AcpSession>>> =
        Rc::new(RefCell::new(HashMap::default()));
    let client_session_list: Rc<RefCell<Option<Rc<AcpSessionList>>>> = Rc::new(RefCell::new(None));

    // Build the fake agent side. It handles the requests issued by
    // `AcpConnection` during the test and tracks load/close counts.
    let agent_future = Agent
        .builder()
        .name("fake-agent")
        .on_receive_request(
            async move |req: acp::InitializeRequest, responder, _cx| {
                responder.respond(
                    acp::InitializeResponse::new(req.protocol_version).agent_capabilities(
                        acp::AgentCapabilities::default()
                            .load_session(true)
                            .session_capabilities(
                                acp::SessionCapabilities::default()
                                    .close(acp::SessionCloseCapabilities::new()),
                            ),
                    ),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: acp::AuthenticateRequest, responder, _cx| {
                responder.respond(Default::default())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: acp::NewSessionRequest, responder, _cx| {
                responder.respond(acp::NewSessionResponse::new(acp::SessionId::new("unused")))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: acp::PromptRequest, responder, _cx| {
                responder.respond(acp::PromptResponse::new(acp::StopReason::EndTurn))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let load_count = load_count.clone();
                let load_session_updates = load_session_updates.clone();
                let load_session_gate = load_session_gate.clone();
                async move |req: acp::LoadSessionRequest, responder, cx| {
                    load_count.fetch_add(1, Ordering::SeqCst);

                    // Simulate spec-compliant history replay: send
                    // notifications to the client before responding to the
                    // load request.
                    let updates = std::mem::take(
                        &mut *load_session_updates
                            .lock()
                            .expect("load_session_updates mutex poisoned"),
                    );
                    for update in updates {
                        cx.send_notification(acp::SessionNotification::new(
                            req.session_id.clone(),
                            update,
                        ))?;
                    }

                    // If a gate was installed, park on it before responding
                    // so tests can interleave other work (e.g.
                    // `close_session`) with an in-flight load.
                    let gate = load_session_gate
                        .lock()
                        .expect("load_session_gate mutex poisoned")
                        .take();
                    if let Some(gate) = gate {
                        gate.recv().await.ok();
                    }

                    responder.respond(acp::LoadSessionResponse::new())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let close_count = close_count.clone();
                async move |_req: acp::CloseSessionRequest, responder, _cx| {
                    close_count.fetch_add(1, Ordering::SeqCst);
                    responder.respond(acp::CloseSessionResponse::new())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |_notif: acp::CancelNotification, _cx| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(agent_transport);

    let agent_io_task = cx.background_spawn(agent_future);

    // Wire the production handler set into the fake client so inbound
    // requests/notifications from the fake agent reach the same
    // dispatcher that the real `stdio` path uses.
    let (dispatch_tx, dispatch_rx) = mpsc::unbounded::<ForegroundWork>();

    let (connection_tx, connection_rx) = futures::channel::oneshot::channel();
    let client_future = connect_client_future(
        "mav-test",
        client_transport,
        dispatch_tx.clone(),
        connection_tx,
    );
    let client_io_task = cx.background_spawn(async move {
        client_future.await.ok();
    });

    let client_conn: ConnectionTo<Agent> = connection_rx
        .await
        .expect("failed to receive ACP connection handle");

    let response = client_conn
        .send_request(acp::InitializeRequest::new(ProtocolVersion::V1))
        .block_task()
        .await
        .expect("failed to initialize ACP connection");

    let agent_capabilities = response.agent_capabilities;

    let dispatch_context = ClientContext {
        sessions: sessions.clone(),
        session_list: client_session_list.clone(),
    };
    // `TestAppContext::spawn` hands out an `AsyncApp` by value, whereas the
    // production path uses `Context::spawn` which hands out `&mut AsyncApp`.
    // Bind the value-form to a local and take `&mut` of it to reuse the
    // same dispatch loop shape.
    let dispatch_task = cx.spawn({
        let mut dispatch_rx = dispatch_rx;
        move |cx| async move {
            let mut cx = cx;
            while let Some(work) = dispatch_rx.next().await {
                work.run(&mut cx, &dispatch_context);
            }
        }
    });

    let agent_server_store =
        project.read_with(cx, |project, _| project.agent_server_store().downgrade());

    let connection = cx.update(|cx| {
        AcpConnection::new_for_test(
            client_conn,
            sessions,
            agent_capabilities,
            agent_server_store,
            client_io_task,
            dispatch_task,
            cx,
        )
    });

    let keep_agent_alive = cx.background_spawn(async move {
        agent_io_task.await.ok();
        anyhow::Ok(())
    });

    (
        Rc::new(connection),
        project,
        load_count,
        close_count,
        load_session_updates,
        load_session_gate,
        keep_agent_alive,
    )
}
