use super::*;

impl ConversationView {
    pub(super) fn handle_thread_event(
        &mut self,
        thread: &Entity<AcpThread>,
        event: &AcpThreadEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_id = thread.read(cx).session_id().clone();
        let has_thread = self
            .as_connected()
            .is_some_and(|connected| connected.threads.contains_key(&session_id));
        if !has_thread {
            return;
        };
        let is_subagent = thread.read(cx).parent_session_id().is_some();
        if !is_subagent && affects_thread_metadata(event) {
            cx.emit(RootThreadUpdated);
        }
        match event {
            AcpThreadEvent::StatusChanged => {
                if let Some(active) = self.thread_view(&session_id) {
                    active.update(cx, |active, cx| {
                        active.sync_generating_indicator(cx);
                    });
                }
            }
            AcpThreadEvent::NewEntry => {
                let len = thread.read(cx).entries().len();
                let index = len - 1;
                if let Some(active) = self.thread_view(&session_id) {
                    let entry_view_state = active.read(cx).entry_view_state.clone();
                    let list_state = active.read(cx).list_state.clone();
                    entry_view_state.update(cx, |view_state, cx| {
                        view_state.sync_entry(index, thread, window, cx);
                        list_state.splice_focusable(
                            index..index,
                            [view_state
                                .entry(index)
                                .and_then(|entry| entry.focus_handle(cx))],
                        );
                    });
                    active.update(cx, |active, cx| {
                        active.sync_editor_mode_for_empty_state(cx);
                        active.sync_generating_indicator(cx);
                    });
                }
            }
            AcpThreadEvent::EntryUpdated(index) => {
                if let Some(active) = self.thread_view(&session_id) {
                    let entry_view_state = active.read(cx).entry_view_state.clone();
                    let list_state = active.read(cx).list_state.clone();
                    entry_view_state.update(cx, |view_state, cx| {
                        view_state.sync_entry(*index, thread, window, cx);
                    });
                    list_state.remeasure_items(*index..*index + 1);
                    active.update(cx, |active, cx| {
                        active.auto_expand_streaming_thought(cx);
                        active.sync_generating_indicator(cx);
                    });
                }
            }
            AcpThreadEvent::EntriesRemoved(range) => {
                if let Some(active) = self.thread_view(&session_id) {
                    let entry_view_state = active.read(cx).entry_view_state.clone();
                    let list_state = active.read(cx).list_state.clone();
                    entry_view_state.update(cx, |view_state, _cx| view_state.remove(range.clone()));
                    list_state.splice(range.clone(), 0);
                    active.update(cx, |active, cx| {
                        active.sync_editor_mode_for_empty_state(cx);
                    });
                }
            }
            AcpThreadEvent::SubagentSpawned(subagent_session_id) => {
                self.load_subagent_session(subagent_session_id.clone(), session_id, window, cx)
            }
            AcpThreadEvent::ToolAuthorizationRequested(_) => {
                self.notify_with_sound("Waiting for tool confirmation", IconName::Info, window, cx);
            }
            AcpThreadEvent::ToolAuthorizationReceived(_) => {}
            AcpThreadEvent::Retry(retry) => {
                if let Some(active) = self.thread_view(&session_id) {
                    active.update(cx, |active, _cx| {
                        active.thread_retry_status = Some(retry.clone());
                    });
                }
            }
            AcpThreadEvent::Stopped(stop_reason) => {
                if let Some(active) = self.thread_view(&session_id) {
                    let is_generating =
                        matches!(thread.read(cx).status(), ThreadStatus::Generating);
                    active.update(cx, |active, cx| {
                        if !is_generating {
                            active.thread_retry_status.take();
                            active.clear_auto_expand_tracking(cx);
                            if active.list_state.is_following_tail() {
                                active.list_state.scroll_to_end();
                            }
                        }
                        active.sync_generating_indicator(cx);
                    });
                }
                if is_subagent {
                    if *stop_reason == acp::StopReason::EndTurn {
                        thread.update(cx, |thread, cx| {
                            thread.mark_as_subagent_output(cx);
                        });
                    }
                    return;
                }

                let sent_queued_message = if let Some(active) = self.root_thread_view() {
                    active.update(cx, |active, cx| {
                        // Don't auto-send while the user is editing the next message.
                        let is_first_editor_focused = active
                            .message_queue
                            .first()
                            .is_some_and(|entry| entry.editor.focus_handle(cx).is_focused(window));
                        if let Some(entry) = active
                            .message_queue
                            .on_generation_stopped(is_first_editor_focused)
                        {
                            active.dispatch_queued_entry(entry, window, cx);
                            true
                        } else {
                            false
                        }
                    })
                } else {
                    false
                };

                // Skip notifying when a queued message was just auto-sent: the agent
                // is not actually idle and a notification here would fire just before the
                // next turn starts.
                if !sent_queued_message {
                    let used_tools = thread.read(cx).used_tools_since_last_user_message();
                    self.notify_with_sound(
                        if used_tools {
                            "Finished running tools"
                        } else {
                            "New message"
                        },
                        IconName::MavAssistant,
                        window,
                        cx,
                    );
                }
            }
            AcpThreadEvent::Refusal => {
                let error = ThreadError::Refusal;
                if let Some(active) = self.thread_view(&session_id) {
                    active.update(cx, |active, cx| {
                        active.handle_thread_error(error, cx);
                        active.thread_retry_status.take();
                    });
                }
                if !is_subagent {
                    let model_or_agent_name = self.current_model_name(cx);
                    let notification_message =
                        format!("{} refused to respond to this request", model_or_agent_name);
                    self.notify_with_sound(&notification_message, IconName::Warning, window, cx);
                }
            }
            AcpThreadEvent::Error => {
                if let Some(active) = self.thread_view(&session_id) {
                    let is_generating =
                        matches!(thread.read(cx).status(), ThreadStatus::Generating);
                    active.update(cx, |active, cx| {
                        if !is_generating {
                            active.thread_retry_status.take();
                            if active.list_state.is_following_tail() {
                                active.list_state.scroll_to_end();
                            }
                        }
                        active.sync_generating_indicator(cx);
                    });
                }
                if !is_subagent {
                    self.notify_with_sound(
                        "Agent stopped due to an error",
                        IconName::Warning,
                        window,
                        cx,
                    );
                }
            }
            AcpThreadEvent::LoadError(error) => {
                if let Some(view) = self.root_thread_view() {
                    if view
                        .read(cx)
                        .message_editor
                        .focus_handle(cx)
                        .is_focused(window)
                    {
                        self.focus_handle.focus(window, cx)
                    }
                }
                self.set_server_state(
                    ServerState::LoadError {
                        error: error.clone(),
                    },
                    cx,
                );
            }
            AcpThreadEvent::TitleUpdated => {
                let override_title = ThreadMetadataStore::try_global(cx).and_then(|store| {
                    store
                        .read(cx)
                        .entry(self.thread_id)
                        .and_then(|m| m.title_override.clone())
                });
                let title = override_title.or_else(|| thread.read(cx).title());
                if let Some(title) = title
                    && let Some(active_thread) = self.thread_view(&session_id)
                {
                    let title_editor = active_thread.read(cx).title_editor.clone();
                    title_editor.update(cx, |editor, cx| {
                        if editor.text(cx) != title {
                            editor.set_text(title, window, cx);
                        }
                    });
                }
                cx.emit(ConversationTitleUpdated);
                cx.notify();
            }
            AcpThreadEvent::PromptCapabilitiesUpdated => {
                if let Some(active) = self.thread_view(&session_id) {
                    active.update(cx, |active, _cx| {
                        active
                            .session_capabilities
                            .write()
                            .set_prompt_capabilities(thread.read(_cx).prompt_capabilities());
                    });
                }
            }
            AcpThreadEvent::TokenUsageUpdated => {
                if let Some(active) = self.thread_view(&session_id) {
                    active.update(cx, |active, cx| {
                        active.update_turn_tokens(cx);
                    });
                }
            }
            AcpThreadEvent::AvailableCommandsUpdated(available_commands) => {
                if let Some(thread_view) = self.thread_view(&session_id) {
                    let available_skills = thread
                        .read(cx)
                        .connection()
                        .clone()
                        .downcast::<agent::NativeAgentConnection>()
                        .map(|native_connection| {
                            native_available_skills(&native_connection, &session_id, cx)
                        })
                        .unwrap_or_default();
                    let has_slash_completions =
                        !available_commands.is_empty() || !available_skills.is_empty();

                    let agent_display_name = self
                        .agent_server_store
                        .read(cx)
                        .agent_display_name(&self.agent.agent_id())
                        .unwrap_or_else(|| self.agent.agent_id().0.to_string().into());

                    let new_placeholder =
                        placeholder_text(agent_display_name.as_ref(), has_slash_completions);

                    thread_view.update(cx, |thread_view, cx| {
                        let mut session_capabilities = thread_view.session_capabilities.write();
                        session_capabilities.set_available_commands(available_commands.clone());
                        session_capabilities.set_available_skills(available_skills);
                        thread_view.message_editor.update(cx, |editor, cx| {
                            editor.set_placeholder_text(&new_placeholder, window, cx);
                        });
                    });
                }
            }
            AcpThreadEvent::ModeUpdated(_mode) => {
                // The connection keeps track of the mode
                cx.notify();
            }
            AcpThreadEvent::ConfigOptionsUpdated(_) => {
                // The watch task in ConfigOptionsView handles rebuilding selectors
                cx.notify();
            }
            AcpThreadEvent::WorkingDirectoriesUpdated => {
                cx.notify();
            }
            AcpThreadEvent::PromptUpdated => {
                if !is_subagent && self.is_draft(cx) {
                    self.schedule_draft_prompt_persist(cx);
                }
                cx.notify();
            }
        }
        cx.notify();
    }
}
