use super::*;

impl ConversationView {
    pub fn has_auth_methods(&self) -> bool {
        self.as_connected().map_or(false, |connected| {
            !connected.connection.auth_methods().is_empty()
        })
    }

    pub fn supports_logout(&self) -> bool {
        self.as_connected().is_some_and(|connected| {
            connected.auth_state.is_ok() && connected.connection.supports_logout()
        })
    }

    pub fn active_thread(&self) -> Option<&Entity<ThreadView>> {
        match &self.server_state {
            ServerState::Connected(connected) => connected.active_view(),
            _ => None,
        }
    }

    pub fn pending_tool_call<'a>(
        &'a self,
        cx: &'a App,
    ) -> Option<(acp::SessionId, acp::ToolCallId, &'a PermissionOptions)> {
        let session_id = self.active_thread()?.read(cx).session_id.clone();
        self.as_connected()?
            .conversation
            .read(cx)
            .pending_tool_call(&session_id, cx)
    }

    pub fn root_thread_has_pending_tool_call(&self, cx: &App) -> bool {
        let Some(root_thread) = self.root_thread_view() else {
            return false;
        };
        let root_session_id = root_thread.read(cx).thread.read(cx).session_id().clone();
        self.as_connected().is_some_and(|connected| {
            connected
                .conversation
                .read(cx)
                .pending_tool_call(&root_session_id, cx)
                .is_some()
        })
    }

    pub(crate) fn root_thread(&self, cx: &App) -> Option<Entity<AcpThread>> {
        self.root_thread_view()
            .map(|view| view.read(cx).thread.clone())
    }

    pub fn root_thread_view(&self) -> Option<Entity<ThreadView>> {
        self.root_session_id
            .as_ref()
            .and_then(|id| self.thread_view(id))
    }

    pub fn thread_view(&self, session_id: &acp::SessionId) -> Option<Entity<ThreadView>> {
        let connected = self.as_connected()?;
        connected.threads.get(session_id).cloned()
    }

    pub fn as_connected(&self) -> Option<&ConnectedServerState> {
        match &self.server_state {
            ServerState::Connected(connected) => Some(connected),
            _ => None,
        }
    }

    pub fn as_connected_mut(&mut self) -> Option<&mut ConnectedServerState> {
        match &mut self.server_state {
            ServerState::Connected(connected) => Some(connected),
            _ => None,
        }
    }

    pub fn updated_at(&self, cx: &App) -> Option<Instant> {
        self.as_connected()
            .and_then(|connected| connected.conversation.read(cx).updated_at)
    }

    pub fn navigate_to_thread(
        &mut self,
        session_id: acp::SessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(connected) = self.as_connected_mut() else {
            return;
        };

        connected.navigate_to_thread(session_id);
        if let Some(view) = self.active_thread() {
            view.focus_handle(cx).focus(window, cx);
        }
        cx.emit(AcpServerViewEvent::ActiveThreadChanged);
        cx.notify();
    }

    pub fn set_work_dirs(&mut self, work_dirs: PathList, cx: &mut Context<Self>) {
        if let Some(connected) = self.as_connected() {
            connected.conversation.update(cx, |conversation, cx| {
                conversation.set_work_dirs(work_dirs.clone(), cx);
            });
        }
    }
}
