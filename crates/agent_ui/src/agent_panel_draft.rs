use super::*;

impl AgentPanel {
    pub(super) fn draft_has_content(&self, draft: &Entity<ConversationView>, cx: &App) -> bool {
        let cv = draft.read(cx);
        if let Some(thread_view) = cv.active_thread() {
            let text = thread_view.read(cx).message_editor.read(cx).text(cx);
            if !text.trim().is_empty() {
                return true;
            }
        }
        if let Some(acp_thread) = cv.root_thread(cx) {
            let thread = acp_thread.read(cx);
            if !thread.is_draft_thread() {
                return true;
            }
            if thread
                .draft_prompt()
                .is_some_and(|blocks| !blocks.is_empty())
            {
                return true;
            }
        }
        false
    }

    /// Reattaches the panel's new-draft slot to the persisted `thread_id`,
    /// seeding the editor with any prompt text from the draft-prompt kvp
    /// store.
    ///
    /// If the active view already holds this thread — because the user's
    /// last-active thread was the new-draft itself — we reuse that
    /// ConversationView instead of building a second one.
    pub(super) fn restore_new_draft(
        &mut self,
        thread_id: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_open_project(cx) {
            return;
        }

        let active_matching = match &self.base_view {
            BaseView::AgentThread { conversation_view }
                if conversation_view.read(cx).thread_id == thread_id =>
            {
                Some(conversation_view.clone())
            }
            _ => None,
        };
        if let Some(conversation_view) = active_matching {
            self.observe_draft_editor(&conversation_view, cx);
            self.draft_thread = Some(conversation_view);
            return;
        }

        let Some(metadata) = ThreadMetadataStore::try_global(cx)
            .and_then(|store| store.read(cx).entry(thread_id).cloned())
            .filter(|m| m.is_draft())
        else {
            return;
        };

        let agent = if self.project.read(cx).is_via_collab() {
            Agent::NativeAgent
        } else {
            Agent::from(metadata.agent_id.clone())
        };
        let initial_content = crate::draft_prompt_store::read(thread_id, cx).map(|blocks| {
            AgentInitialContent::ContentBlock {
                blocks,
                auto_submit: false,
            }
        });
        let thread = self.create_agent_thread_with_server(
            agent,
            None,
            Some(thread_id),
            Some(metadata.folder_paths().clone()),
            metadata.title.clone(),
            initial_content,
            None,
            AgentThreadSource::AgentPanel,
            window,
            cx,
        );
        self.observe_draft_editor(&thread.conversation_view, cx);
        self.draft_thread = Some(thread.conversation_view);
    }

    pub fn activate_draft(
        &mut self,
        focus: bool,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_open_project(cx) {
            return;
        }

        let draft = self.ensure_draft(source, window, cx);
        if let BaseView::AgentThread { conversation_view } = &self.base_view
            && conversation_view.entity_id() == draft.entity_id()
        {
            // If we're already viewing the draft as the base view but an
            // overlay (e.g. Settings) is covering it, clear the overlay
            // so the user actually sees the draft they asked for.
            // Otherwise pressing "New Thread" from the Settings panel is
            // a silent no-op because the early return below would leave
            // the overlay on top of the draft.
            if self.overlay_view.is_some() {
                self.clear_overlay(focus, window, cx);
            } else if focus {
                self.focus_handle(cx).focus(window, cx);
            }
            return;
        }
        self.set_base_view(
            BaseView::AgentThread {
                conversation_view: draft,
            },
            focus,
            window,
            cx,
        );
    }

    fn ensure_draft(
        &mut self,
        source: AgentThreadSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ConversationView> {
        let desired_agent = self.selected_agent(cx);
        if let Some(draft) = &self.draft_thread {
            let draft_entity = draft.entity_id();
            let agent_matches = *draft.read(cx).agent_key() == desired_agent;
            let has_editor_content = draft.read(cx).root_thread_view().is_some_and(|tv| {
                !tv.read(cx)
                    .message_editor
                    .read(cx)
                    .text(cx)
                    .trim()
                    .is_empty()
            });
            // Only retarget the empty draft when the user is actively
            // viewing it — that's the case where switching agents in the
            // toolbar should replace the draft with one bound to the
            // newly-selected agent. When the draft is parked in its slot
            // while the user is viewing a real thread, `selected_agent`
            // reflects that real thread's agent and must not be allowed
            // to silently rebuild the draft.
            let draft_is_active = matches!(
                &self.base_view,
                BaseView::AgentThread { conversation_view }
                    if conversation_view.entity_id() == draft_entity
            );

            if agent_matches || has_editor_content || !draft_is_active {
                return draft.clone();
            }

            // Clean up the old empty draft's metadata so it doesn't
            // linger as a ghost entry in the sidebar.
            let old_draft_id = draft.read(cx).thread_id;
            ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                store.delete(old_draft_id, cx);
            });

            self.draft_thread = None;
            self._draft_editor_observation = None;
        }

        let thread = self.create_agent_thread_with_server(
            desired_agent,
            None,
            None,
            None,
            None,
            None,
            None,
            source,
            window,
            cx,
        );

