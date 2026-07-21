use super::*;

#[derive(Clone)]
pub(crate) struct RestoredAvailableCommandsConnection;

impl AgentConnection for RestoredAvailableCommandsConnection {
    fn agent_id(&self) -> AgentId {
        AgentId::new("restored-available-commands")
    }

    fn telemetry_id(&self) -> SharedString {
        "restored-available-commands".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        _work_dirs: PathList,
        cx: &mut App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        let thread = build_test_thread(
            self,
            project,
            "RestoredAvailableCommandsConnection",
            acp::SessionId::new("new-session"),
            cx,
        );
        Task::ready(Ok(thread))
    }

    fn supports_load_session(&self) -> bool {
        true
    }

    fn load_session(
        self: Rc<Self>,
        session_id: acp::SessionId,
        project: Entity<Project>,
        _work_dirs: PathList,
        _title: Option<SharedString>,
        cx: &mut App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        let thread = build_test_thread(
            self,
            project,
            "RestoredAvailableCommandsConnection",
            session_id,
            cx,
        );

        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::AvailableCommandsUpdate(acp::AvailableCommandsUpdate::new(
                        vec![acp::AvailableCommand::new("help", "Get help")],
                    )),
                    cx,
                )
            })
            .expect("available commands update should succeed");

        Task::ready(Ok(thread))
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

fn build_test_thread(
    connection: Rc<dyn AgentConnection>,
    project: Entity<Project>,
    name: &'static str,
    session_id: acp::SessionId,
    cx: &mut App,
) -> Entity<AcpThread> {
    let action_log = cx.new(|_| ActionLog::new(project.clone()));
    cx.new(|cx| {
        AcpThread::new(
            None,
            Some(name.into()),
            None,
            connection,
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

#[derive(Clone)]
pub(crate) struct ResumeOnlyAgentConnection;

impl AgentConnection for ResumeOnlyAgentConnection {
    fn agent_id(&self) -> AgentId {
        AgentId::new("resume-only")
    }

    fn telemetry_id(&self) -> SharedString {
        "resume-only".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        _work_dirs: PathList,
        cx: &mut gpui::App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        let thread = build_test_thread(
            self,
            project,
            "ResumeOnlyAgentConnection",
            acp::SessionId::new("new-session"),
            cx,
        );
        Task::ready(Ok(thread))
    }

    fn supports_resume_session(&self) -> bool {
        true
    }

    fn resume_session(
        self: Rc<Self>,
        session_id: acp::SessionId,
        project: Entity<Project>,
        _work_dirs: PathList,
        _title: Option<SharedString>,
        cx: &mut App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        let thread = build_test_thread(self, project, "ResumeOnlyAgentConnection", session_id, cx);
        Task::ready(Ok(thread))
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

/// Simulates an agent that requires authentication before a session can be
/// created. `new_session` returns `AuthRequired` until `authenticate` is
/// called with the correct method, after which sessions are created normally.
#[derive(Clone, Clone)]
pub(crate) struct CwdCapturingConnection {
    pub(crate) captured_work_dirs: Arc<Mutex<Option<PathList>>>,
}

impl CwdCapturingConnection {
    pub(crate) fn new() -> Self {
        Self {
            captured_work_dirs: Arc::new(Mutex::new(None)),
        }
    }
}

impl AgentConnection for CwdCapturingConnection {
    fn agent_id(&self) -> AgentId {
        AgentId::new("cwd-capturing")
    }

    fn telemetry_id(&self) -> SharedString {
        "cwd-capturing".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        work_dirs: PathList,
        cx: &mut gpui::App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        *self.captured_work_dirs.lock() = Some(work_dirs.clone());
        let action_log = cx.new(|_| ActionLog::new(project.clone()));
        let thread = cx.new(|cx| {
            AcpThread::new(
                None,
                None,
                Some(work_dirs),
                self.clone(),
                project,
                action_log,
                acp::SessionId::new("new-session"),
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

    fn supports_load_session(&self) -> bool {
        true
    }

    fn load_session(
        self: Rc<Self>,
        session_id: acp::SessionId,
        project: Entity<Project>,
        work_dirs: PathList,
        _title: Option<SharedString>,
        cx: &mut App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        *self.captured_work_dirs.lock() = Some(work_dirs.clone());
        let action_log = cx.new(|_| ActionLog::new(project.clone()));
        let thread = cx.new(|cx| {
            AcpThread::new(
                None,
                None,
                Some(work_dirs),
                self.clone(),
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
        });
        Task::ready(Ok(thread))
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
