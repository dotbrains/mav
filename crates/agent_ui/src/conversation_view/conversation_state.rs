use super::*;

#[derive(Default)]
pub(crate) struct Conversation {
    pub(super) threads: HashMap<acp::SessionId, Entity<AcpThread>>,
    permission_requests: IndexMap<acp::SessionId, Vec<acp::ToolCallId>>,
    subscriptions: Vec<Subscription>,
    pub(super) updated_at: Option<Instant>,
}

impl Conversation {
    pub fn register_thread(&mut self, thread: Entity<AcpThread>, cx: &mut Context<Self>) {
        let session_id = thread.read(cx).session_id().clone();
        let subscription = cx.subscribe(&thread, {
            let session_id = session_id.clone();
            move |this, _thread, event, _cx| {
                this.updated_at = Some(Instant::now());
                match event {
                    AcpThreadEvent::ToolAuthorizationRequested(id) => {
                        this.permission_requests
                            .entry(session_id.clone())
                            .or_default()
                            .push(id.clone());
                    }
                    AcpThreadEvent::ToolAuthorizationReceived(id) => {
                        if let Some(tool_calls) = this.permission_requests.get_mut(&session_id) {
                            tool_calls.retain(|tool_call_id| tool_call_id != id);
                            if tool_calls.is_empty() {
                                this.permission_requests.shift_remove(&session_id);
                            }
                        }
                    }
                    AcpThreadEvent::NewEntry
                    | AcpThreadEvent::StatusChanged
                    | AcpThreadEvent::TitleUpdated
                    | AcpThreadEvent::TokenUsageUpdated
                    | AcpThreadEvent::EntryUpdated(_)
                    | AcpThreadEvent::EntriesRemoved(_)
                    | AcpThreadEvent::Retry(_)
                    | AcpThreadEvent::SubagentSpawned(_)
                    | AcpThreadEvent::Stopped(_)
                    | AcpThreadEvent::Error
                    | AcpThreadEvent::LoadError(_)
                    | AcpThreadEvent::PromptCapabilitiesUpdated
                    | AcpThreadEvent::Refusal
                    | AcpThreadEvent::AvailableCommandsUpdated(_)
                    | AcpThreadEvent::ModeUpdated(_)
                    | AcpThreadEvent::ConfigOptionsUpdated(_)
                    | AcpThreadEvent::WorkingDirectoriesUpdated
                    | AcpThreadEvent::PromptUpdated => {}
                }
            }
        });
        self.subscriptions.push(subscription);
        self.threads.insert(session_id, thread);
    }

