use super::*;

impl ThreadView {
    pub(crate) fn new(
        root_thread_id: ThreadId,
        started_as_draft: bool,
        thread: Entity<AcpThread>,
        conversation: Entity<super::Conversation>,
        server_view: WeakEntity<ConversationView>,
        agent_icon: IconName,
        agent_icon_from_external_svg: Option<SharedString>,
        agent_id: AgentId,
        agent_display_name: SharedString,
        workspace: WeakEntity<Workspace>,
        entry_view_state: Entity<EntryViewState>,
        config_options_view: Option<Entity<ConfigOptionsView>>,
        mode_selector: Option<Entity<ModeSelector>>,
        model_selector: Option<Entity<ModelSelectorPopover>>,
        profile_selector: Option<Entity<ProfileSelector>>,
        list_state: ListState,
        session_capabilities: SharedSessionCapabilities,
        resumed_without_history: bool,
        project: WeakEntity<Project>,
        code_span_resolver: AgentCodeSpanResolver,
        thread_store: Option<Entity<ThreadStore>>,
        initial_content: Option<AgentInitialContent>,
        mut subscriptions: Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let session_id = thread.read(cx).session_id().clone();
        let parent_session_id = thread.read(cx).parent_session_id().cloned();

        let has_slash_completions = session_capabilities.read().has_slash_completions();
        let placeholder = placeholder_text(agent_display_name.as_ref(), has_slash_completions);

        let mut should_auto_submit = false;
        let mut show_external_source_prompt_warning = false;

        let message_editor = cx.new(|cx| {
            let mut editor = MessageEditor::new(
                workspace.clone(),
                project.clone(),
                thread_store,
                session_capabilities.clone(),
                agent_id.clone(),
                &placeholder,
                editor::EditorMode::AutoHeight {
                    min_lines: AgentSettings::get_global(cx).message_editor_min_lines,
                    max_lines: Some(AgentSettings::get_global(cx).set_message_editor_max_lines()),
                },
                window,
                cx,
            );
            if let Some(content) = initial_content {
                match content {
                    AgentInitialContent::ThreadSummary { session_id, title } => {
                        editor.insert_thread_summary(session_id, title, window, cx);
                    }
                    AgentInitialContent::ContentBlock {
                        blocks,
                        auto_submit,
                    } => {
                        should_auto_submit = auto_submit;
                        editor.set_message(blocks, window, cx);
                    }
                    AgentInitialContent::FromExternalSource(prompt) => {
                        show_external_source_prompt_warning = true;
                        // SECURITY: Be explicit about not auto submitting prompt from external source.
                        should_auto_submit = false;
                        editor.set_message(
                            vec![acp::ContentBlock::Text(acp::TextContent::new(
                                prompt.into_string(),
                            ))],
                            window,
                            cx,
                        );
                    }
                }
            } else if let Some(draft) = thread.read(cx).draft_prompt() {
                editor.set_message(draft.to_vec(), window, cx);
            }
            editor
        });

        let show_codex_windows_warning = cfg!(windows)
            && project.upgrade().is_some_and(|p| p.read(cx).is_local())
            && agent_id.as_ref() == "Codex";

        if let Some(project) = project.upgrade() {
            subscriptions.push(cx.subscribe(&project, {
                let resolver = code_span_resolver.clone();
                move |_this: &mut Self, _project, event: &project::Event, cx| {
                    if matches!(
                        event,
                        project::Event::WorktreeAdded(_)
                            | project::Event::WorktreeRemoved(_)
                            | project::Event::WorktreeUpdatedEntries(_, _)
                    ) {
                        resolver.clear_cache();
                        cx.notify();
                    }
                }
            }));
        }

        let title_editor = {
            let metadata = ThreadMetadataStore::try_global(cx)
                .and_then(|store| store.read(cx).entry(root_thread_id).cloned());
            let initial_title = if parent_session_id.is_none() {
                metadata.as_ref().and_then(|m| m.title())
            } else {
                thread.read(cx).title()
            }
            .unwrap_or_else(|| DEFAULT_THREAD_TITLE.into());
            let editor = cx.new(|cx| {
                let mut editor = Editor::single_line(window, cx);
                editor.set_text(initial_title, window, cx);
                editor
            });
            subscriptions.push(cx.subscribe_in(&editor, window, Self::handle_title_editor_event));
            editor
        };

        subscriptions.push(cx.subscribe_in(
            &entry_view_state,
            window,
            Self::handle_entry_view_event,
        ));

        subscriptions.push(cx.subscribe_in(
            &message_editor,
            window,
            Self::handle_message_editor_event,
        ));

        // If this thread is backed by a NativeAgent, listen for skill loading
        // issues so we can surface them as banners. The agent emits a single
        // replacement-style event per project refresh, so we overwrite our
        // local list rather than appending — this also clears stale issues
        // once a user resolves them.
        if let Some(native_connection) = thread
            .read(cx)
            .connection()
            .clone()
            .downcast::<agent::NativeAgentConnection>()
        {
            let project_id = thread.read(cx).project().entity_id();
            subscriptions.push(cx.subscribe(
                &native_connection.0,
                move |this: &mut Self, _agent, event: &SkillLoadingIssuesUpdated, cx| {
                    if event.project_id != project_id {
                        return;
                    }
                    // Drop dismissals for issues that no longer appear in the emitted
                    // list — the underlying file must have been fixed or removed, so a
                    // future regression should re-show.
                    this.dismissed_skill_loading_issues
                        .retain(|dismissed| event.issues.contains(dismissed));

                    // Show only issues that haven't been dismissed.
                    this.skill_loading_issues = event
                        .issues
                        .iter()
                        .filter(|issue| !this.dismissed_skill_loading_issues.contains(issue))
                        .cloned()
                        .collect();
                    cx.notify();
                },
            ));
        }

        subscriptions.push(cx.observe(&message_editor, |this, editor, cx| {
            let is_empty = editor.read(cx).text(cx).is_empty();
            let draft_contents_task = if is_empty {
                None
            } else {
                Some(editor.update(cx, |editor, cx| editor.draft_contents(cx)))
            };
            this._draft_resolve_task = Some(cx.spawn(async move |this, cx| {
                let draft = if let Some(task) = draft_contents_task {
                    task.await.ok().filter(|b| !b.is_empty())
                } else {
                    None
                };
                this.update(cx, |this, cx| {
                    this.thread.update(cx, |thread, cx| {
                        thread.set_draft_prompt(draft, cx);
                    });
                    this.schedule_save(cx);
                })
                .ok();
            }));
        }));

        let mut this = Self {
            root_thread_id,
            started_as_draft,
            session_id,
            parent_session_id,
            focus_handle: cx.focus_handle(),
            thread,
            conversation,
            server_view,
            agent_icon,
            agent_icon_from_external_svg,
            agent_id,
            workspace,
            entry_view_state,
            title_editor,
            config_options_view,
            mode_selector,
            model_selector,
            profile_selector,
            list_state,
            session_capabilities,
            resumed_without_history,
            _subscriptions: subscriptions,
            permission_dropdown_handle: PopoverMenuHandle::default(),
            thread_retry_status: None,
            thread_error: None,
            thread_error_markdown: None,
            token_limit_callout_dismissed: false,
            last_token_limit_telemetry: None,
            thread_feedback: Default::default(),
            expanded_tool_call_raw_inputs: HashSet::default(),
            collapsed_sandbox_authorization_details: HashSet::default(),
            collapsed_sandbox_network_details: HashSet::default(),
            subagent_scroll_handles: RefCell::new(HashMap::default()),
            edits_expanded: false,
            plan_expanded: false,
            queue_expanded: true,
            editor_expanded: false,
            should_be_following: false,
            editing_message: None,
            message_queue: MessageQueue::default(),
            turn_fields: TurnFields::default(),
            discarded_partial_edits: HashSet::default(),
            is_loading_contents: false,
            new_server_version_available: None,
            permission_selections: HashMap::default(),
            _cancel_task: None,
            _save_task: None,
            _draft_resolve_task: None,
            _sandbox_status_refresh_task: None,
            hovered_edited_file_buttons: None,
            in_flight_prompt: None,
            message_editor,
            draft_agent_selector_menu_handle: PopoverMenuHandle::default(),
            add_context_menu_handle: PopoverMenuHandle::default(),
            thinking_effort_menu_handle: PopoverMenuHandle::default(),
            fast_mode_menu_handle: PopoverMenuHandle::default(),
            project,
            code_span_resolver,
            show_external_source_prompt_warning,
            show_codex_windows_warning,
            sandbox_status: None,
            sandbox_status_key: None,
            pending_sandbox_status_key: None,
            multi_root_callout_dismissed: false,
            generating_indicator_in_list: false,
            skill_loading_issues: Vec::new(),
            dismissed_skill_loading_issues: HashSet::default(),
            thread_search_bar: None,
            thread_search_visible: false,
        };

        this.sync_generating_indicator(cx);
        this.sync_editor_mode_for_empty_state(cx);
        let list_state_for_scroll = this.list_state.clone();
        let thread_view = cx.entity().downgrade();

        this.list_state
            .set_scroll_handler(move |_event, _window, cx| {
                let list_state = list_state_for_scroll.clone();
                let thread_view = thread_view.clone();
                // N.B. We must defer because the scroll handler is called while the
                // ListState's RefCell is mutably borrowed. Reading logical_scroll_top()
                // directly would panic from a double borrow.
                cx.defer(move |cx| {
                    let scroll_top = list_state.logical_scroll_top();
                    let _ = thread_view.update(cx, |this, cx| {
                        if let Some(thread) = this.as_native_thread(cx) {
                            thread.update(cx, |thread, _cx| {
                                thread.set_ui_scroll_position(Some(scroll_top));
                            });
                        }
                        this.schedule_save(cx);
                    });
                });
            });

        if should_auto_submit {
            this.send(window, cx);
        }
        this
    }

