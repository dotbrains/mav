use super::*;

impl AgentPanel {
    pub(super) fn emit_terminal_thread_started(
        &self,
        terminal_id: TerminalId,
        source: AgentThreadSource,
        cx: &App,
    ) {
        telemetry::event!(
            "Agent Thread Started",
            agent = TERMINAL_AGENT_TELEMETRY_ID,
            terminal_id = terminal_id.to_key_string(),
            source = source.as_str(),
            side = crate::sidebar_side(cx),
            thread_location = "current_worktree",
        );
    }

    pub(super) fn refresh_terminal_metadata(
        &mut self,
        terminal_id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.terminals.get_mut(&terminal_id)
            && terminal.refresh_metadata(cx)
        {
            self.persist_terminal_metadata(terminal_id, cx);
            cx.emit(AgentPanelEvent::EntryChanged);
            cx.notify();
        }
    }

    pub(super) fn report_terminal_program(
        &mut self,
        terminal_id: TerminalId,
        source: AgentThreadSource,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.terminals.get_mut(&terminal_id) {
            terminal.report_started_terminal_program(terminal_id, source, cx);
        }
    }

    pub(super) fn persist_all_terminal_metadata(&self, cx: &mut Context<Self>) {
        let terminal_ids = self.terminals.keys().copied().collect::<Vec<_>>();
        for terminal_id in terminal_ids {
            self.persist_terminal_metadata(terminal_id, cx);
        }
    }

    pub(super) fn persist_terminal_metadata(
        &self,
        terminal_id: TerminalId,
        cx: &mut Context<Self>,
    ) {
        let Some(store) = TerminalThreadMetadataStore::try_global(cx) else {
            return;
        };
        let Some(metadata) = self.terminal_metadata(terminal_id, cx) else {
            return;
        };
        store.update(cx, |store, cx| {
            store.save(metadata, cx);
        });
    }

    pub(super) fn terminal_metadata(
        &self,
        terminal_id: TerminalId,
        cx: &App,
    ) -> Option<TerminalThreadMetadata> {
        let terminal = self.terminals.get(&terminal_id)?;
        let project = self.project.read(cx);
        Some(TerminalThreadMetadata {
            terminal_id,
            title: terminal.terminal_title(cx),
            custom_title: terminal.custom_title(cx),
            created_at: terminal.created_at,
            worktree_paths: project.worktree_paths(cx),
            remote_connection: project.remote_connection_options(cx),
            working_directory: terminal.working_directory.clone(),
        })
    }

    pub fn restore_terminal(
        &mut self,
        metadata: TerminalThreadMetadata,
        focus: bool,
        source: AgentThreadSource,
        workspace: Option<&Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_terminal(metadata.terminal_id) {
            self.activate_terminal(metadata.terminal_id, focus, window, cx);
            return;
        }

        if !self.supports_terminal(cx) {
            return;
        }

        self.pending_terminal_spawn = Some(metadata.terminal_id);
        let working_directory = self.terminal_restore_working_directory(&metadata, workspace, cx);
        let initial_title = Self::terminal_restore_initial_title(&metadata);
        self.spawn_terminal(
            metadata.terminal_id,
            working_directory,
            metadata.custom_title.clone(),
            initial_title,
            Some(metadata.created_at),
            true,
            focus,
            true,
            source,
            window,
            cx,
        );
    }

    pub(super) fn restore_terminal_for_panel_load(
        &mut self,
        metadata: TerminalThreadMetadata,
        focus: bool,
        source: AgentThreadSource,
        workspace: Option<&Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        self.restore_test_terminal(metadata, focus, source, workspace, window, cx)
            .log_err();

        #[cfg(not(test))]
        self.restore_terminal(metadata, focus, source, workspace, window, cx);
    }

    pub(super) fn terminal_restore_working_directory(
        &self,
        metadata: &TerminalThreadMetadata,
        workspace: Option<&Workspace>,
        cx: &App,
    ) -> Option<PathBuf> {
        if let Some(working_directory) = metadata.working_directory.clone() {
            return Some(working_directory);
        }

        if let Some(workspace) = workspace {
            return terminal_view::default_working_directory(workspace, cx);
        }

        self.default_terminal_working_directory(cx)
    }

    pub(super) fn terminal_restore_initial_title(
        metadata: &TerminalThreadMetadata,
    ) -> Option<SharedString> {
        (!metadata.title.is_empty()).then(|| metadata.title.clone())
    }
}
