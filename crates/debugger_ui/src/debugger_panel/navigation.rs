use super::*;

impl DebugPanel {
    pub(crate) fn activate_pane_in_direction(
        &mut self,
        direction: SplitDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.active_session() {
            session.update(cx, |session, cx| {
                session.running_state().update(cx, |running, cx| {
                    running.activate_pane_in_direction(direction, window, cx);
                })
            });
        }
    }

    pub(crate) fn activate_item(
        &mut self,
        item: DebuggerPaneItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.active_session() {
            session.update(cx, |session, cx| {
                session.running_state().update(cx, |running, cx| {
                    running.activate_item(item, window, cx);
                });
            });
        }
    }

    pub(crate) fn activate_session_by_id(
        &mut self,
        session_id: SessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self
            .sessions_with_children
            .keys()
            .find(|session| session.read(cx).session_id(cx) == session_id)
        {
            self.activate_session(session.clone(), window, cx);
        }
    }

    pub(crate) fn activate_session(
        &mut self,
        session_item: Entity<DebugSession>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(self.sessions_with_children.contains_key(&session_item));
        session_item.focus_handle(cx).focus(window, cx);
        session_item.update(cx, |this, cx| {
            this.running_state().update(cx, |this, cx| {
                this.go_to_selected_stack_frame(window, cx);
            });
        });
        self.active_session = Some(session_item);
        cx.notify();
    }
}
