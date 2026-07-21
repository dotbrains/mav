use super::*;

impl Sidebar {
    pub(super) fn toggle_agent_options_menu(
        &mut self,
        _: &ToggleOptionsMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.focus(&self.focus_handle, cx);
        self.agent_options_menu_handle.toggle(window, cx);
    }

    pub(super) fn active_workspace(&self, cx: &App) -> Option<Entity<Workspace>> {
        self.multi_workspace
            .upgrade()
            .map(|w| w.read(cx).workspace().clone())
    }

    pub(super) fn show_thread_import_modal(
        &mut self,
        source: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        telemetry::event!(
            "Agent Threads Import Clicked",
            source = source,
            side = match self.side(cx) {
                SidebarSide::Left => "left",
                SidebarSide::Right => "right",
            }
        );

        let Some(active_workspace) = self.active_workspace(cx) else {
            return;
        };

        let Some(agent_registry_store) = AgentRegistryStore::try_global(cx) else {
            return;
        };

        let agent_server_store = active_workspace
            .read(cx)
            .project()
            .read(cx)
            .agent_server_store()
            .clone();

        let workspace_handle = active_workspace.downgrade();
        let multi_workspace = self.multi_workspace.clone();

        active_workspace.update(cx, |workspace, cx| {
            workspace.toggle_modal(window, cx, |window, cx| {
                ThreadImportModal::new(
                    agent_server_store,
                    agent_registry_store,
                    workspace_handle.clone(),
                    multi_workspace.clone(),
                    window,
                    cx,
                )
            });
        });
    }

    pub(super) fn should_render_acp_import_onboarding(&self, cx: &App) -> bool {
        let has_external_agents = self
            .active_workspace(cx)
            .map(|ws| {
                ws.read(cx)
                    .project()
                    .read(cx)
                    .agent_server_store()
                    .read(cx)
                    .has_external_agents()
            })
            .unwrap_or(false);

        has_external_agents && !AcpThreadImportOnboarding::dismissed(cx)
    }

    pub(super) fn render_acp_import_onboarding(
        &mut self,
        verbose_labels: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let on_import = cx.listener(|this, _, window, cx| {
            this.show_archive(window, cx);
            this.show_thread_import_modal("external_agent_onboarding", window, cx);
        });
        render_import_onboarding_banner(
            "acp",
            "Looking for threads from external agents?",
            "Import threads from agents like Claude Agent, Codex, and more, whether started in Mav or another client.",
            if verbose_labels {
                "Import Threads from External Agents"
            } else {
                "Import Threads"
            },
            |_, _window, cx| AcpThreadImportOnboarding::dismiss(cx),
            on_import,
            cx,
        )
    }

    pub(super) fn should_render_cross_channel_import_onboarding(&self, cx: &App) -> bool {
        !CrossChannelImportOnboarding::dismissed(cx)
            && !self.cross_channel_import_channels.is_empty()
    }

    pub(super) fn render_cross_channel_import_onboarding(
        &mut self,
        verbose_labels: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let channel_names = self
            .cross_channel_import_channels
            .iter()
            .map(SharedString::as_str)
            .join(" and ");

        let description = format!(
            "Import threads from {} to continue where you left off.",
            channel_names
        );

        let on_import = cx.listener(|this, _, _window, cx| {
            telemetry::event!(
                "Agent Threads Import Clicked",
                source = "cross_channel_onboarding",
                side = match this.side(cx) {
                    SidebarSide::Left => "left",
                    SidebarSide::Right => "right",
                }
            );
            CrossChannelImportOnboarding::dismiss(cx);
            if let Some(workspace) = this.active_workspace(cx) {
                workspace.update(cx, |workspace, cx| {
                    import_threads_from_other_channels(workspace, cx);
                });
            }
        });
        render_import_onboarding_banner(
            "channel",
            "Threads found from other channels",
            description,
            if verbose_labels {
                "Import Threads from Other Channels"
            } else {
                "Import Threads"
            },
            |_, _window, cx| CrossChannelImportOnboarding::dismiss(cx),
            on_import,
            cx,
        )
    }

    pub(super) fn toggle_archive(
        &mut self,
        _: &ToggleThreadHistory,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.view {
            SidebarView::ThreadList => {
                self.show_archive(window, cx);
            }
            SidebarView::Archive(_) => self.show_thread_list(window, cx),
        }
    }

    pub(super) fn show_archive(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = match self.side(cx) {
            SidebarSide::Left => "left",
            SidebarSide::Right => "right",
        };
        telemetry::event!("Thread History Viewed", side = side);

        let Some(active_workspace) = self
            .multi_workspace
            .upgrade()
            .map(|w| w.read(cx).workspace().clone())
        else {
            return;
        };
        let project = active_workspace.read(cx).project().clone();
        let agent_server_store = project.read(cx).agent_server_store().downgrade();
        let agent_connection_store = connection_store_for_project(&project, cx).downgrade();

        let archive_view = cx.new(|cx| {
            ThreadsArchiveView::new(
                active_workspace.downgrade(),
                agent_connection_store.clone(),
                agent_server_store.clone(),
                window,
                cx,
            )
        });

        let subscription = cx.subscribe_in(
            &archive_view,
            window,
            |this, _, event: &ThreadsArchiveViewEvent, window, cx| match event {
                ThreadsArchiveViewEvent::Close => {
                    this.show_thread_list(window, cx);
                }
                ThreadsArchiveViewEvent::Activate { thread } => {
                    this.open_thread_from_archive(thread.clone(), window, cx);
                }
                ThreadsArchiveViewEvent::CancelRestore { thread_id } => {
                    this.restoring_tasks.remove(thread_id);
                }
                ThreadsArchiveViewEvent::Import => {
                    this.show_thread_import_modal("thread_history", window, cx);
                }
                ThreadsArchiveViewEvent::NewThread => {
                    this.show_thread_list(window, cx);
                    if let Some(workspace) = this.active_workspace(cx) {
                        this.create_new_entry(&workspace, window, cx);
                    }
                }
            },
        );

        self._subscriptions.push(subscription);
        self.view = SidebarView::Archive(archive_view.clone());
        self.serialize(cx);
        cx.notify();
    }

    pub(super) fn show_thread_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.view = SidebarView::ThreadList;
        self._subscriptions.clear();
        self.focus_handle.focus(window, cx);
        self.serialize(cx);
        cx.notify();
    }
}
