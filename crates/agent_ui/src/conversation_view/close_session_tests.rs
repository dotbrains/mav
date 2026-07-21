use super::tests::*;
use super::*;

#[gpui::test]
async fn test_close_all_sessions_skips_when_unsupported(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());

    let thread_store = cx.update(|_window, cx| cx.new(|cx| ThreadStore::new(cx)));
    let connection_store =
        cx.update(|_window, cx| cx.new(|cx| AgentConnectionStore::new(project.clone(), cx)));

    let conversation_view = cx.update(|window, cx| {
        cx.new(|cx| {
            ConversationView::new(
                Rc::new(StubAgentServer::default_response()),
                connection_store,
                Agent::Custom { id: "Test".into() },
                None,
                None,
                None,
                None,
                None,
                workspace.downgrade(),
                project,
                Some(thread_store),
                AgentThreadSource::AgentPanel,
                window,
                cx,
            )
        })
    });

    cx.run_until_parked();

    conversation_view.read_with(cx, |view, _cx| {
        let connected = view.as_connected().expect("Should be connected");
        assert!(!connected.threads.is_empty());
        assert!(!connected.connection.supports_close_session());
    });

    conversation_view
        .update(cx, |view, cx| {
            view.as_connected()
                .expect("Should be connected")
                .close_all_sessions(cx)
        })
        .await;
}

#[gpui::test]
async fn test_close_all_sessions_calls_close_when_supported(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::new(CloseCapableConnection::new()), cx).await;

    cx.run_until_parked();

    let close_capable = conversation_view.read_with(cx, |view, _cx| {
        let connected = view.as_connected().expect("Should be connected");
        assert!(!connected.threads.is_empty());
        assert!(connected.connection.supports_close_session());
        connected
            .connection
            .clone()
            .into_any()
            .downcast::<CloseCapableConnection>()
            .expect("Should be CloseCapableConnection")
    });

    conversation_view
        .update(cx, |view, cx| {
            view.as_connected()
                .expect("Should be connected")
                .close_all_sessions(cx)
        })
        .await;

    assert!(close_capable.closed_sessions.lock().len() > 0);
}

#[gpui::test]
async fn test_close_session_returns_error_when_unsupported(cx: &mut TestAppContext) {
    init_test(cx);

    let (conversation_view, cx) =
        setup_conversation_view(StubAgentServer::default_response(), cx).await;

    cx.run_until_parked();

    let result = conversation_view
        .update(cx, |view, cx| {
            let connected = view.as_connected().expect("Should be connected");
            assert!(!connected.connection.supports_close_session());
            let thread_view = connected
                .threads
                .values()
                .next()
                .expect("Should have at least one thread");
            let session_id = thread_view.read(cx).thread.read(cx).session_id().clone();
            connected.connection.clone().close_session(&session_id, cx)
        })
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not supported"));
}

#[derive(Clone)]
struct CloseCapableConnection {
    closed_sessions: Arc<Mutex<Vec<acp::SessionId>>>,
}

impl CloseCapableConnection {
    fn new() -> Self {
        Self {
            closed_sessions: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl AgentConnection for CloseCapableConnection {
    fn agent_id(&self) -> AgentId {
        AgentId::new("close-capable")
    }

    fn telemetry_id(&self) -> SharedString {
        "close-capable".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        work_dirs: PathList,
        cx: &mut App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        let action_log = cx.new(|_| ActionLog::new(project.clone()));
        let thread = cx.new(|cx| {
            AcpThread::new(
                None,
                Some("CloseCapableConnection".into()),
                Some(work_dirs),
                self,
                project,
                action_log,
                acp::SessionId::new("close-capable-session"),
                watch::Receiver::constant(
                    acp::PromptCapabilities::new()
                        .image(true)
                        .audio(true)
                        .embedded_context(true),
                ),
                cx,
            )
        });
        Task::ready(Ok(thread))
    }

    fn supports_close_session(&self) -> bool {
        true
    }

    fn close_session(
        self: Rc<Self>,
        session_id: &acp::SessionId,
        _cx: &mut App,
    ) -> Task<Result<()>> {
        self.closed_sessions.lock().push(session_id.clone());
        Task::ready(Ok(()))
    }

    fn auth_methods(&self) -> &[acp::AuthMethod] {
        &[]
    }

    fn authenticate(&self, _method_id: acp::AuthMethodId, _cx: &mut App) -> Task<gpui::Result<()>> {
        Task::ready(Ok(()))
    }

    fn prompt(
        &self,
        _params: acp::PromptRequest,
        _cx: &mut App,
    ) -> Task<gpui::Result<acp::PromptResponse>> {
        Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
    }

    fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