    pub fn permission_options_for_tool_call<'a>(
        &'a self,
        session_id: &acp::SessionId,
        tool_call_id: acp::ToolCallId,
        cx: &'a App,
    ) -> Option<&'a PermissionOptions> {
        let thread = self.threads.get(session_id)?;
        let (_, tool_call) = thread.read(cx).tool_call(&tool_call_id)?;
        let ToolCallStatus::WaitingForConfirmation { options, .. } = &tool_call.status else {
            return None;
        };
        Some(options)
    }

    pub fn pending_tool_call<'a>(
        &'a self,
        session_id: &acp::SessionId,
        cx: &'a App,
    ) -> Option<(acp::SessionId, acp::ToolCallId, &'a PermissionOptions)> {
        let thread = self.threads.get(session_id)?;
        let is_subagent = thread.read(cx).parent_session_id().is_some();
        let (result_session_id, thread, tool_id) = if is_subagent {
            let id = self.permission_requests.get(session_id)?.iter().next()?;
            (session_id.clone(), thread, id)
        } else {
            let (id, tool_calls) = self.permission_requests.first()?;
            let thread = self.threads.get(id)?;
            let tool_id = tool_calls.iter().next()?;
            (id.clone(), thread, tool_id)
        };
        let (_, tool_call) = thread.read(cx).tool_call(tool_id)?;

        let ToolCallStatus::WaitingForConfirmation { options, .. } = &tool_call.status else {
            return None;
        };
        Some((result_session_id, tool_id.clone(), options))
    }

    pub fn subagents_awaiting_permission(&self, cx: &App) -> Vec<(acp::SessionId, usize)> {
        self.permission_requests
            .iter()
            .filter_map(|(session_id, tool_call_ids)| {
                let thread = self.threads.get(session_id)?;
                if thread.read(cx).parent_session_id().is_some() && !tool_call_ids.is_empty() {
                    Some((session_id.clone(), tool_call_ids.len()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns the first pending tool call request for exactly `session_id`.
    /// Unlike `pending_tool_call`, this does not use the global FIFO pending
    /// request for non-subagent sessions.
    pub fn pending_tool_call_for_session(
        &self,
        session_id: &acp::SessionId,
        cx: &App,
    ) -> Option<acp::ToolCallId> {
        let thread = self.threads.get(session_id)?;
        let tool_call_id = self.permission_requests.get(session_id)?.iter().next()?;
        let (_, tool_call) = thread.read(cx).tool_call(tool_call_id)?;
        if !matches!(
            tool_call.status,
            ToolCallStatus::WaitingForConfirmation { .. }
        ) {
            return None;
        }
        Some(tool_call_id.clone())
    }

    pub fn pending_tool_call_count_for_session(&self, session_id: &acp::SessionId) -> usize {
        self.permission_requests
            .get(session_id)
            .map(|tool_call_ids| tool_call_ids.len())
            .unwrap_or(0)
    }

    pub fn authorize_pending_tool_call(
        &mut self,
        session_id: &acp::SessionId,
        kind: acp::PermissionOptionKind,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let (authorize_session_id, tool_call_id, options) =
            self.pending_tool_call(session_id, cx)?;
        let option = permission_option_for_action(options, kind)?;
        self.authorize_tool_call(
            authorize_session_id,
            tool_call_id,
            SelectedPermissionOutcome::new(option.option_id.clone(), option.kind),
            cx,
        );
        Some(())
    }

    pub fn authorize_with_granularity(
        &mut self,
        session_id: acp::SessionId,
        tool_call_id: acp::ToolCallId,
        selection: Option<&thread_view::PermissionSelection>,
        is_allow: bool,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let options =
            self.permission_options_for_tool_call(&session_id, tool_call_id.clone(), cx)?;
        let outcome = resolve_outcome_from_selection(options, selection, is_allow)?;
        self.authorize_tool_call(session_id, tool_call_id, outcome, cx);
        Some(())
    }

    pub fn authorize_tool_call(
        &mut self,
        session_id: acp::SessionId,
        tool_call_id: acp::ToolCallId,
        outcome: SelectedPermissionOutcome,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.threads.get(&session_id) else {
            return;
        };
        let agent_telemetry_id = thread.read(cx).connection().telemetry_id();
        let session_id = thread.read(cx).session_id().clone();

        telemetry::event!(
            "Agent Tool Call Authorized",
            agent = agent_telemetry_id,
            session = session_id,
            option = outcome.option_kind
        );

        thread.update(cx, |thread, cx| {
            thread.authorize_tool_call(tool_call_id, outcome, cx);
        });
        cx.notify();
    }

    pub(super) fn set_work_dirs(&mut self, work_dirs: PathList, cx: &mut Context<Self>) {
        for thread in self.threads.values() {
            thread.update(cx, |thread, cx| {
                thread.set_work_dirs(work_dirs.clone(), cx);
            });
        }
    }
}

pub(super) fn permission_option_for_action(
    options: &PermissionOptions,
    kind: acp::PermissionOptionKind,
) -> Option<&acp::PermissionOption> {
    if kind == acp::PermissionOptionKind::AllowAlways
        && let PermissionOptions::Flat(options) = options
        && let Some(option) = options.iter().find(|option| {
            option.option_id.0.as_ref() == acp_thread::SandboxPermission::AllowAlways.as_id()
        })
    {
        return Some(option);
    }

    options.first_option_of_kind(kind)
}

pub(super) fn resolve_outcome_from_selection(
    options: &PermissionOptions,
    selection: Option<&thread_view::PermissionSelection>,
    is_allow: bool,
) -> Option<SelectedPermissionOutcome> {
    let choices = match options {
        PermissionOptions::Dropdown(choices) => choices.as_slice(),
        PermissionOptions::DropdownWithPatterns { choices, .. } => choices.as_slice(),
        PermissionOptions::Flat(_) => {
            let kind = if is_allow {
                acp::PermissionOptionKind::AllowOnce
            } else {
                acp::PermissionOptionKind::RejectOnce
            };
            let option = options.first_option_of_kind(kind)?;
            return Some(SelectedPermissionOutcome::new(
                option.option_id.clone(),
                option.kind,
            ));
        }
    };

    // When in per-command pattern mode, use the checked patterns.
    if let Some(thread_view::PermissionSelection::SelectedPatterns(checked)) = selection {
        if let Some(outcome) = options.build_outcome_for_checked_patterns(checked, is_allow) {
            return Some(outcome);
        }
    }

    // Use the selected granularity choice ("Always for terminal" or "Only this time").
    let selected_index = selection
        .and_then(|s| s.choice_index())
        .unwrap_or_else(|| choices.len().saturating_sub(1));
    let selected_choice = choices.get(selected_index).or(choices.last())?;
    Some(selected_choice.build_outcome(is_allow))
}

pub(super) fn affects_thread_metadata(event: &AcpThreadEvent) -> bool {
    match event {
        AcpThreadEvent::NewEntry
        | AcpThreadEvent::TitleUpdated
        | AcpThreadEvent::ToolAuthorizationRequested(_)
        | AcpThreadEvent::ToolAuthorizationReceived(_)
        | AcpThreadEvent::Stopped(_)
        | AcpThreadEvent::Error
        | AcpThreadEvent::LoadError(_)
        | AcpThreadEvent::Refusal
        | AcpThreadEvent::WorkingDirectoriesUpdated => true,
        // --
        AcpThreadEvent::EntryUpdated(_)
        | AcpThreadEvent::StatusChanged
        | AcpThreadEvent::EntriesRemoved(_)
        | AcpThreadEvent::Retry(_)
        | AcpThreadEvent::TokenUsageUpdated
        | AcpThreadEvent::PromptCapabilitiesUpdated
        | AcpThreadEvent::AvailableCommandsUpdated(_)
        | AcpThreadEvent::ModeUpdated(_)
        | AcpThreadEvent::ConfigOptionsUpdated(_)
        | AcpThreadEvent::SubagentSpawned(_)
        | AcpThreadEvent::PromptUpdated => false,
    }
}
