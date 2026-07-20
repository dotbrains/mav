use super::*;

impl AgentPanel {
    pub fn toggle_focus(
        workspace: &mut Workspace,
        _: &ToggleFocus,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if workspace
            .panel::<Self>(cx)
            .is_some_and(|panel| panel.read(cx).enabled(cx))
        {
            workspace.toggle_panel_focus::<Self>(window, cx);
        }
    }

    pub fn focus(
        workspace: &mut Workspace,
        _: &FocusAgent,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if workspace
            .panel::<Self>(cx)
            .is_some_and(|panel| panel.read(cx).enabled(cx))
        {
            workspace.focus_panel::<Self>(window, cx);
        }
    }

    pub fn toggle(
        workspace: &mut Workspace,
        _: &Toggle,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if workspace
            .panel::<Self>(cx)
            .is_some_and(|panel| panel.read(cx).enabled(cx))
            && !workspace.toggle_panel_focus::<Self>(window, cx)
        {
            workspace.close_panel::<Self>(window, cx);
        }
    }

    pub fn dismiss_all_notifications(&mut self, cx: &mut Context<Self>) -> bool {
        let mut dismissed = false;
        for conversation_view in self.conversation_views() {
            dismissed |= conversation_view.update(cx, |view, cx| view.dismiss_notifications(cx));
        }
        let had_terminal_notifications = self
            .terminals
            .values()
            .any(|t| !t.notification_windows.is_empty());
        if had_terminal_notifications {
            self.dismiss_all_terminal_notifications(cx);
            dismissed = true;
        }
        dismissed
    }

    pub(super) fn manage_skills(
        &mut self,
        _action: &ManageSkills,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.dispatch_action(
            Box::new(mav_actions::OpenSettingsAt {
                path: mav_actions::AGENT_SKILLS_SETTINGS_PATH.to_string(),
                target: None,
            }),
            cx,
        );
    }

    /// Refresh the native agent's view of available skills
    pub fn refresh_skills(&mut self, cx: &mut Context<Self>) {
        if !self.has_open_project(cx) {
            return;
        }

        self.ensure_native_agent_connection(cx);
        let Some(connect_task) = self.connection_store.update(cx, |store, cx| {
            store
                .entry(&Agent::NativeAgent)
                .map(|entry| entry.read(cx).wait_for_connection())
        }) else {
            return;
        };
        let project = self.project.clone();
        cx.spawn(async move |_this, cx| -> Result<()> {
            let connected = connect_task.await?;
            if let Some(native_connection) = connected
                .connection
                .downcast::<agent::NativeAgentConnection>()
            {
                cx.update(|cx| native_connection.refresh_skills_for_project(project, cx));
            }
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub(super) fn expand_message_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(conversation_view) = self.active_conversation_view() else {
            return;
        };

        let Some(active_thread) = conversation_view.read(cx).root_thread_view() else {
            return;
        };

        active_thread.update(cx, |active_thread, cx| {
            active_thread.expand_message_editor(&ExpandMessageEditor, window, cx);
            active_thread.focus_handle(cx).focus(window, cx);
        })
    }
}
