use super::*;
use crate::agent_panel::agent_panel_thread_types::SourcePanelInitialization;

impl AgentPanel {
    pub(super) fn ensure_thread_initialized(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.base_view, BaseView::Uninitialized) {
            if self.pending_terminal_spawn.is_some() {
                return;
            }
            if self.should_create_terminal_for_new_entry(cx) {
                let terminal_id = TerminalId::new();
                self.pending_terminal_spawn = Some(terminal_id);
                cx.defer_in(window, move |this, window, cx| {
                    if matches!(this.base_view, BaseView::Uninitialized)
                        && this.pending_terminal_spawn == Some(terminal_id)
                        && this.should_create_terminal_for_new_entry(cx)
                    {
                        this.create_initial_terminal(
                            terminal_id,
                            AgentThreadSource::AgentPanel,
                            window,
                            cx,
                        );
                    } else if this.pending_terminal_spawn == Some(terminal_id) {
                        this.pending_terminal_spawn = None;
                    }
                });
            } else {
                self.activate_draft(false, AgentThreadSource::AgentPanel, window, cx);
            }
        }
    }

    fn create_initial_terminal(
        &mut self,
        terminal_id: TerminalId,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.supports_terminal(cx) {
            if self.pending_terminal_spawn == Some(terminal_id) {
                self.pending_terminal_spawn = None;
            }
            return;
        }
        let working_directory = self.terminal_working_directory(None, cx);
        self.spawn_initial_terminal(terminal_id, working_directory, source, window, cx);
    }

    #[cfg(not(test))]
    fn spawn_initial_terminal(
        &mut self,
        terminal_id: TerminalId,
        working_directory: Option<PathBuf>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spawn_terminal(
            terminal_id,
            working_directory,
            None,
            None,
            None,
            true,
            false,
            true,
            source,
            window,
            cx,
        );
    }

    #[cfg(test)]
    fn spawn_initial_terminal(
        &mut self,
        terminal_id: TerminalId,
        working_directory: Option<PathBuf>,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.insert_display_only_terminal(
            terminal_id,
            working_directory,
            None,
            None,
            None,
            true,
            false,
            true,
            source,
            window,
            cx,
        ) {
            log::error!("failed to spawn test agent panel terminal: {error:#}");
            if self.pending_terminal_spawn == Some(terminal_id) {
                self.pending_terminal_spawn = None;
                cx.notify();
            }
        }
    }

    fn destination_has_meaningful_state(&self, cx: &App) -> bool {
        if self.overlay_view.is_some()
            || !self.retained_threads.is_empty()
            || !self.terminals.is_empty()
        {
            return true;
        }

        match &self.base_view {
            BaseView::Uninitialized => false,
            BaseView::Terminal { .. } => true,
            BaseView::AgentThread { conversation_view } => {
                let has_entries = conversation_view
                    .read(cx)
                    .root_thread_view()
                    .is_some_and(|tv| !tv.read(cx).thread.read(cx).entries().is_empty());
                if has_entries {
                    return true;
                }

                conversation_view
                    .read(cx)
                    .root_thread_view()
                    .is_some_and(|thread_view| {
                        let thread_view = thread_view.read(cx);
                        thread_view
                            .thread
                            .read(cx)
                            .draft_prompt()
                            .is_some_and(|draft| !draft.is_empty())
                            || !thread_view
                                .message_editor
                                .read(cx)
                                .text(cx)
                                .trim()
                                .is_empty()
                    })
            }
        }
    }

    fn active_initial_content(&self, cx: &App) -> Option<AgentInitialContent> {
        let thread_view = self.active_thread_view(cx)?;
        let thread_view = thread_view.read(cx);
        let saved = thread_view
            .thread
            .read(cx)
            .draft_prompt()
            .map(|blocks| blocks.to_vec())
            .filter(|blocks| !blocks.is_empty());
        let blocks = saved.unwrap_or_else(|| {
            thread_view
                .message_editor
                .read(cx)
                .draft_content_blocks_snapshot(cx)
        });
        if blocks.is_empty() {
            return None;
        }
        Some(AgentInitialContent::ContentBlock {
            blocks,
            auto_submit: false,
        })
    }

    fn source_panel_initialization(
        source_workspace: &WeakEntity<Workspace>,
        cx: &App,
    ) -> Option<SourcePanelInitialization> {
        let source_workspace = source_workspace.upgrade()?;
        let source_panel = source_workspace.read(cx).panel::<AgentPanel>(cx)?;
        let source_panel = source_panel.read(cx);
        Some(SourcePanelInitialization {
            agent: source_panel.selected_agent(cx),
            initial_content: source_panel.active_initial_content(cx),
        })
    }

    pub fn initialize_from_source_workspace_if_needed(
        &mut self,
        source_workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.has_open_project(cx) {
            return false;
        }

        if self.destination_has_meaningful_state(cx) {
            return false;
        }

        let Some(initialization) = Self::source_panel_initialization(&source_workspace, cx) else {
            return false;
        };

        let mut initialized = false;
        if self.selected_agent != initialization.agent {
            self.selected_agent = initialization.agent.clone();
            self.serialize(cx);
            initialized = true;
        }

        if let Some(initial_content) = initialization.initial_content {
            let thread = self.create_agent_thread_with_server(
                initialization.agent,
                None,
                None,
                None,
                None,
                Some(initial_content),
                None,
                AgentThreadSource::AgentPanel,
                window,
                cx,
            );
            self.draft_thread = Some(thread.conversation_view.clone());
            self.observe_draft_editor(&thread.conversation_view, cx);
            self.set_base_view(thread.into(), false, window, cx);
            true
        } else {
            if initialized
                && matches!(
                    &self.base_view,
                    BaseView::AgentThread { conversation_view }
                        if self.draft_thread.as_ref().is_some_and(|draft| {
                            draft.entity_id() == conversation_view.entity_id()
                        })
                )
            {
                self.activate_draft(false, AgentThreadSource::AgentPanel, window, cx);
            } else if initialized {
                cx.notify();
            }
            initialized
        }
    }
}
