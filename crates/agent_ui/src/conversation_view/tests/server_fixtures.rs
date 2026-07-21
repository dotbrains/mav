use super::*;

pub(crate) struct StubAgentServer<C> {
    connection: C,
}

impl<C> StubAgentServer<C> {
    pub(crate) fn new(connection: C) -> Self {
        Self { connection }
    }
}

impl StubAgentServer<StubAgentConnection> {
    pub(crate) fn default_response() -> Self {
        let conn = StubAgentConnection::new();
        conn.set_next_prompt_updates(vec![acp::SessionUpdate::AgentMessageChunk(
            acp::ContentChunk::new("Default response".into()),
        )]);
        Self::new(conn)
    }
}

impl<C> AgentServer for StubAgentServer<C>
where
    C: 'static + AgentConnection + Send + Clone,
{
    fn logo(&self) -> ui::IconName {
        ui::IconName::MavAgent
    }

    fn agent_id(&self) -> AgentId {
        "Test".into()
    }

    fn connect(
        &self,
        _delegate: AgentServerDelegate,
        _project: Entity<Project>,
        _cx: &mut App,
    ) -> Task<gpui::Result<Rc<dyn AgentConnection>>> {
        Task::ready(Ok(Rc::new(self.connection.clone())))
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

pub(crate) struct FailingAgentServer;

impl AgentServer for FailingAgentServer {
    fn logo(&self) -> ui::IconName {
        ui::IconName::AiOpenAi
    }

    fn agent_id(&self) -> AgentId {
        AgentId::new("Codex CLI")
    }

    fn connect(
        &self,
        _delegate: AgentServerDelegate,
        _project: Entity<Project>,
        _cx: &mut App,
    ) -> Task<gpui::Result<Rc<dyn AgentConnection>>> {
        Task::ready(Err(anyhow!(
            "extracting downloaded asset for \
             https://github.com/mav-industries/codex-acp/releases/download/v0.9.4/\
             codex-acp-0.9.4-aarch64-pc-windows-msvc.zip: \
             failed to iterate over archive: Invalid gzip header"
        )))
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

/// Agent server whose `connect()` fails while `fail` is `true` and
/// returns the wrapped connection otherwise. Used to simulate the
/// race where an external agent isn't yet registered at startup.
pub(crate) struct FlakyAgentServer {
    connection: StubAgentConnection,
    fail: Arc<std::sync::atomic::AtomicBool>,
}

impl FlakyAgentServer {
    pub(crate) fn new(
        connection: StubAgentConnection,
    ) -> (Self, Arc<std::sync::atomic::AtomicBool>) {
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(true));
        (
            Self {
                connection,
                fail: fail.clone(),
            },
            fail,
        )
    }
}

impl AgentServer for FlakyAgentServer {
    fn logo(&self) -> ui::IconName {
        ui::IconName::MavAgent
    }

    fn agent_id(&self) -> AgentId {
        "Flaky".into()
    }

    fn connect(
        &self,
        _delegate: AgentServerDelegate,
        _project: Entity<Project>,
        _cx: &mut App,
    ) -> Task<gpui::Result<Rc<dyn AgentConnection>>> {
        if self.fail.load(std::sync::atomic::Ordering::SeqCst) {
            Task::ready(Err(anyhow!(
                "Custom agent server `Flaky` is not registered"
            )))
        } else {
            Task::ready(Ok(Rc::new(self.connection.clone())))
        }
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
