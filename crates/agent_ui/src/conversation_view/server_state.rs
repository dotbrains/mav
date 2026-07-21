use super::*;

pub(super) enum ServerState {
    Loading {
        _loading: Entity<LoadingView>,
        draft: Option<LoadingDraft>,
    },
    LoadError {
        error: LoadError,
    },
    Connected(ConnectedServerState),
}

// current -> Entity
// hashmap of threads, current becomes session_id
pub struct ConnectedServerState {
    pub(super) auth_state: AuthState,
    pub(super) active_id: Option<acp::SessionId>,
    pub(crate) threads: HashMap<acp::SessionId, Entity<ThreadView>>,
    pub(super) connection: Rc<dyn AgentConnection>,
    pub(super) conversation: Entity<Conversation>,
    pub(super) _connection_entry_subscription: Subscription,
}

pub(super) enum AuthState {
    Ok,
    Unauthenticated {
        description: Option<Entity<Markdown>>,
        configuration_view: Option<AnyView>,
        pending_auth_method: Option<acp::AuthMethodId>,
        _subscription: Option<Subscription>,
    },
}

impl AuthState {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

pub(super) struct LoadingView {
    pub(super) _load_task: Task<()>,
}

pub(super) struct LoadingDraft {
    pub(super) message_editor: Entity<MessageEditor>,
    pub(super) agent_selector_menu_handle: PopoverMenuHandle<ContextMenu>,
    pub(super) _subscriptions: Vec<Subscription>,
}

impl ConnectedServerState {
    pub fn active_view(&self) -> Option<&Entity<ThreadView>> {
        self.active_id.as_ref().and_then(|id| self.threads.get(id))
    }

    pub fn has_thread_error(&self, cx: &App) -> bool {
        self.active_view()
            .map_or(false, |view| view.read(cx).thread_error.is_some())
    }

    pub fn navigate_to_thread(&mut self, session_id: acp::SessionId) {
        if self.threads.contains_key(&session_id) {
            self.active_id = Some(session_id);
        }
    }

    pub fn close_all_sessions(&self, cx: &mut App) -> Task<()> {
        let tasks = self.threads.values().filter_map(|view| {
            if self.connection.supports_close_session() {
                let session_id = view.read(cx).thread.read(cx).session_id().clone();
                Some(self.connection.clone().close_session(&session_id, cx))
            } else {
                None
            }
        });
        let task = futures::future::join_all(tasks);
        cx.background_spawn(async move {
            task.await;
        })
    }
}
