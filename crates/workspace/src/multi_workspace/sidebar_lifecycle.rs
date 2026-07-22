use super::*;

impl MultiWorkspace {
    pub fn sidebar_side(&self, cx: &App) -> SidebarSide {
        self.sidebar
            .as_ref()
            .map_or(SidebarSide::Left, |s| s.side(cx))
    }

    pub fn sidebar_render_state(&self, cx: &App) -> SidebarRenderState {
        SidebarRenderState {
            open: self.sidebar_open(),
            side: self.sidebar_side(cx),
        }
    }

    pub fn new(workspace: Entity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sidebar_open = SidebarSettings::get_global(cx).starts_open;
        Self::new_with_initial_sidebar_open(workspace, window, cx, sidebar_open)
    }

    pub(crate) fn new_with_initial_sidebar_open(
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
        sidebar_open: bool,
    ) -> Self {
        let release_subscription = cx.on_release(|this: &mut MultiWorkspace, _cx| {
            if let Some(task) = this._serialize_task.take() {
                task.detach();
            }
            for task in std::mem::take(&mut this.pending_removal_tasks) {
                task.detach();
            }
        });
        let quit_subscription = cx.on_app_quit(Self::app_will_quit);
        Self::subscribe_to_workspace(&workspace, window, cx);
        let weak_self = cx.weak_entity();
        let active_workspace_id = Rc::new(Cell::new(workspace.entity_id()));
        workspace.update(cx, |workspace, cx| {
            workspace.set_multi_workspace(weak_self, active_workspace_id.clone(), cx);
        });
        let mut multi_workspace = Self {
            window_id: window.window_handle().window_id(),
            retained_workspaces: Vec::new(),
            project_groups: Vec::new(),
            active_workspace: workspace,
            active_workspace_id,
            sidebar: None,
            sidebar_open: false,
            pending_sidebar_state: None,
            sidebar_overlay: None,
            pending_removal_tasks: Vec::new(),
            _serialize_task: None,
            _subscriptions: vec![release_subscription, quit_subscription],
            previous_focus_handle: None,
        };

        if sidebar_open {
            multi_workspace.apply_open_sidebar(false, cx);
        }

        multi_workspace
    }

    pub fn register_sidebar<T: Sidebar>(
        &mut self,
        sidebar: Entity<T>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._subscriptions
            .push(cx.observe(&sidebar, |_this, _, cx| {
                cx.notify();
            }));
        self._subscriptions
            .push(cx.subscribe(&sidebar, |this, _, event, cx| match event {
                SidebarEvent::SerializeNeeded => {
                    this.serialize(cx);
                }
            }));
        self.sidebar = Some(Box::new(sidebar));
        if self.sidebar_open {
            let sidebar_focus_handle = self.sidebar.as_ref().map(|s| s.focus_handle(cx));
            for workspace in self.retained_workspaces.clone() {
                workspace.update(cx, |workspace, _cx| {
                    workspace.set_sidebar_focus_handle(sidebar_focus_handle.clone());
                });
            }
        }

        if let Some(state) = self.pending_sidebar_state.take()
            && let Some(sidebar) = &self.sidebar
        {
            sidebar.restore_serialized_state(&state, window, cx);
            self.serialize(cx);
        }
        cx.notify();
    }

    pub fn sidebar(&self) -> Option<&dyn SidebarHandle> {
        self.sidebar.as_deref()
    }

    pub fn set_sidebar_overlay(&mut self, overlay: Option<AnyView>, cx: &mut Context<Self>) {
        self.sidebar_overlay = overlay;
        cx.notify();
    }

    pub fn sidebar_open(&self) -> bool {
        self.sidebar_open
    }

    pub fn sidebar_has_notifications(&self, cx: &App) -> bool {
        self.sidebar
            .as_ref()
            .map_or(false, |s| s.has_notifications(cx))
    }

    pub fn is_threads_list_view_active(&self, cx: &App) -> bool {
        self.sidebar
            .as_ref()
            .map_or(false, |s| s.is_threads_list_view_active(cx))
    }

