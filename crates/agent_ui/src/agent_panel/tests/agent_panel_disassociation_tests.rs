use super::*;

/// Connection that tracks closed sessions and detects prompts against sessions
/// that no longer exist, used to reproduce session disassociation.
#[derive(Clone, Default)]
struct DisassociationTrackingConnection {
    next_session_number: Arc<Mutex<usize>>,
    sessions: Arc<Mutex<HashSet<acp::SessionId>>>,
    closed_sessions: Arc<Mutex<Vec<acp::SessionId>>>,
    missing_prompt_sessions: Arc<Mutex<Vec<acp::SessionId>>>,
}

impl DisassociationTrackingConnection {
    fn new() -> Self {
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

impl AgentConnection for DisassociationTrackingConnection {
    fn agent_id(&self) -> AgentId {
        agent::MAV_AGENT_ID.clone()
    }

    fn telemetry_id(&self) -> SharedString {
        "disassociation-tracking-test".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        work_dirs: PathList,
        cx: &mut App,
    ) -> Task<Result<Entity<AcpThread>>> {
        let session_id = {
            let mut next_session_number = self.next_session_number.lock();
            let session_id = acp::SessionId::new(format!(
                "disassociation-tracking-session-{}",
                *next_session_number
            ));
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
        self.closed_sessions.lock().push(session_id.clone());
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
            self.missing_prompt_sessions.lock().push(params.session_id);
            return Task::ready(Err(anyhow!("Session not found")));
        }

        Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
    }

    fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

#[gpui::test]
async fn test_retained_thread_reset_race_disassociates_session(cx: &mut TestAppContext) {
    let (_workspace, panel, mut cx) = setup_workspace_panel(cx).await;
    cx.run_until_parked();

    let connection = DisassociationTrackingConnection::new();
    panel.update(&mut cx, |panel, cx| {
        panel.connection_store.update(cx, |store, cx| {
            store.restart_connection(
                Agent::Stub,
                Rc::new(StubAgentServer::new(connection.clone())),
                cx,
            );
        });
    });
    cx.run_until_parked();

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.external_thread(
            Some(Agent::Stub),
            None,
            None,
            None,
            None,
            true,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    send_message(&panel, &mut cx);

    let session_id_a = active_session_id(&panel, &cx);
    let thread_id_a = active_thread_id(&panel, &cx);

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.external_thread(
            Some(Agent::Stub),
            None,
            None,
            None,
            None,
            true,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    send_message(&panel, &mut cx);

    panel.read_with(&cx, |panel, _cx| {
        assert!(
            panel.retained_threads.contains_key(&thread_id_a),
            "thread A should be in retained_threads after switching to B"
        );
    });

    let retained_conversation_a = panel.read_with(&cx, |panel, _cx| {
        panel
            .retained_threads
            .get(&thread_id_a)
            .expect("thread A should be retained")
            .clone()
    });
    retained_conversation_a.update(&mut cx, |conversation, cx| {
        if let Some(thread_view) = conversation.active_thread() {
            thread_view.update(cx, |view, cx| {
                view.handle_thread_error(
                    crate::conversation_view::ThreadError::Other {
                        message: "simulated error".into(),
                        acp_error_code: None,
                    },
                    cx,
                );
            });
        }
    });

    retained_conversation_a.read_with(&cx, |conversation, cx| {
        let connected = conversation.as_connected().expect("should be connected");
        assert!(
            connected.has_thread_error(cx),
            "retained A should have a thread error"
        );
    });

    panel.update(&mut cx, |panel, cx| {
        panel.project.update(cx, |project, cx| {
            project
                .agent_server_store()
                .update(cx, |_store, cx| cx.emit(project::AgentServersUpdated));
        });
    });

    panel.update_in(&mut cx, |panel, window, cx| {
        panel.open_thread(session_id_a.clone(), None, None, window, cx);
    });

    cx.run_until_parked();

    panel.read_with(&cx, |panel, cx| {
        let active_session = panel
            .active_agent_thread(cx)
            .map(|t| t.read(cx).session_id().clone());
        assert_eq!(
            active_session,
            Some(session_id_a.clone()),
            "session A should be the active session after open_thread"
        );
    });

    drop(retained_conversation_a);
    panel.update(&mut cx, |panel, _cx| {
        panel.retained_threads.remove(&thread_id_a);
    });
    cx.run_until_parked();

    send_message(&panel, &mut cx);
    send_message(&panel, &mut cx);

    let missing = connection.missing_prompt_sessions.lock().clone();
    assert!(
        missing.is_empty(),
        "session should not be disassociated after retained thread reset race, \
         got missing prompt sessions: {:?}",
        missing
    );

    panel.read_with(&cx, |panel, cx| {
        let active_view = panel
            .active_conversation_view()
            .expect("conversation should remain open");
        let connected = active_view
            .read(cx)
            .as_connected()
            .expect("conversation should be connected");
        assert!(
            !connected.has_thread_error(cx),
            "conversation should not have a thread error"
        );
    });
}