    /// Schedule a throttled save of the thread state (draft prompt, scroll position, etc.).
    /// Multiple calls within `SERIALIZATION_THROTTLE_TIME` are coalesced into a single save.
    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self._save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(SERIALIZATION_THROTTLE_TIME)
                .await;
            this.update(cx, |this, cx| {
                if let Some(thread) = this.as_native_thread(cx) {
                    thread.update(cx, |_thread, cx| cx.notify());
                }
            })
            .ok();
        }));
    }

    pub(crate) fn as_native_connection(
        &self,
        cx: &App,
    ) -> Option<Rc<agent::NativeAgentConnection>> {
        let acp_thread = self.thread.read(cx);
        acp_thread.connection().clone().downcast()
    }

    pub fn as_native_thread(&self, cx: &App) -> Option<Entity<agent::Thread>> {
        let acp_thread = self.thread.read(cx);
        self.as_native_connection(cx)?
            .thread(acp_thread.session_id(), cx)
    }

    /// Resolves the message editor's contents into content blocks. For profiles
    /// that do not enable any tools, directory mentions are expanded to inline
    /// file contents since the agent can't read files on its own.
    pub(super) fn resolve_message_contents(
        &self,
        message_editor: &Entity<MessageEditor>,
        cx: &mut App,
    ) -> Task<Result<(Vec<acp::ContentBlock>, Vec<Entity<Buffer>>)>> {
        let expand = self.as_native_thread(cx).is_some_and(|thread| {
            let thread = thread.read(cx);
            AgentSettings::get_global(cx)
                .profiles
                .get(thread.profile())
                .is_some_and(|profile| profile.tools.is_empty())
        });
        message_editor.update(cx, |message_editor, cx| message_editor.contents(expand, cx))
    }

    pub fn current_model_id(&self, cx: &App) -> Option<String> {
        let selector = self.model_selector.as_ref()?;
        let model = selector.read(cx).active_model(cx)?;
        Some(model.id.to_string())
    }

    pub fn current_mode_id(&self, cx: &App) -> Option<Arc<str>> {
        if let Some(thread) = self.as_native_thread(cx) {
            Some(thread.read(cx).profile().0.clone())
        } else {
            let mode_selector = self.mode_selector.as_ref()?;
            Some(mode_selector.read(cx).mode().0)
        }
    }

    pub(super) fn is_subagent(&self) -> bool {
        self.parent_session_id.is_some()
    }

    pub(crate) fn is_draft(&self, cx: &App) -> bool {
        self.parent_session_id.is_none()
            && self.started_as_draft
            && !self.has_user_submitted_prompt(cx)
    }

    pub(crate) fn has_user_submitted_prompt(&self, cx: &App) -> bool {
        self.in_flight_prompt.is_some()
            || self
                .thread
                .read(cx)
                .entries()
                .iter()
                .any(|entry| matches!(entry, AgentThreadEntry::UserMessage(_)))
    }

    /// Returns the currently active editor, either for a message that is being
    /// edited or the editor for a new message.
    pub(crate) fn active_editor(&self, cx: &App) -> Entity<MessageEditor> {
        if let Some(index) = self.editing_message
            && let Some(editor) = self
                .entry_view_state
                .read(cx)
                .entry(index)
                .and_then(|entry| entry.message_editor())
                .cloned()
        {
            editor
        } else {
            self.message_editor.clone()
        }
    }

    pub fn has_queued_messages(&self) -> bool {
        !self.message_queue.is_empty()
    }
}
