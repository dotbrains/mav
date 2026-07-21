use super::*;

impl ConversationView {
    pub(super) fn save_provisional_draft_metadata(
        thread_id: ThreadId,
        agent: &Agent,
        project: &Entity<Project>,
        cx: &mut App,
    ) {
        if project.read(cx).is_via_collab() {
            return;
        }
        let Some(store) = ThreadMetadataStore::try_global(cx) else {
            return;
        };

        let project = project.read(cx);
        let worktree_paths = project.worktree_paths(cx);
        let remote_connection = project.remote_connection_options(cx);
        let updated_at = chrono::Utc::now();
        let archived = worktree_paths.is_empty();

        store.update(cx, |store, cx| {
            store.save(
                ThreadMetadata {
                    thread_id,
                    session_id: None,
                    agent_id: agent.id(),
                    title: None,
                    title_override: None,
                    updated_at,
                    created_at: Some(updated_at),
                    interacted_at: Some(updated_at),
                    worktree_paths,
                    remote_connection,
                    archived,
                },
                cx,
            );
        });
    }

    pub(super) fn new_loading_draft(
        agent: &Rc<dyn AgentServer>,
        connection_key: &Agent,
        thread_id: ThreadId,
        workspace: WeakEntity<Workspace>,
        project: WeakEntity<Project>,
        thread_store: Option<Entity<ThreadStore>>,
        initial_content: Option<&AgentInitialContent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> LoadingDraft {
        let session_capabilities = Arc::new(RwLock::new(SessionCapabilities::default()));
        let agent_display_name = project
            .upgrade()
            .and_then(|project| {
                let agent_server_store = project.read(cx).agent_server_store().clone();
                agent_server_store
                    .read(cx)
                    .agent_display_name(&agent.agent_id())
            })
            .unwrap_or_else(|| connection_key.label());
        let placeholder = placeholder_text(agent_display_name.as_ref(), false);

        let message_editor = cx.new(|cx| {
            let mut editor = MessageEditor::new(
                workspace.clone(),
                project,
                thread_store,
                session_capabilities.clone(),
                agent.agent_id(),
                &placeholder,
                editor::EditorMode::AutoHeight {
                    min_lines: AgentSettings::get_global(cx).message_editor_min_lines,
                    max_lines: Some(AgentSettings::get_global(cx).set_message_editor_max_lines()),
                },
                window,
                cx,
            );
            let mut seeded_from_initial_content = false;
            if let Some(AgentInitialContent::ContentBlock {
                blocks,
                auto_submit: false,
            }) = initial_content
            {
                editor.set_message(blocks.clone(), window, cx);
                seeded_from_initial_content = true;
            }
            if !seeded_from_initial_content
                && let Some(blocks) = crate::draft_prompt_store::read(thread_id, cx)
            {
                editor.set_message(blocks, window, cx);
            }
            editor
        });

        let mut subscriptions = Vec::new();
        subscriptions.push(
            cx.subscribe(&message_editor, move |_this, editor, event, cx| {
                if !matches!(event, MessageEditorEvent::Edited) {
                    return;
                }
                let draft_contents = editor.update(cx, |editor, cx| editor.draft_contents(cx));
                cx.spawn(async move |_, cx| {
                    let blocks = draft_contents
                        .await
                        .ok()
                        .filter(|blocks| !blocks.is_empty());
                    cx.update(|cx| {
                        if let Some(blocks) = blocks {
                            crate::draft_prompt_store::write(thread_id, &blocks, cx)
                        } else {
                            crate::draft_prompt_store::delete(thread_id, cx)
                        }
                        .detach_and_log_err(cx);
                    });
                })
                .detach();
            }),
        );
        subscriptions.push(cx.subscribe(&message_editor, |this, _editor, event, cx| {
            if matches!(
                event,
                MessageEditorEvent::Send | MessageEditorEvent::SendImmediately
            ) {
                this.loading_status = Some("Agent is still loading...".into());
                cx.notify();
            }
        }));

        LoadingDraft {
            message_editor,
            agent_selector_menu_handle: PopoverMenuHandle::default(),
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn loading_draft_editor(&self) -> Option<Entity<MessageEditor>> {
        match &self.server_state {
            ServerState::Loading {
                draft: Some(draft), ..
            } => Some(draft.message_editor.clone()),
            _ => None,
        }
    }

    fn draft_contents_task(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<Vec<acp::ContentBlock>>>> {
        if let Some(message_editor) = self.loading_draft_editor() {
            return Some(
                message_editor.update(cx, |message_editor, cx| message_editor.draft_contents(cx)),
            );
        }

        let thread_view = self.root_thread_view()?;
        if !self.is_draft(cx) {
            return None;
        }
        Some(thread_view.update(cx, |thread_view, cx| {
            thread_view
                .message_editor
                .update(cx, |message_editor, cx| message_editor.draft_contents(cx))
        }))
    }

    pub fn is_draft(&self, cx: &App) -> bool {
        match &self.server_state {
            ServerState::Loading { draft: Some(_), .. } => true,
            ServerState::Loading { .. } => false,
            ServerState::LoadError { .. } => self.root_session_id.is_none(),
            ServerState::Connected(_) => self
                .root_thread_view()
                .is_some_and(|thread_view| thread_view.read(cx).is_draft(cx)),
        }
    }

    pub(super) fn server_for_agent(
        &self,
        agent: &Agent,
        cx: &mut App,
    ) -> Option<(Rc<dyn AgentServer>, Option<Entity<ThreadStore>>)> {
        let workspace = self.workspace.upgrade()?;
        let fs = workspace.read(cx).app_state().fs.clone();
        let thread_store = ThreadStore::global(cx);
        let server = agent.server(fs, thread_store.clone());
        let thread_store_for_view = server
            .clone()
            .downcast::<NativeAgentServer>()
            .is_some()
            .then_some(thread_store);
        Some((server, thread_store_for_view))
    }

    pub fn switch_draft_agent(
        &mut self,
        connection_key: Agent,
        server: Rc<dyn AgentServer>,
        thread_store: Option<Entity<ThreadStore>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_draft(cx) || self.connection_key == connection_key {
            return;
        }

        let draft_contents_task = self.draft_contents_task(cx);
        let this = cx.weak_entity();
        cx.spawn_in(window, async move |_, cx| {
            let initial_content = if let Some(task) = draft_contents_task {
                task.await
                    .ok()
                    .filter(|blocks| !blocks.is_empty())
                    .map(|blocks| AgentInitialContent::ContentBlock {
                        blocks,
                        auto_submit: false,
                    })
            } else {
                None
            };

            this.update_in(cx, |this, window, cx| {
                if !this.is_draft(cx) {
                    return;
                }
                this.agent = server.clone();
                this.connection_key = connection_key.clone();
                this.thread_store = thread_store.clone();
                this.root_session_id = None;
                this.loading_status = None;
                Self::save_provisional_draft_metadata(
                    this.thread_id,
                    &connection_key,
                    &this.project,
                    cx,
                );
                let state = Self::initial_state(
                    server,
                    this.connection_store.clone(),
                    connection_key,
                    None,
                    this.thread_id,
                    None,
                    None,
                    this.project.clone(),
                    this.workspace.clone(),
                    thread_store,
                    initial_content,
                    AgentThreadSource::Sidebar,
                    window,
                    cx,
                );
                this.set_server_state(state, cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn switch_draft_agent_to(
        &mut self,
        agent: Agent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((server, thread_store)) = self.server_for_agent(&agent, cx) else {
            return;
        };
        self.switch_draft_agent(agent, server, thread_store, window, cx);
    }
}
