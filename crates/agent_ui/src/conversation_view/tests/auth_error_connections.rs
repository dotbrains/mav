use super::*;

pub(crate) struct AuthGatedAgentConnection {
    authenticated: Arc<Mutex<bool>>,
    auth_method: acp::AuthMethod,
}

impl AuthGatedAgentConnection {
    pub(crate) const AUTH_METHOD_ID: &str = "test-login";

    pub(crate) fn new() -> Self {
        Self {
            authenticated: Arc::new(Mutex::new(false)),
            auth_method: acp::AuthMethod::Agent(acp::AuthMethodAgent::new(
                Self::AUTH_METHOD_ID,
                "Test Login",
            )),
        }
    }
}

impl AgentConnection for AuthGatedAgentConnection {
    fn agent_id(&self) -> AgentId {
        AgentId::new("auth-gated")
    }

    fn telemetry_id(&self) -> SharedString {
        "auth-gated".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        work_dirs: PathList,
        cx: &mut gpui::App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        if !*self.authenticated.lock() {
            return Task::ready(Err(acp_thread::AuthRequired::new()
                .with_description("Sign in to continue".to_string())
                .into()));
        }

        let session_id = acp::SessionId::new("auth-gated-session");
        let action_log = cx.new(|_| ActionLog::new(project.clone()));
        Task::ready(Ok(cx.new(|cx| {
            AcpThread::new(
                None,
                None,
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
        })))
    }

    fn auth_methods(&self) -> &[acp::AuthMethod] {
        std::slice::from_ref(&self.auth_method)
    }

    fn authenticate(&self, method_id: acp::AuthMethodId, _cx: &mut App) -> Task<gpui::Result<()>> {
        if &method_id == self.auth_method.id() {
            *self.authenticated.lock() = true;
            Task::ready(Ok(()))
        } else {
            Task::ready(Err(anyhow::anyhow!("Unknown auth method")))
        }
    }

    fn supports_logout(&self) -> bool {
        true
    }

    fn logout(&self, _cx: &mut App) -> Task<gpui::Result<()>> {
        *self.authenticated.lock() = false;
        Task::ready(Ok(()))
    }

    fn prompt(
        &self,
        _params: acp::PromptRequest,
        _cx: &mut App,
    ) -> Task<gpui::Result<acp::PromptResponse>> {
        unimplemented!()
    }

    fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {
        unimplemented!()
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

/// Simulates a model which always returns a refusal response
#[derive(Clone)]
pub(crate) struct RefusalAgentConnection;

impl AgentConnection for RefusalAgentConnection {
    fn agent_id(&self) -> AgentId {
        AgentId::new("refusal")
    }

    fn telemetry_id(&self) -> SharedString {
        "refusal".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        work_dirs: PathList,
        cx: &mut gpui::App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        Task::ready(Ok(cx.new(|cx| {
            let action_log = cx.new(|_| ActionLog::new(project.clone()));
            AcpThread::new(
                None,
                None,
                Some(work_dirs),
                self,
                project,
                action_log,
                acp::SessionId::new("test"),
                watch::Receiver::constant(
                    acp::PromptCapabilities::new()
                        .image(true)
                        .audio(true)
                        .embedded_context(true),
                ),
                cx,
            )
        })))
    }

    fn auth_methods(&self) -> &[acp::AuthMethod] {
        &[]
    }

    fn authenticate(&self, _method_id: acp::AuthMethodId, _cx: &mut App) -> Task<gpui::Result<()>> {
        unimplemented!()
    }

    fn prompt(
        &self,
        _params: acp::PromptRequest,
        _cx: &mut App,
    ) -> Task<gpui::Result<acp::PromptResponse>> {
        Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::Refusal)))
    }

    fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {
        unimplemented!()
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