    pub fn simulate_update_available(&mut self, cx: &mut Context<Self>) {
        if let Some(sidebar) = &self.sidebar {
            sidebar.simulate_update_available(cx);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn open_application_menu(
        &mut self,
        menu_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(sidebar) = &self.sidebar {
            sidebar.open_application_menu(menu_name, window, cx);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn activate_application_menu(
        &mut self,
        right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(sidebar) = &self.sidebar {
            sidebar.activate_application_menu(right, window, cx);
        }
    }

    pub fn multi_workspace_enabled(&self, _cx: &App) -> bool {
        true
    }

    pub fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.multi_workspace_enabled(cx) {
            return;
        }

        if self.sidebar_open() {
            self.close_sidebar(window, cx);
        } else {
            self.previous_focus_handle = window.focused(cx);
            self.open_sidebar(cx);
            if let Some(sidebar) = &self.sidebar {
                sidebar.prepare_for_focus(window, cx);
                sidebar.focus(window, cx);
            }
        }
    }

    pub fn close_sidebar_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.multi_workspace_enabled(cx) {
            return;
        }

        if self.sidebar_open() {
            self.close_sidebar(window, cx);
        }
    }

    pub fn focus_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.multi_workspace_enabled(cx) {
            return;
        }

        if self.sidebar_open() {
            let sidebar_is_focused = self
                .sidebar
                .as_ref()
                .is_some_and(|s| s.focus_handle(cx).contains_focused(window, cx));

            if sidebar_is_focused {
                self.restore_previous_focus(false, window, cx);
            } else {
                self.previous_focus_handle = window.focused(cx);
                if let Some(sidebar) = &self.sidebar {
                    sidebar.prepare_for_focus(window, cx);
                    sidebar.focus(window, cx);
                }
            }
        } else {
            self.previous_focus_handle = window.focused(cx);
            self.open_sidebar(cx);
            if let Some(sidebar) = &self.sidebar {
                sidebar.prepare_for_focus(window, cx);
                sidebar.focus(window, cx);
            }
        }
    }

    pub fn open_sidebar(&mut self, cx: &mut Context<Self>) {
        let side = match self.sidebar_side(cx) {
            SidebarSide::Left => "left",
            SidebarSide::Right => "right",
        };
        telemetry::event!("Sidebar Toggled", action = "open", side = side);
        self.apply_open_sidebar(true, cx);
    }

    pub(crate) fn restore_sidebar_open_state(
        &mut self,
        sidebar_open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if sidebar_open {
            self.apply_open_sidebar(false, cx);
        } else {
            self.apply_close_sidebar(false, window, false, cx);
        }
    }

    fn apply_open_sidebar(&mut self, serialize: bool, cx: &mut Context<Self>) {
        self.sidebar_open = true;
        self.retain_active_workspace_without_serializing(cx);
        let sidebar_focus_handle = self.sidebar.as_ref().map(|s| s.focus_handle(cx));
        for workspace in self.retained_workspaces.clone() {
            workspace.update(cx, |workspace, cx| {
                workspace.set_sidebar_focus_handle(sidebar_focus_handle.clone());
                workspace.notify_panes(cx);
            });
        }
        if serialize {
            self.serialize(cx);
        }
        cx.notify();
    }

    pub(crate) fn restore_sidebar_serialized_state(
        &mut self,
        state: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(sidebar) = &self.sidebar {
            sidebar.restore_serialized_state(&state, window, cx);
            self.serialize(cx);
        } else {
            self.pending_sidebar_state = Some(state);
        }
    }

    pub fn close_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = match self.sidebar_side(cx) {
            SidebarSide::Left => "left",
            SidebarSide::Right => "right",
        };
        telemetry::event!("Sidebar Toggled", action = "close", side = side);
        self.apply_close_sidebar(true, window, true, cx);
    }

    fn apply_close_sidebar(
        &mut self,
        restore_focus: bool,
        window: &mut Window,
        serialize: bool,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_open = false;
        for workspace in self.retained_workspaces.clone() {
            workspace.update(cx, |workspace, cx| {
                workspace.set_sidebar_focus_handle(None);
                workspace.notify_panes(cx);
            });
        }
        let sidebar_has_focus = self
            .sidebar
            .as_ref()
            .is_some_and(|s| s.focus_handle(cx).contains_focused(window, cx));
        if restore_focus && sidebar_has_focus {
            self.restore_previous_focus(true, window, cx);
        } else {
            self.previous_focus_handle.take();
        }
        if serialize {
            self.serialize(cx);
        }
        cx.notify();
    }

    fn restore_previous_focus(&mut self, clear: bool, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = if clear {
            self.previous_focus_handle.take()
        } else {
            self.previous_focus_handle.clone()
        };

        if let Some(previous_focus) = focus_handle {
            previous_focus.focus(window, cx);
        } else {
            let pane = self.workspace().read(cx).active_pane().clone();
            window.focus(&pane.read(cx).focus_handle(cx), cx);
        }
    }

    pub fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let workspaces = this.update(cx, |multi_workspace, _cx| {
                multi_workspace.workspaces().cloned().collect::<Vec<_>>()
            })?;

            for workspace in workspaces {
                let should_continue = workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.prepare_to_close(CloseIntent::CloseWindow, window, cx)
                    })?
                    .await?;
                if !should_continue {
                    return anyhow::Ok(());
                }
            }

            cx.update(|window, _cx| {
                window.remove_window();
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }
}
