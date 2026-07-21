use super::*;

impl Sidebar {
    pub(super) fn activate_terminal_entry(
        &mut self,
        metadata: TerminalThreadMetadata,
        workspace: ThreadEntryWorkspace,
        retain: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match workspace {
            ThreadEntryWorkspace::Open(workspace) => {
                self.activate_terminal_in_workspace(&workspace, metadata, retain, window, cx);
            }
            ThreadEntryWorkspace::Closed {
                folder_paths,
                project_group_key,
            } => {
                self.open_workspace_and_activate_terminal(
                    metadata,
                    folder_paths,
                    &project_group_key,
                    window,
                    cx,
                );
            }
        }
    }

    pub(super) fn load_agent_terminal_in_workspace(
        workspace: &Entity<Workspace>,
        metadata: &TerminalThreadMetadata,
        focus: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let restore_terminal = |agent_panel: Entity<AgentPanel>,
                                metadata: &TerminalThreadMetadata,
                                focus: bool,
                                workspace: Option<&Workspace>,
                                window: &mut Window,
                                cx: &mut App| {
            agent_panel.update(cx, |panel, cx| {
                panel.restore_terminal(
                    metadata.clone(),
                    focus,
                    AgentThreadSource::Sidebar,
                    workspace,
                    window,
                    cx,
                );
            });
        };

        let mut existing_panel = None;
        workspace.update(cx, |workspace, cx| {
            if let Some(panel) = workspace.panel::<AgentPanel>(cx) {
                existing_panel = Some(panel);
            }
        });

        if let Some(agent_panel) = existing_panel {
            restore_terminal(agent_panel, metadata, focus, None, window, cx);
            workspace.update(cx, |workspace, cx| {
                if focus {
                    workspace.focus_panel::<AgentPanel>(window, cx);
                } else {
                    workspace.reveal_panel::<AgentPanel>(window, cx);
                }
            });
            return;
        }

        let workspace = workspace.downgrade();
        let metadata = metadata.clone();
        let mut async_window_cx = window.to_async(cx);
        cx.spawn(async move |_cx| {
            let panel = AgentPanel::load(workspace.clone(), async_window_cx.clone()).await?;

            workspace.update_in(&mut async_window_cx, |workspace, window, cx| {
                let panel = workspace.panel::<AgentPanel>(cx).unwrap_or_else(|| {
                    workspace.add_panel(panel.clone(), window, cx);
                    panel.clone()
                });
                restore_terminal(panel, &metadata, focus, Some(workspace), window, cx);
                if focus {
                    workspace.focus_panel::<AgentPanel>(window, cx);
                } else {
                    workspace.reveal_panel::<AgentPanel>(window, cx);
                }
            })?;

            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub(super) fn activate_terminal_in_workspace(
        &mut self,
        workspace: &Entity<Workspace>,
        metadata: TerminalThreadMetadata,
        retain: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };

        let terminal_id = metadata.terminal_id;
        self.record_terminal_access(terminal_id);
        self.active_entry = Some(ActiveEntry::Terminal {
            terminal_id,
            workspace: workspace.clone(),
        });

        multi_workspace.update(cx, |multi_workspace, cx| {
            multi_workspace.activate(workspace.clone(), None, window, cx);
            if retain {
                multi_workspace.retain_active_workspace(cx);
            }
        });

        Self::load_agent_terminal_in_workspace(workspace, &metadata, true, window, cx);

        self.update_entries(cx);
    }

    pub(super) fn open_workspace_and_activate_terminal(
        &mut self,
        metadata: TerminalThreadMetadata,
        folder_paths: PathList,
        project_group_key: &ProjectGroupKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };

        let host = project_group_key.host();
        let provisional_key = Some(project_group_key.clone());
        let active_workspace = multi_workspace.read(cx).workspace().clone();
        let modal_workspace = active_workspace.clone();

        let open_task = multi_workspace.update(cx, |this, cx| {
            this.find_or_create_workspace(
                folder_paths,
                host,
                provisional_key,
                |options, window, cx| connect_remote(active_workspace, options, window, cx),
                &[],
                None,
                OpenMode::Activate,
                window,
                cx,
            )
        });

        cx.spawn_in(window, async move |this, cx| {
            let result = open_task.await;
            remote_connection::dismiss_connection_modal(&modal_workspace, cx);
            let workspace = result?;
            this.update_in(cx, |this, window, cx| {
                this.activate_terminal_in_workspace(&workspace, metadata, false, window, cx);
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }
}
