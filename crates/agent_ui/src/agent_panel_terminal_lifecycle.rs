use super::*;

impl AgentPanel {
    pub fn new_terminal(
        &mut self,
        workspace: Option<&Workspace>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.supports_terminal(cx) {
            return;
        }
        self.set_last_created_entry_kind_from_user_action(AgentPanelEntryKind::Terminal, cx);
        let working_directory = self.terminal_working_directory(workspace, cx);
        self.spawn_terminal(
            TerminalId::new(),
            working_directory,
            None,
            None,
            None,
            true,
            true,
            true,
            source,
            window,
            cx,
        );
    }

    pub(super) fn terminal_working_directory(
        &self,
        workspace: Option<&Workspace>,
        cx: &App,
    ) -> Option<PathBuf> {
        workspace
            .map(|workspace| terminal_view::default_working_directory(workspace, cx))
            .unwrap_or_else(|| self.default_terminal_working_directory(cx))
    }

    pub fn supports_terminal(&self, cx: &App) -> bool {
        self.has_open_project(cx) && self.project.read(cx).supports_terminal(cx)
    }

    pub fn should_create_terminal_for_new_entry(&self, cx: &App) -> bool {
        self.last_created_entry_kind == AgentPanelEntryKind::Terminal
            && self.project.read(cx).supports_terminal(cx)
    }

    pub(super) fn set_last_created_entry_kind_from_user_action(
        &mut self,
        entry_kind: AgentPanelEntryKind,
        cx: &mut Context<Self>,
    ) {
        if self.last_created_entry_kind != entry_kind {
            self.last_created_entry_kind = entry_kind;
            self.serialize(cx);
        }

        cx.background_spawn({
            let kvp = KeyValueStore::global(cx);
            async move {
                write_global_last_created_entry_kind(kvp, entry_kind).await;
            }
        })
        .detach();
    }

