use super::*;

impl DebugPanel {
    pub fn new(
        workspace: &Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let project = workspace.project().clone();
            let focus_handle = cx.focus_handle();
            let thread_picker_menu_handle = PopoverMenuHandle::default();
            let session_picker_menu_handle = PopoverMenuHandle::default();

            let focus_subscription = cx.on_focus(
                &focus_handle,
                window,
                |this: &mut DebugPanel, window, cx| {
                    this.focus_active_item(window, cx);
                },
            );

            Self {
                sessions_with_children: Default::default(),
                active_session: None,
                focus_handle,
                breakpoint_list: BreakpointList::new(
                    None,
                    workspace.weak_handle(),
                    &project,
                    window,
                    cx,
                ),
                project,
                workspace: workspace.weak_handle(),
                context_menu: None,
                fs: workspace.app_state().fs.clone(),
                thread_picker_menu_handle,
                session_picker_menu_handle,
                is_zoomed: false,
                _subscriptions: [focus_subscription],
                debug_scenario_scheduled_last: true,
            }
        })
    }

    pub(crate) fn focus_active_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.active_session.clone() else {
            return;
        };
        let active_pane = session
            .read(cx)
            .running_state()
            .read(cx)
            .active_pane()
            .clone();
        active_pane.update(cx, |pane, cx| {
            pane.focus_active_item(window, cx);
        });
    }

    #[cfg(test)]
    pub(crate) fn sessions(&self) -> impl Iterator<Item = Entity<DebugSession>> {
        self.sessions_with_children.keys().cloned()
    }

    pub fn active_session(&self) -> Option<Entity<DebugSession>> {
        self.active_session.clone()
    }

    pub(crate) fn running_state(&self, cx: &mut App) -> Option<Entity<RunningState>> {
        self.active_session()
            .map(|session| session.read(cx).running_state().clone())
    }

    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }
}