        self.draft_thread = Some(thread.conversation_view.clone());
        self.observe_draft_editor(&thread.conversation_view, cx);
        thread.conversation_view
    }

    pub(super) fn observe_draft_editor(
        &mut self,
        conversation_view: &Entity<ConversationView>,
        cx: &mut Context<Self>,
    ) {
        if let Some(acp_thread) = conversation_view.read(cx).root_thread(cx) {
            self._draft_editor_observation = Some(cx.subscribe(
                &acp_thread,
                |this, acp_thread, event: &AcpThreadEvent, cx| {
                    if !acp_thread.read(cx).is_draft_thread()
                        && this.draft_thread.as_ref().is_some_and(|draft| {
                            draft
                                .read(cx)
                                .root_thread(cx)
                                .is_some_and(|thread| thread.entity_id() == acp_thread.entity_id())
                        })
                    {
                        this.draft_thread = None;
                        this._draft_editor_observation = None;
                        this.serialize(cx);
                        return;
                    }

                    if let AcpThreadEvent::PromptUpdated = event {
                        this.serialize(cx);
                    }
                },
            ));
        } else {
            let cv = conversation_view.clone();
            self._draft_editor_observation = Some(cx.observe(&cv, |this, cv, cx| {
                if cv.read(cx).root_thread(cx).is_some() {
                    this.observe_draft_editor(&cv, cx);
                }
            }));
        }
    }

    /// Sets up an editor observation on the active view that reclaims
    /// it as ephemeral when the editor becomes empty. Only activates
    /// for non-ephemeral draft threads.
    pub(super) fn observe_active_draft_for_empty_editor(
        &mut self,
        conversation_view: &Entity<ConversationView>,
        cx: &mut Context<Self>,
    ) {
        let thread_id = conversation_view.read(cx).thread_id;
        let is_ephemeral = self
            .draft_thread
            .as_ref()
            .is_some_and(|d| d.read(cx).thread_id == thread_id);
        if is_ephemeral {
            self._active_draft_reclaim_observation = None;
            return;
        }
        let is_draft = conversation_view
            .read(cx)
            .root_thread(cx)
            .is_some_and(|t| t.read(cx).is_draft_thread());
        if !is_draft {
            self._active_draft_reclaim_observation = None;
            return;
        }
        let Some(editor) = conversation_view
            .read(cx)
            .active_thread()
            .map(|tv| tv.read(cx).message_editor.clone())
        else {
            self._active_draft_reclaim_observation = None;
            return;
        };
        let cv = conversation_view.clone();
        self._active_draft_reclaim_observation =
            Some(cx.observe(&editor, move |this, _editor, cx| {
                let editor_has_text = cv.read(cx).active_thread().is_some_and(|tv| {
                    !tv.read(cx)
                        .message_editor
                        .read(cx)
                        .text(cx)
                        .trim()
                        .is_empty()
                });
                if editor_has_text {
                    return;
                }
                if this.ephemeral_draft_thread_id(cx) == Some(thread_id) {
                    return;
                }
                if this.active_thread_id(cx) != Some(thread_id) {
                    return;
                }
                if this.try_make_empty_draft_ephemeral(cv.clone(), cx) {
                    this._active_draft_reclaim_observation = None;
                    cx.emit(AgentPanelEvent::EntryChanged);
                    cx.notify();
                }
            }));
    }

    pub(super) fn try_make_empty_draft_ephemeral(
        &mut self,
        conversation_view: Entity<ConversationView>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (thread_id, is_draft, is_empty) = {
            let conversation = conversation_view.read(cx);
            let thread_id = conversation.thread_id;
            let is_draft = conversation
                .root_thread(cx)
                .is_some_and(|thread| thread.read(cx).is_draft_thread());
            let is_empty = if let Some(thread_view) = conversation.active_thread() {
                thread_view
                    .read(cx)
                    .message_editor
                    .read(cx)
                    .text(cx)
                    .trim()
                    .is_empty()
            } else {
                !self.draft_has_content(&conversation_view, cx)
            };

            (thread_id, is_draft, is_empty)
        };

        if !is_draft || !is_empty {
            return false;
        }

        self.retained_threads.remove(&thread_id);
        self.set_ephemeral_draft(conversation_view, cx);
        true
    }

    /// Moves a conversation view into the ephemeral `draft_thread` slot,
    /// cleaning up any previous ephemeral draft and deleting the thread's
    /// metadata so it no longer appears in the sidebar.
    fn set_ephemeral_draft(
        &mut self,
        conversation_view: Entity<ConversationView>,
        cx: &mut Context<Self>,
    ) {
        if let Some(old_draft) = self.draft_thread.take() {
            let old_id = old_draft.read(cx).thread_id;
            let new_id = conversation_view.read(cx).thread_id;
            if old_id != new_id {
                ThreadMetadataStore::global(cx).update(cx, |store, cx| {
                    store.delete(old_id, cx);
                });
            }
            self._draft_editor_observation = None;
        }
        self.draft_thread = Some(conversation_view.clone());
        self.observe_draft_editor(&conversation_view, cx);
        self.serialize(cx);
    }
}
