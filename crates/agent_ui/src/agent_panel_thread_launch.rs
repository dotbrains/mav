use super::*;

impl AgentPanel {
    pub(super) fn new_native_agent_thread_from_summary(
        &mut self,
        action: &NewNativeAgentThreadFromSummary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_id = action.from_session_id.clone();

        let Some(content) = Self::initial_content_for_thread_summary(session_id.clone(), cx) else {
            log::error!("No session found for summarization with id {}", session_id);
            return;
        };

        cx.spawn_in(window, async move |this, cx| {
            this.update_in(cx, |this, window, cx| {
                this.external_thread(
                    Some(Agent::NativeAgent),
                    None,
                    None,
                    None,
                    Some(content),
                    true,
                    AgentThreadSource::AgentPanel,
                    window,
                    cx,
                );
                anyhow::Ok(())
            })
        })
        .detach_and_log_err(cx);
    }

    pub(super) fn initial_content_for_thread_summary(
        session_id: acp::SessionId,
        cx: &App,
    ) -> Option<AgentInitialContent> {
        let thread = ThreadStore::global(cx)
            .read(cx)
            .entries()
            .find(|t| t.id == session_id)?;

        Some(AgentInitialContent::ThreadSummary {
            session_id: thread.id,
            title: Some(thread.title),
        })
    }

    pub(super) fn external_thread(
        &mut self,
        agent_choice: Option<crate::Agent>,
        resume_thread_id: Option<ThreadId>,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        initial_content: Option<AgentInitialContent>,
        focus: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if resume_thread_id.is_none() && !self.has_open_project(cx) {
            return;
        }

        let agent = agent_choice.unwrap_or_else(|| self.selected_agent(cx));
        let thread = self.create_agent_thread_with_server(
            agent,
            None,
            resume_thread_id,
            work_dirs,
            title,
            initial_content,
            None,
            source,
            window,
            cx,
        );
        self.set_base_view(thread.into(), focus, window, cx);
    }
}
