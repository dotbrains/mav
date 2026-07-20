use super::*;

impl AgentPanel {
    pub(crate) fn active_native_agent_thread(&self, cx: &App) -> Option<Entity<agent::Thread>> {
        match &self.base_view {
            BaseView::AgentThread { conversation_view } => {
                conversation_view.read(cx).as_native_thread(cx)
            }
            _ => None,
        }
    }

    pub(super) fn migrate_agent_server_from_extensions(
        &mut self,
        id: Arc<str>,
        cx: &mut Context<Self>,
    ) {
        self.project.update(cx, |project, cx| {
            project.agent_server_store().update(cx, |store, cx| {
                store.migrate_agent_server_from_extensions(id, project.fs().clone(), cx);
            });
        });
    }

    pub fn new_agent_thread_with_external_source_prompt(
        &mut self,
        external_source_prompt: Option<ExternalSourcePrompt>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.external_thread(
            None,
            None,
            None,
            None,
            external_source_prompt.map(AgentInitialContent::from),
            true,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
    }

    pub fn load_agent_thread(
        &mut self,
        agent: Agent,
        thread_id: ThreadId,
        work_dirs: Option<PathList>,
        title: Option<SharedString>,
        focus: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(store) = ThreadMetadataStore::try_global(cx) {
            store.update(cx, |store, cx| {
                store.unarchive(thread_id, cx);
            });
        }

        // Check if the active view already holds this thread.
        if let BaseView::AgentThread { conversation_view } = &self.base_view
            && conversation_view.read(cx).thread_id == thread_id
        {
            self.clear_overlay_state();
            cx.emit(AgentPanelEvent::ActiveViewChanged);
            return;
        }

        // Check if the thread is already in memory — either as the
        // ephemeral draft pointer or in retained_threads. Either way we
        // can just reactivate without touching storage.
        if let Some(draft) = self.draft_thread.clone()
            && draft.read(cx).thread_id == thread_id
        {
            self.set_base_view(
                BaseView::AgentThread {
                    conversation_view: draft,
                },
                focus,
                window,
                cx,
            );
            return;
        }
        if let Some(conversation_view) = self.retained_threads.remove(&thread_id) {
            self.try_make_empty_draft_ephemeral(conversation_view.clone(), cx);
            self.set_base_view(
                BaseView::AgentThread { conversation_view },
                focus,
                window,
                cx,
            );
            return;
        }

        // Not in memory. Build a fresh ConversationView. For drafts we
        // also seed the message editor with any prompt text the user had
        // typed before closing the window (persisted in the scoped kvp
        // draft-prompt store).
        let is_draft = ThreadMetadataStore::try_global(cx)
            .and_then(|store| store.read(cx).entry(thread_id).map(|m| m.is_draft()))
            .unwrap_or(false);
        let initial_content = is_draft
            .then(|| crate::draft_prompt_store::read(thread_id, cx))
            .flatten()
            .map(|blocks| AgentInitialContent::ContentBlock {
                blocks,
                auto_submit: false,
            });

        self.external_thread(
            Some(agent),
            Some(thread_id),
            work_dirs,
            title,
            initial_content,
            focus,
            source,
            window,
            cx,
        );
    }
}
