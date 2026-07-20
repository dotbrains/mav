use super::*;

impl AgentPanel {
    pub fn create_thread_with_options(
        &mut self,
        options: CreateThreadOptions,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ThreadId {
        let (agent, override_used) = if self.project.read(cx).is_via_collab() {
            (Agent::NativeAgent, false)
        } else if let Some(override_agent) = options.agent {
            (override_agent, true)
        } else {
            (self.selected_agent.clone(), false)
        };
        // If the caller explicitly overrode the agent (e.g., the `create_thread`
        // tool wants to spawn a sibling thread using a specific agent), we
        // shouldn't let that change the panel's selected_agent or the
        // last-used-agent preference. Snapshot and restore both.
        let saved_selected_agent = override_used.then(|| self.selected_agent.clone());
        let thread = self.create_agent_thread_with_server(
            agent,
            None,
            None,
            options.work_dirs,
            options.title.clone(),
            options.initial_content,
            options.model,
            source,
            window,
            cx,
        );
        if let Some(original) = saved_selected_agent
            && self.selected_agent != original
        {
            self.selected_agent = original.clone();
            self.serialize(cx);
            // Restore the last-used-agent in persistent storage as well.
            cx.background_spawn({
                let kvp = KeyValueStore::global(cx);
                async move {
                    write_global_last_used_agent(kvp, original).await;
                }
            })
            .detach();
        }
        let thread_id = thread.conversation_view.read(cx).thread_id;
        self.retained_threads
            .insert(thread_id, thread.conversation_view);
        thread_id
    }

    pub fn activate_retained_thread(
        &mut self,
        id: ThreadId,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conversation_view = if let Some(view) = self.retained_threads.remove(&id) {
            self.try_make_empty_draft_ephemeral(view.clone(), cx);
            view
        } else if let Some(draft) = &self.draft_thread {
            if draft.read(cx).thread_id == id {
                draft.clone()
            } else {
                return;
            }
        } else {
            return;
        };
        self.set_base_view(
            BaseView::AgentThread { conversation_view },
            focus,
            window,
            cx,
        );
    }

    pub fn active_thread_id(&self, cx: &App) -> Option<ThreadId> {
        match &self.base_view {
            BaseView::AgentThread { conversation_view } => {
                Some(conversation_view.read(cx).thread_id)
            }
            _ => None,
        }
    }

    /// Drops a thread — retained or the active ephemeral draft — from
    /// the panel and deletes its metadata row. Used by the sidebar when
    /// the user dismisses a parked draft.
    pub fn remove_thread(&mut self, id: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        self.remove_thread_internal(id, true, window, cx);
    }

    pub fn remove_thread_without_activating_draft(
        &mut self,
        id: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_thread_internal(id, false, window, cx);
    }

    fn remove_thread_internal(
        &mut self,
        id: ThreadId,
        activate_draft_after_remove: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retained_threads.remove(&id);
        ThreadMetadataStore::global(cx).update(cx, |store, cx| {
            store.delete(id, cx);
        });

        if self
            .draft_thread
            .as_ref()
            .is_some_and(|d| d.read(cx).thread_id == id)
        {
            self.draft_thread = None;
            self._draft_editor_observation = None;
        }

        if self.active_thread_id(cx) == Some(id) {
            self.clear_overlay_state();
            if activate_draft_after_remove {
                self.activate_draft(false, AgentThreadSource::AgentPanel, window, cx);
            } else {
                self.base_view = BaseView::Uninitialized;
                self.refresh_base_view_subscriptions(window, cx);
            }
            self.serialize(cx);
            cx.emit(AgentPanelEvent::ActiveViewChanged);
            cx.notify();
        }
    }

    pub fn ephemeral_draft_thread_id(&self, cx: &App) -> Option<ThreadId> {
        let draft = self.draft_thread.as_ref()?;
        let draft = draft.read(cx);
        draft
            .root_thread(cx)
            .is_some_and(|thread| thread.read(cx).is_draft_thread())
            .then_some(draft.thread_id)
    }

    pub fn active_terminal_id(&self) -> Option<TerminalId> {
        match &self.base_view {
            BaseView::Terminal { terminal_id } => Some(*terminal_id),
            _ => None,
        }
    }

    pub fn has_terminal(&self, terminal_id: TerminalId) -> bool {
        self.terminals.contains_key(&terminal_id)
    }

    pub fn terminals(&self, cx: &App) -> Vec<AgentPanelTerminalInfo> {
        self.terminals
            .iter()
            .map(|(id, terminal)| AgentPanelTerminalInfo {
                id: *id,
                title: terminal.title(cx),
                created_at: terminal.created_at,
                has_notification: terminal.has_notification,
                custom_title: terminal.custom_title(cx),
                working_directory: terminal.working_directory.clone(),
            })
            .collect()
    }

    pub fn editor_text(&self, id: ThreadId, cx: &App) -> Option<String> {
        self.editor_text_if_in_memory(id, cx).flatten()
    }

    pub fn editor_text_if_in_memory(&self, id: ThreadId, cx: &App) -> Option<Option<String>> {
        let cv = self
            .retained_threads
            .get(&id)
            .or_else(|| {
                self.draft_thread
                    .as_ref()
                    .filter(|draft| draft.read(cx).thread_id == id)
            })
            .or_else(|| match &self.base_view {
                BaseView::AgentThread { conversation_view }
                    if conversation_view.read(cx).thread_id == id =>
                {
                    Some(conversation_view)
                }
                _ => None,
            })?;
        let tv = cv.read(cx).root_thread_view()?;
        let text = tv.read(cx).message_editor.read(cx).text(cx);
        if text.trim().is_empty() {
            Some(None)
        } else {
            Some(Some(text))
        }
    }

    pub fn draft_prompt_blocks_if_in_memory(
        &self,
        id: ThreadId,
        cx: &App,
    ) -> Option<Vec<acp::ContentBlock>> {
        let cv = self
            .retained_threads
            .get(&id)
            .or_else(|| {
                self.draft_thread
                    .as_ref()
                    .filter(|draft| draft.read(cx).thread_id == id)
            })
            .or_else(|| match &self.base_view {
                BaseView::AgentThread { conversation_view }
                    if conversation_view.read(cx).thread_id == id =>
                {
                    Some(conversation_view)
                }
                _ => None,
            })?;
        let thread_view = cv.read(cx).root_thread_view()?;
        let thread_view = thread_view.read(cx);
        Some(
            thread_view
                .message_editor
                .read(cx)
                .draft_content_blocks_snapshot(cx),
        )
    }
}
