use super::*;

/// Test-only helper methods
impl AgentPanel {
    pub fn test_new(workspace: &Workspace, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new(workspace, window, cx)
    }

    /// Drops a thread's `ConversationView` from `retained_threads` without
    /// deleting its metadata or kvp state. Simulates the post-restart
    pub fn test_unload_retained_thread(&mut self, id: ThreadId) -> bool {
        self.retained_threads.remove(&id).is_some()
    }

    /// Opens an external thread using an arbitrary AgentServer.
    ///
    /// This is a test-only helper that allows visual tests and integration tests
    /// to inject a stub server without modifying production code paths.
    /// Not compiled into production builds.
    pub fn open_external_thread_with_server(
        &mut self,
        server: Rc<dyn AgentServer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ext_agent = Agent::Custom {
            id: server.agent_id(),
        };

        let thread = self.create_agent_thread_with_server(
            ext_agent,
            Some(server),
            None,
            None,
            None,
            None,
            None,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
        self.set_base_view(thread.into(), true, window, cx);
    }

    /// Opens a restored external thread with an arbitrary AgentServer and
    /// a specific `resume_session_id` — as if we just restored from the KVP.
    ///
    /// Test-only helper. Not compiled into production builds.
    pub fn open_restored_thread_with_server(
        &mut self,
        server: Rc<dyn AgentServer>,
        resume_session_id: acp::SessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ext_agent = Agent::Custom {
            id: server.agent_id(),
        };

        // The panel addresses threads by `ThreadId` after the draft work;
        // map the test-provided `session_id` back through the metadata
        // store so this helper still resumes the right thread.
        let resume_thread_id = ThreadMetadataStore::try_global(cx).and_then(|store| {
            store
                .read(cx)
                .entry_by_session(&resume_session_id)
                .map(|m| m.thread_id)
        });

        let thread = self.create_agent_thread_with_server(
            ext_agent,
            Some(server),
            resume_thread_id,
            None,
            None,
            None,
            None,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
        self.set_base_view(thread.into(), true, window, cx);
    }

    /// Returns the currently active thread view, if any.
    ///
    /// This is a test-only accessor that exposes the private `active_thread_view()`
    /// method for test assertions. Not compiled into production builds.
    pub fn active_thread_view_for_tests(&self) -> Option<&Entity<ConversationView>> {
        self.active_conversation_view()
    }

    /// Creates a draft thread using a stub server and sets it as the active view.
    pub fn open_draft_with_server(
        &mut self,
        server: Rc<dyn AgentServer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ext_agent = Agent::Custom {
            id: server.agent_id(),
        };
        let thread = self.create_agent_thread_with_server(
            ext_agent,
            Some(server),
            None,
            None,
            None,
            None,
            None,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
        self.draft_thread = Some(thread.conversation_view.clone());
        self.set_base_view(thread.into(), true, window, cx);
    }

    pub fn insert_test_terminal(
        &mut self,
        title: impl Into<String>,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<TerminalId> {
        let terminal_id = TerminalId::new();
        self.set_last_created_entry_kind_from_user_action(AgentPanelEntryKind::Terminal, cx);
        self.insert_display_only_terminal(
            terminal_id,
            None,
            Some(SharedString::from(title.into())),
            None,
            None,
            focus,
            focus,
            true,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        )?;
        Ok(terminal_id)
    }

    pub fn restore_test_terminal(
        &mut self,
        metadata: TerminalThreadMetadata,
        focus: bool,
        source: AgentThreadSource,
        workspace: Option<&Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if self.has_terminal(metadata.terminal_id) {
            self.activate_terminal(metadata.terminal_id, focus, window, cx);
            return Ok(());
        }

        if !self.supports_terminal(cx) {
            return Ok(());
        }

        let working_directory = self.terminal_restore_working_directory(&metadata, workspace, cx);
        let initial_title = Self::terminal_restore_initial_title(&metadata);
        self.insert_display_only_terminal(
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
        )
    }

    pub(super) fn insert_display_only_terminal(
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
    ) -> Result<()> {
        let init_command = Self::terminal_init_command(run_init_command, cx);
        let settings = terminal::terminal_settings::TerminalSettings::get_global(cx).clone();
        let path_style = self.project.read(cx).path_style(cx);
        let builder = terminal::TerminalBuilder::new_display_only(
            settings.cursor_shape,
            settings.alternate_scroll,
            settings.max_scroll_history_lines,
            cx.entity_id().as_u64(),
            cx.background_executor(),
            path_style,
        );
        let terminal = cx.new(|cx| builder.subscribe(cx));
        let terminal_for_init_command = terminal.clone();
        let terminal_view = cx.new(|cx| {
            let mut view = TerminalView::new(
                terminal,
                self.workspace.clone(),
                self.workspace_id,
                self.project.downgrade(),
                window,
                cx,
            );
            view.set_show_workspace_actions(false, cx);
            view
        });
        self.insert_terminal(
            terminal_id,
            terminal_view,
            working_directory,
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
        Ok(())
    }

    pub fn emit_test_terminal_bell(&mut self, terminal_id: TerminalId, cx: &mut Context<Self>) {
        let Some(terminal_entity) = self
            .terminals
            .get(&terminal_id)
            .map(|terminal| terminal.view.read(cx).terminal().clone())
        else {
            return;
        };
        terminal_entity.update(cx, |_terminal, cx| {
            cx.emit(TerminalEvent::Bell);
        });
    }

    pub fn emit_test_terminal_close(&mut self, terminal_id: TerminalId, cx: &mut Context<Self>) {
        let Some(terminal_entity) = self
            .terminals
            .get(&terminal_id)
            .map(|terminal| terminal.view.read(cx).terminal().clone())
        else {
            return;
        };
        terminal_entity.update(cx, |_terminal, cx| {
            cx.emit(TerminalEvent::CloseTerminal);
        });
    }
}
