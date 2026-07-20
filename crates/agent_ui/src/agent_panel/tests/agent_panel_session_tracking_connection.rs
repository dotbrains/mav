use super::*;

#[derive(Clone, Default)]
pub(super) struct SessionTrackingConnection {
    next_session_number: Arc<Mutex<usize>>,
    sessions: Arc<Mutex<HashSet<acp::SessionId>>>,
}

impl SessionTrackingConnection {
    pub(super) fn new() -> Self {
        Self::default()
    }

    fn create_session(
        self: Rc<Self>,
        session_id: acp::SessionId,
        project: Entity<Project>,
        work_dirs: PathList,
        title: Option<SharedString>,
        cx: &mut App,
    ) -> Entity<AcpThread> {
        self.sessions.lock().insert(session_id.clone());

        let action_log = cx.new(|_| ActionLog::new(project.clone()));
        cx.new(|cx| {
            AcpThread::new(
                None,
                title,
                Some(work_dirs),
                self,
                project,
                action_log,
                session_id,
                watch::Receiver::constant(
                    acp::PromptCapabilities::new()
                        .image(true)
                        .audio(true)
                        .embedded_context(true),
                ),
                cx,
            )
        })
    }
}

impl AgentConnection for SessionTrackingConnection {
    fn agent_id(&self) -> AgentId {
        agent::MAV_AGENT_ID.clone()
    }

    fn telemetry_id(&self) -> SharedString {
        "session-tracking-test".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        work_dirs: PathList,
        cx: &mut App,
    ) -> Task<Result<Entity<AcpThread>>> {
        let session_id = {
            let mut next_session_number = self.next_session_number.lock();
            let session_id =
                acp::SessionId::new(format!("session-tracking-session-{}", *next_session_number));
            *next_session_number += 1;
            session_id
        };
        let thread = self.create_session(session_id, project, work_dirs, None, cx);
        Task::ready(Ok(thread))
    }

    fn supports_load_session(&self) -> bool {
        true
    }

    fn load_session(
        self: Rc<Self>,
        session_id: acp::SessionId,
        project: Entity<Project>,
        work_dirs: PathList,
        title: Option<SharedString>,
        cx: &mut App,
    ) -> Task<Result<Entity<AcpThread>>> {
        let thread = self.create_session(session_id, project, work_dirs, title, cx);
        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::UserMessageChunk(acp::ContentChunk::new(
                        "Restored user message".into(),
                    )),
                    cx,
                )
                .expect("restored user message should be applied");
            thread
                .handle_session_update(
                    acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                        "Restored assistant message".into(),
                    )),
                    cx,
                )
                .expect("restored assistant message should be applied");
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
        self.sessions.lock().remove(session_id);
        Task::ready(Ok(()))
    }

    fn auth_methods(&self) -> &[acp::AuthMethod] {
        &[]
    }

    fn authenticate(&self, _method_id: acp::AuthMethodId, _cx: &mut App) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn prompt(
        &self,
        params: acp::PromptRequest,
        _cx: &mut App,
    ) -> Task<Result<acp::PromptResponse>> {
        if !self.sessions.lock().contains(&params.session_id) {
            return Task::ready(Err(anyhow!("Session not found")));
        }

        Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
    }

    fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