    pub(super) fn spawn_terminal(
        &mut self,
        terminal_id: TerminalId,
        working_directory: Option<PathBuf>,
        custom_title: Option<SharedString>,
        initial_title: Option<SharedString>,
        created_at: Option<DateTime<Utc>>,
        select: bool,
        focus: bool,
        run_init_command: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let terminal_working_directory = working_directory.clone();
        let init_command = Self::terminal_init_command(run_init_command, cx);
        let terminal_task = self.project.update(cx, |project, cx| {
            project.create_terminal_shell(working_directory, cx)
        });
        let workspace = self.workspace.clone();
        let workspace_id = self.workspace_id;
        let project = self.project.downgrade();

        cx.spawn_in(window, async move |this, cx| {
            let terminal = match terminal_task.await {
                Ok(terminal) => terminal,
                Err(error) => {
                    log::error!("failed to spawn agent panel terminal: {error:#}");
                    workspace
                        .update(cx, |workspace, cx| workspace.show_error(error, cx))
                        .log_err();
                    this.update(cx, |this, cx| {
                        if this.pending_terminal_spawn == Some(terminal_id) {
                            this.pending_terminal_spawn = None;
                            cx.notify();
                        }
                    })
                    .log_err();
                    return anyhow::Ok(());
                }
            };
            this.update_in(cx, |this, window, cx| {
                let terminal_for_init_command = terminal.clone();
                let terminal_view = cx.new(|cx| {
                    let mut view =
                        TerminalView::new(terminal, workspace, workspace_id, project, window, cx);
                    view.set_show_workspace_actions(false, cx);
                    view
                });
                this.insert_terminal(
                    terminal_id,
                    terminal_view,
                    terminal_working_directory,
                    custom_title,
                    initial_title,
                    created_at,
                    select,
                    focus,
                    source,
                    window,
                    cx,
                );
                Self::write_terminal_init_command(&terminal_for_init_command, init_command, cx);
            })?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub(super) fn terminal_init_command(run_init_command: bool, cx: &App) -> Option<String> {
        run_init_command
            .then(|| AgentSettings::get_global(cx).terminal_init_command.clone())
            .flatten()
            .filter(|command| !command.trim().is_empty())
    }

    pub(super) fn write_terminal_init_command(
        terminal: &Entity<terminal::Terminal>,
        init_command: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(command) = init_command else {
            return;
        };

        if !terminal.read(cx).is_pty() {
            terminal.update(cx, |terminal, _| {
                terminal.write_init_command(Self::terminal_init_command_input(command))
            });
            return;
        }

        let startup = terminal.update(cx, |terminal, _| {
            terminal.start_init_command_startup_handshake()
        });

        let terminal = terminal.downgrade();
        cx.spawn(async move |_this, cx| {
            // Fall back to the timeout so the init command is still delivered if
            // the shell never echoes the marker.
            let timeout = cx
                .background_executor()
                .timer(TERMINAL_INIT_COMMAND_STARTUP_TIMEOUT);
            futures::select_biased! {
                _ = startup.fuse() => {}
                _ = timeout.fuse() => {}
            }

            let input = Self::terminal_init_command_input(command);
            if let Err(error) = terminal.update(cx, move |terminal, cx| {
                if !terminal.write_init_command_after_startup(input, cx) {
                    log::debug!(
                        "skipping terminal init command because the terminal is no longer eligible"
                    );
                }
            }) {
                log::debug!("skipping terminal init command because the terminal closed: {error}");
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn terminal_init_command_input(command: String) -> Vec<u8> {
        let mut input = command.into_bytes();
        // CR, not "\r\n": "\r\n" puts PowerShell into continuation
        // mode (same convention as the activation-script writes in
        // `TerminalBuilder::new`).
        input.push(b'\x0d');
        input
    }

    pub(super) fn insert_terminal(
        &mut self,
        terminal_id: TerminalId,
        terminal_view: Entity<TerminalView>,
        working_directory: Option<PathBuf>,
        custom_title: Option<SharedString>,
        initial_title: Option<SharedString>,
        created_at: Option<DateTime<Utc>>,
        select: bool,
        focus: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(custom_title) = custom_title {
            terminal_view.update(cx, |terminal_view, cx| {
                terminal_view.set_custom_title(Some(custom_title.to_string()), cx);
            });
        }
        let terminal_entity = terminal_view.read(cx).terminal().clone();
        let view_subscription = cx.subscribe(
            &terminal_view,
            move |this, _terminal_view, event: &ItemEvent, cx| match event {
                ItemEvent::UpdateTab | ItemEvent::UpdateBreadcrumbs => {
                    this.refresh_terminal_metadata(terminal_id, cx);
                }
                ItemEvent::CloseItem | ItemEvent::Edit => {}
            },
        );
        // Listen on the underlying `Terminal` entity for shell-driven metadata
        // changes and bell.
        let terminal_subscription = cx.subscribe_in(
            &terminal_entity,
            window,
            move |this, _terminal, event: &TerminalEvent, window, cx| match event {
                TerminalEvent::TitleChanged
                | TerminalEvent::Wakeup
                | TerminalEvent::BreadcrumbsChanged => {
                    this.refresh_terminal_metadata(terminal_id, cx);
                    this.report_terminal_program(terminal_id, source, cx);
                }
                TerminalEvent::Bell => this.mark_terminal_notification(terminal_id, window, cx),
                TerminalEvent::CloseTerminal => {
                    this.close_terminal_from_terminal_event(terminal_id, window, cx);
                }
                TerminalEvent::BlinkChanged(_)
                | TerminalEvent::SelectionsChanged
                | TerminalEvent::NewNavigationTarget(_)
                | TerminalEvent::Open(_) => {}
            },
        );

        let last_known_terminal_title = initial_title
            .map(|title| title.to_string())
            .unwrap_or_default();
        let mut terminal = AgentTerminal {
            view: terminal_view,
            title_editor: None,
            title_editor_initial_title: None,
            title_editor_subscription: None,
            last_known_title: last_known_terminal_title.clone(),
            last_known_terminal_title,
            last_observed_program: None,
            working_directory,
            created_at: created_at.unwrap_or_else(Utc::now),
            has_notification: false,
            notification_windows: Vec::new(),
            notification_subscriptions: Vec::new(),
            _subscriptions: vec![view_subscription, terminal_subscription],
        };
        if self.pending_terminal_spawn == Some(terminal_id) {
            self.pending_terminal_spawn = None;
        }
        terminal.refresh_metadata(cx);
        terminal.report_started_terminal_program(terminal_id, source, cx);
        self.terminals.insert(terminal_id, terminal);
        self.persist_terminal_metadata(terminal_id, cx);
        self.emit_terminal_thread_started(terminal_id, source, cx);
        if select {
            self.set_base_view(BaseView::Terminal { terminal_id }, focus, window, cx);
        }
        cx.emit(AgentPanelEvent::EntryChanged);
        cx.notify();
    }

    pub fn activate_terminal(
        &mut self,
        terminal_id: TerminalId,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal) = self.terminals.get_mut(&terminal_id) else {
            return;
        };
        let had_notification = terminal.has_notification;
        terminal.has_notification = false;
        if had_notification {
            self.dismiss_terminal_notifications(terminal_id, cx);
        }
        self.set_base_view(BaseView::Terminal { terminal_id }, focus, window, cx);
        if had_notification {
            cx.emit(AgentPanelEvent::EntryChanged);
            cx.notify();
        }
    }

    pub fn close_terminal(
        &mut self,
        terminal_id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_internal(terminal_id, true, None, window, cx);
    }

    pub fn close_terminal_without_activating_draft(
        &mut self,
        terminal_id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal_internal(terminal_id, false, None, window, cx);
    }

    fn close_terminal_internal(
        &mut self,
        terminal_id: TerminalId,
        activate_draft_after_close: bool,
        terminal_closed_metadata: Option<TerminalThreadMetadata>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_active = self.active_terminal_id() == Some(terminal_id);

        if self.pending_terminal_spawn == Some(terminal_id) {
            self.pending_terminal_spawn = None;
        }
        self.dismiss_terminal_notifications(terminal_id, cx);
        if self.terminals.remove(&terminal_id).is_none() {
            return;
        }
        if let Some(store) = TerminalThreadMetadataStore::try_global(cx) {
            store.update(cx, |store, cx| {
                store.delete(terminal_id, cx);
            });
        }
        if was_active {
            self.base_view = BaseView::Uninitialized;
            self.refresh_base_view_subscriptions(window, cx);
            if activate_draft_after_close {
                self.activate_draft(false, AgentThreadSource::AgentPanel, window, cx);
            }
        }

        if let Some(metadata) = terminal_closed_metadata {
            cx.emit(AgentPanelEvent::TerminalClosed { metadata });
        }
        cx.emit(AgentPanelEvent::EntryChanged);
        cx.notify();
    }

    pub(super) fn close_terminal_from_terminal_event(
        &mut self,
        terminal_id: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let metadata = self.terminal_metadata(terminal_id, cx);
        self.close_terminal_internal(terminal_id, false, metadata, window, cx);
    }
}
