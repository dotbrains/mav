use super::*;

impl AgentPanel {
    pub(crate) fn create_agent_thread_with_server(
        &mut self,
        agent: Agent,
        server_override: Option<Rc<dyn AgentServer>>,
        resume_thread_id: Option<ThreadId>,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        initial_content: Option<AgentInitialContent>,
        model_override: Option<String>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AgentThread {
        let resume_session_id = resume_thread_id.and_then(|tid| {
            ThreadMetadataStore::try_global(cx)
                .and_then(|store| store.read(cx).entry(tid).and_then(|m| m.session_id.clone()))
        });
        self.create_agent_thread_inner(
            agent,
            server_override,
            resume_thread_id,
            resume_session_id,
            work_dirs,
            title,
            initial_content,
            model_override,
            source,
            window,
            cx,
        )
    }

    /// Legacy entry that resumes a thread by raw ACP session id when no
    /// local [`ThreadMetadata`] row exists yet (share-link imports and
    /// clipboard imports).
    ///
    /// TODO(legacy-session-id): migrate remaining callers (share-link
    /// handler, clipboard import) to mint a [`ThreadId`] + seed metadata
    /// so they can route through [`create_agent_thread_with_server`] and
    /// this entry can be deleted.
    pub(super) fn create_agent_thread_with_server_for_external_session(
        &mut self,
        agent: Agent,
        server_override: Option<Rc<dyn AgentServer>>,
        resume_session_id: acp::SessionId,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        initial_content: Option<AgentInitialContent>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AgentThread {
        self.create_agent_thread_inner(
            agent,
            server_override,
            None,
            Some(resume_session_id),
            work_dirs,
            title,
            initial_content,
            None,
            source,
            window,
            cx,
        )
    }

    fn create_agent_thread_inner(
        &mut self,
        agent: Agent,
        server_override: Option<Rc<dyn AgentServer>>,
        resume_thread_id: Option<ThreadId>,
        resume_session_id: Option<acp::SessionId>,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        initial_content: Option<AgentInitialContent>,
        model_override: Option<String>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AgentThread {
        let thread_id = resume_thread_id.unwrap_or_else(ThreadId::new);
        let workspace = self.workspace.clone();
        let project = self.project.clone();

        if self.selected_agent != agent {
            self.selected_agent = agent.clone();
            self.serialize(cx);
        }

        cx.background_spawn({
            let kvp = KeyValueStore::global(cx);
            let agent = agent.clone();
            async move {
                write_global_last_used_agent(kvp, agent).await;
            }
        })
        .detach();

        let server = server_override
            .unwrap_or_else(|| agent.server(self.fs.clone(), self.thread_store.clone()));
        let thread_store = server
            .clone()
            .downcast::<agent::NativeAgentServer>()
            .is_some()
            .then(|| self.thread_store.clone());

        let connection_store = self.connection_store.clone();

        let conversation_view = cx.new(|cx| {
            crate::ConversationView::new(
                server,
                connection_store,
                agent,
                resume_session_id,
                Some(thread_id),
                work_dirs,
                title,
                initial_content,
                workspace.clone(),
                project,
                thread_store,
                source,
                window,
                cx,
            )
        });

        cx.observe_in(
            &conversation_view,
            window,
            |this, server_view, window, cx| {
                let is_active = this
                    .active_conversation_view()
                    .is_some_and(|active| active.entity_id() == server_view.entity_id());
                if is_active {
                    cx.emit(AgentPanelEvent::ActiveViewChanged);
                    this.serialize(cx);
                } else {
                    cx.emit(AgentPanelEvent::EntryChanged);
                }
                this.ensure_sibling_host_installed(&server_view, window, cx);
                cx.notify();
            },
        )
        .detach();

        // Try installing the host eagerly as well, in case the connection is
        // already established by the time the observe fires.
        self.ensure_sibling_host_installed(&conversation_view, window, cx);

        if let Some(model) = model_override {
            // The native thread is constructed asynchronously after the
            // connection establishes. Wait for the first `RootThreadUpdated`
            // event that yields a native thread, then apply the override once.
            let applied = Cell::new(false);
            cx.subscribe(
                &conversation_view,
                move |_this, view, _event: &RootThreadUpdated, cx| {
                    if applied.get() {
                        return;
                    }
                    let Some(native_thread) = view.read(cx).as_native_thread(cx) else {
                        return;
                    };
                    apply_native_model_override(&native_thread, &model, cx);
                    applied.set(true);
                },
            )
            .detach();
        }

        AgentThread { conversation_view }
    }

    fn ensure_sibling_host_installed(
        &self,
        conversation_view: &Entity<ConversationView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !cx.has_flag::<CreateThreadToolFeatureFlag>() {
            return;
        }
        let Some(native_connection) = conversation_view.read(cx).as_native_connection(cx) else {
            return;
        };
        let host = Rc::new(AgentPanelSiblingHost::new(
            cx.weak_entity(),
            window.window_handle(),
        )) as Rc<dyn agent::SiblingThreadHost>;
        native_connection.0.update(cx, |native_agent, _cx| {
            native_agent.set_sibling_thread_host(host);
        });
    }
}
