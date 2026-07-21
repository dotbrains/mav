use super::*;

pub struct NativeAgentSessionList {
    thread_store: Entity<ThreadStore>,
    updates_tx: async_channel::Sender<acp_thread::SessionListUpdate>,
    updates_rx: async_channel::Receiver<acp_thread::SessionListUpdate>,
    _subscription: Subscription,
}

impl NativeAgentSessionList {
    pub(super) fn new(thread_store: Entity<ThreadStore>, cx: &mut App) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let this_tx = tx.clone();
        let subscription = cx.observe(&thread_store, move |_, _| {
            this_tx
                .try_send(acp_thread::SessionListUpdate::Refresh)
                .ok();
        });
        Self {
            thread_store,
            updates_tx: tx,
            updates_rx: rx,
            _subscription: subscription,
        }
    }
}

impl AgentSessionList for NativeAgentSessionList {
    fn list_sessions(
        &self,
        _request: AgentSessionListRequest,
        cx: &mut App,
    ) -> Task<Result<AgentSessionListResponse>> {
        let sessions = self
            .thread_store
            .read(cx)
            .entries()
            .map(|entry| AgentSessionInfo::from(&entry))
            .collect();
        Task::ready(Ok(AgentSessionListResponse::new(sessions)))
    }

    fn supports_delete(&self) -> bool {
        true
    }

    fn delete_session(&self, session_id: &acp::SessionId, cx: &mut App) -> Task<Result<()>> {
        self.thread_store
            .update(cx, |store, cx| store.delete_thread(session_id.clone(), cx))
    }

    fn delete_sessions(&self, cx: &mut App) -> Task<Result<()>> {
        self.thread_store
            .update(cx, |store, cx| store.delete_threads(cx))
    }

    fn watch(
        &self,
        _cx: &mut App,
    ) -> Option<async_channel::Receiver<acp_thread::SessionListUpdate>> {
        Some(self.updates_rx.clone())
    }

    fn notify_refresh(&self) {
        self.updates_tx
            .try_send(acp_thread::SessionListUpdate::Refresh)
            .ok();
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

pub(super) struct NativeAgentSessionTruncate {
    pub(super) thread: Entity<Thread>,
    pub(super) acp_thread: WeakEntity<AcpThread>,
}

impl acp_thread::AgentSessionTruncate for NativeAgentSessionTruncate {
    fn run(
        &self,
        client_user_message_id: acp_thread::ClientUserMessageId,
        cx: &mut App,
    ) -> Task<Result<()>> {
        match self.thread.update(cx, |thread, cx| {
            thread.truncate(client_user_message_id.clone(), cx)?;
            Ok(thread.latest_token_usage())
        }) {
            Ok(usage) => {
                self.acp_thread
                    .update(cx, |thread, cx| {
                        thread.update_token_usage(usage, cx);
                    })
                    .ok();
                Task::ready(Ok(()))
            }
            Err(error) => Task::ready(Err(error)),
        }
    }
}

pub(super) struct NativeAgentSessionRetry {
    pub(super) connection: NativeAgentConnection,
    pub(super) session_id: acp::SessionId,
}

impl acp_thread::AgentSessionRetry for NativeAgentSessionRetry {
    fn run(&self, cx: &mut App) -> Task<Result<acp::PromptResponse>> {
        self.connection
            .run_turn(self.session_id.clone(), cx, |thread, cx| {
                thread.update(cx, |thread, cx| thread.resume(cx))
            })
    }
}

pub(super) struct NativeAgentSessionSetTitle {
    pub(super) thread: Entity<Thread>,
}

impl acp_thread::AgentSessionSetTitle for NativeAgentSessionSetTitle {
    fn run(&self, title: SharedString, cx: &mut App) -> Task<Result<()>> {
        self.thread
            .update(cx, |thread, cx| thread.set_title(title, cx));
        Task::ready(Ok(()))
    }
}
