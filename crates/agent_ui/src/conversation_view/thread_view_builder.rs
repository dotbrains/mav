use super::*;

impl ConversationView {
    pub(super) fn new_thread_view(
        &self,
        thread: Entity<AcpThread>,
        conversation: Entity<Conversation>,
        resumed_without_history: bool,
        initial_content: Option<AgentInitialContent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ThreadView> {
        let agent_id = self.agent.agent_id();
        let connection = thread.read(cx).connection().clone();
        let session_id = thread.read(cx).session_id().clone();
        let available_skills = connection
            .clone()
            .downcast::<agent::NativeAgentConnection>()
            .map(|native_connection| native_available_skills(&native_connection, &session_id, cx))
            .unwrap_or_default();
        let session_capabilities = Arc::new(RwLock::new(SessionCapabilities::new(
            thread.read(cx).prompt_capabilities(),
            thread.read(cx).available_commands().to_vec(),
            available_skills,
        )));

        let action_log = thread.read(cx).action_log().clone();

        let entry_view_state = cx.new(|_| {
            EntryViewState::new(
                self.workspace.clone(),
                self.project.downgrade(),
                self.thread_store.clone(),
                session_capabilities.clone(),
                self.agent.agent_id(),
            )
        });

        let count = thread.read(cx).entries().len();
        let list_state = ListState::new(0, gpui::ListAlignment::Top, px(2048.0));
        list_state.set_follow_mode(gpui::FollowMode::Tail);

        entry_view_state.update(cx, |view_state, cx| {
            for ix in 0..count {
                view_state.sync_entry(ix, &thread, window, cx);
            }
            list_state.splice_focusable(
                0..0,
                (0..count).map(|ix| view_state.entry(ix)?.focus_handle(cx)),
            );
        });

        if let Some(scroll_position) = thread.read(cx).ui_scroll_position() {
            list_state.scroll_to(scroll_position);
        } else {
            list_state.scroll_to_end();
        }

        AgentDiff::set_active_thread(&self.workspace, thread.clone(), window, cx);

        let connection = thread.read(cx).connection().clone();
        let session_id = thread.read(cx).session_id().clone();
        let config_options_provider = connection.session_config_options(&session_id, cx);

        let config_options_view;
        let mode_selector;
        let model_selector;
        if let Some(config_options) = config_options_provider {
            let agent_server = self.agent.clone();
            let fs = self.project.read(cx).fs().clone();
            config_options_view =
                Some(cx.new(|cx| {
                    ConfigOptionsView::new(config_options, agent_server, fs, window, cx)
                }));
            model_selector = None;
            mode_selector = None;
        } else {
            config_options_view = None;
            model_selector = connection.model_selector(&session_id).map(|selector| {
                cx.new(|cx| {
                    ModelSelectorPopover::new(
                        selector,
                        PopoverMenuHandle::default(),
                        self.focus_handle(cx),
                        window,
                        cx,
                    )
                })
            });

            mode_selector = connection
                .session_modes(&session_id, cx)
                .map(|session_modes| {
                    let fs = self.project.read(cx).fs().clone();
                    cx.new(|_cx| ModeSelector::new(session_modes, self.agent.clone(), fs))
                });
        }

        let subscriptions = vec![
            cx.subscribe_in(&thread, window, Self::handle_thread_event),
            cx.observe(&action_log, |_, _, cx| cx.notify()),
        ];

        let subagent_sessions = thread
            .read(cx)
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                AgentThreadEntry::ToolCall(call) => call
                    .subagent_session_info
                    .as_ref()
                    .map(|i| i.session_id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        if !subagent_sessions.is_empty() {
            let parent_session_id = thread.read(cx).session_id().clone();
            cx.spawn_in(window, async move |this, cx| {
                this.update_in(cx, |this, window, cx| {
                    for subagent_id in subagent_sessions {
                        this.load_subagent_session(
                            subagent_id,
                            parent_session_id.clone(),
                            window,
                            cx,
                        );
                    }
                })
            })
            .detach();
        }

        let profile_selector: Option<Rc<agent::NativeAgentConnection>> =
            connection.clone().downcast();
        let profile_selector = profile_selector
            .and_then(|native_connection| native_connection.thread(&session_id, cx))
            .map(|native_thread| {
                cx.new(|cx| {
                    ProfileSelector::new(
                        <dyn Fs>::global(cx),
                        Arc::new(native_thread),
                        self.focus_handle(cx),
                        cx,
                    )
                })
            });

        let agent_display_name = self
            .agent_server_store
            .read(cx)
            .agent_display_name(&agent_id.clone())
            .unwrap_or_else(|| agent_id.0.clone());

        let agent_icon = self.agent.logo();
        let agent_icon_from_external_svg = self
            .agent_server_store
            .read(cx)
            .agent_icon(&self.agent.agent_id())
            .or_else(|| {
                project::AgentRegistryStore::try_global(cx).and_then(|store| {
                    store
                        .read(cx)
                        .agent(&self.agent.agent_id())
                        .and_then(|a| a.icon_path().cloned())
                })
            });

        let weak = cx.weak_entity();
        cx.new(|cx| {
            ThreadView::new(
                self.thread_id,
                self.started_as_draft,
                thread,
                conversation,
                weak,
                agent_icon,
                agent_icon_from_external_svg,
                agent_id,
                agent_display_name,
                self.workspace.clone(),
                entry_view_state,
                config_options_view,
                mode_selector,
                model_selector,
                profile_selector,
                list_state,
                session_capabilities,
                resumed_without_history,
                self.project.downgrade(),
                self.code_span_resolver.clone(),
                self.thread_store.clone(),
                initial_content,
                subscriptions,
                window,
                cx,
            )
        })
    }
}
