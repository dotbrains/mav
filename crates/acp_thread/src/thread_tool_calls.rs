use super::*;

impl AcpThread {
    pub fn update_tool_call(
        &mut self,
        update: impl Into<ToolCallUpdate>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let update = update.into();
        let languages = self.project.read(cx).languages().clone();
        let path_style = self.project.read(cx).path_style(cx);

        let ix = match self.index_for_tool_call(update.id()) {
            Some(ix) => ix,
            None => {
                // Tool call not found - create a failed tool call entry
                let failed_tool_call = ToolCall {
                    id: update.id().clone(),
                    label: cx.new(|cx| Markdown::new("Tool call not found".into(), None, None, cx)),
                    kind: acp::ToolKind::Fetch,
                    content: vec![ToolCallContent::ContentBlock(ContentBlock::new(
                        "Tool call not found".into(),
                        &languages,
                        path_style,
                        cx,
                    ))],
                    status: ToolCallStatus::Failed,
                    locations: Vec::new(),
                    resolved_locations: Vec::new(),
                    raw_input: None,
                    raw_input_markdown: None,
                    raw_output: None,
                    tool_name: None,
                    subagent_session_info: None,
                    sandbox_authorization_details: None,
                    sandbox_fallback_authorization_details: None,
                    sandbox_not_applied: None,
                };
                self.push_entry(AgentThreadEntry::ToolCall(failed_tool_call), cx);
                return Ok(());
            }
        };
        let AgentThreadEntry::ToolCall(call) = &mut self.entries[ix] else {
            unreachable!()
        };

        match update {
            ToolCallUpdate::UpdateFields(update) => {
                let location_updated = update.fields.locations.is_some();
                call.update_fields(
                    update.fields,
                    update.meta,
                    languages,
                    path_style,
                    &self.terminals,
                    cx,
                )?;
                if location_updated {
                    self.resolve_locations(update.tool_call_id, cx);
                }
            }
            ToolCallUpdate::UpdateDiff(update) => {
                call.content.clear();
                call.content.push(ToolCallContent::Diff(update.diff));
            }
            ToolCallUpdate::UpdateTerminal(update) => {
                call.content.clear();
                call.content
                    .push(ToolCallContent::Terminal(update.terminal));
            }
        }

        cx.emit(AcpThreadEvent::EntryUpdated(ix));

        Ok(())
    }

    /// Updates a tool call if id matches an existing entry, otherwise inserts a new one.
    pub fn upsert_tool_call(
        &mut self,
        tool_call: acp::ToolCall,
        cx: &mut Context<Self>,
    ) -> Result<(), acp::Error> {
        let status = tool_call.status.into();
        self.upsert_tool_call_inner(tool_call.into(), status, cx)
    }

    /// Fails if id does not match an existing entry.
    pub fn upsert_tool_call_inner(
        &mut self,
        update: acp::ToolCallUpdate,
        status: ToolCallStatus,
        cx: &mut Context<Self>,
    ) -> Result<(), acp::Error> {
        let language_registry = self.project.read(cx).languages().clone();
        let path_style = self.project.read(cx).path_style(cx);
        let id = update.tool_call_id.clone();

        let agent_telemetry_id = self.connection().telemetry_id();
        let session = self.session_id();
        let parent_session_id = self.parent_session_id();
        if let ToolCallStatus::Completed | ToolCallStatus::Failed = status {
            let status = if matches!(status, ToolCallStatus::Completed) {
                "completed"
            } else {
                "failed"
            };
            telemetry::event!(
                "Agent Tool Call Completed",
                agent_telemetry_id,
                session,
                parent_session_id,
                status
            );
        }

        if let Some(ix) = self.index_for_tool_call(&id) {
            let AgentThreadEntry::ToolCall(call) = &mut self.entries[ix] else {
                unreachable!()
            };

            call.update_fields(
                update.fields,
                update.meta,
                language_registry,
                path_style,
                &self.terminals,
                cx,
            )?;
            call.update_status(status);

            cx.emit(AcpThreadEvent::EntryUpdated(ix));
        } else {
            let call = ToolCall::from_acp(
                update.try_into()?,
                status,
                language_registry,
                self.project.read(cx).path_style(cx),
                &self.terminals,
                cx,
            )?;
            self.push_entry(AgentThreadEntry::ToolCall(call), cx);
        };

        self.resolve_locations(id, cx);
        Ok(())
    }

    fn index_for_tool_call(&self, id: &acp::ToolCallId) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, entry)| {
                if let AgentThreadEntry::ToolCall(tool_call) = entry
                    && &tool_call.id == id
                {
                    Some(index)
                } else {
                    None
                }
            })
    }

    fn tool_call_mut(&mut self, id: &acp::ToolCallId) -> Option<(usize, &mut ToolCall)> {
        // The tool call we are looking for is typically the last one, or very close to the end.
        // At the moment, it doesn't seem like a hashmap would be a good fit for this use case.
        self.entries
            .iter_mut()
            .enumerate()
            .rev()
            .find_map(|(index, tool_call)| {
                if let AgentThreadEntry::ToolCall(tool_call) = tool_call
                    && &tool_call.id == id
                {
                    Some((index, tool_call))
                } else {
                    None
                }
            })
    }

    pub fn tool_call(&self, id: &acp::ToolCallId) -> Option<(usize, &ToolCall)> {
        self.entries
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, tool_call)| {
                if let AgentThreadEntry::ToolCall(tool_call) = tool_call
                    && &tool_call.id == id
                {
                    Some((index, tool_call))
                } else {
                    None
                }
            })
    }

    pub fn tool_call_for_subagent(&self, session_id: &acp::SessionId) -> Option<&ToolCall> {
        self.entries.iter().find_map(|entry| match entry {
            AgentThreadEntry::ToolCall(tool_call) => {
                if let Some(subagent_session_info) = &tool_call.subagent_session_info
                    && &subagent_session_info.session_id == session_id
                {
                    Some(tool_call)
                } else {
                    None
                }
            }
            _ => None,
        })
    }

    pub fn resolve_locations(&mut self, id: acp::ToolCallId, cx: &mut Context<Self>) {
        let project = self.project.clone();
        let should_update_agent_location = self.parent_session_id.is_none();
        let Some((_, tool_call)) = self.tool_call_mut(&id) else {
            return;
        };
        let task = tool_call.resolve_locations(project, cx);
        cx.spawn(async move |this, cx| {
            let resolved_locations = task.await;

            this.update(cx, |this, cx| {
                let project = this.project.clone();

                for location in resolved_locations.iter().flatten() {
                    this.shared_buffers
                        .insert(location.buffer.clone(), location.buffer.read(cx).snapshot());
                }
                let Some((ix, tool_call)) = this.tool_call_mut(&id) else {
                    return;
                };

                if let Some(Some(location)) = resolved_locations.last() {
                    project.update(cx, |project, cx| {
                        let should_ignore = if let Some(agent_location) = project
                            .agent_location()
                            .filter(|agent_location| agent_location.buffer == location.buffer)
                        {
                            let snapshot = location.buffer.read(cx).snapshot();
                            let old_position = agent_location.position.to_point(&snapshot);
                            let new_position = location.position.to_point(&snapshot);

                            // ignore this so that when we get updates from the edit tool
                            // the position doesn't reset to the startof line
                            old_position.row == new_position.row
                                && old_position.column > new_position.column
                        } else {
                            false
                        };
                        if !should_ignore && should_update_agent_location {
                            project.set_agent_location(Some(location.into()), cx);
                        }
                    });
                }

                let resolved_locations = resolved_locations
                    .iter()
                    .map(|l| l.as_ref().map(|l| AgentLocation::from(l)))
                    .collect::<Vec<_>>();

                if tool_call.resolved_locations != resolved_locations {
                    tool_call.resolved_locations = resolved_locations;
                    cx.emit(AcpThreadEvent::EntryUpdated(ix));
                }
            })
        })
        .detach();
    }

    pub fn request_tool_call_authorization(
        &mut self,
        tool_call: acp::ToolCallUpdate,
        options: PermissionOptions,
        kind: AuthorizationKind,
        cx: &mut Context<Self>,
    ) -> Result<Task<RequestPermissionOutcome>> {
        let (tx, rx) = oneshot::channel();

        let current_status = self
            .tool_call(&tool_call.tool_call_id)
            .and_then(|(_, tool_call)| tool_call.status.as_acp_status())
            .or(tool_call.fields.status)
            .unwrap_or(acp::ToolCallStatus::Pending);
        let status = ToolCallStatus::WaitingForConfirmation {
            current_status,
            options,
            respond_tx: tx,
            kind,
        };

        let tool_call_id = tool_call.tool_call_id.clone();
        self.upsert_tool_call_inner(tool_call, status, cx)?;
        cx.emit(AcpThreadEvent::ToolAuthorizationRequested(
            tool_call_id.clone(),
        ));

        Ok(cx.spawn(async move |this, cx| {
            let outcome = match rx.await {
                Ok(outcome) => RequestPermissionOutcome::Selected(outcome),
                Err(oneshot::Canceled) => RequestPermissionOutcome::Cancelled,
            };
            this.update(cx, |_this, cx| {
                cx.emit(AcpThreadEvent::ToolAuthorizationReceived(tool_call_id))
            })
            .ok();
            outcome
        }))
    }

    pub fn cancel_tool_call_authorization(&mut self, id: &acp::ToolCallId, cx: &mut Context<Self>) {
        let Some((ix, call)) = self.tool_call_mut(id) else {
            return;
        };
        if !matches!(call.status, ToolCallStatus::WaitingForConfirmation { .. }) {
            return;
        }

        call.status = ToolCallStatus::Canceled;
        cx.emit(AcpThreadEvent::EntryUpdated(ix));
        cx.emit(AcpThreadEvent::ToolAuthorizationReceived(id.clone()));
    }

    pub fn authorize_tool_call(
        &mut self,
        id: acp::ToolCallId,
        outcome: SelectedPermissionOutcome,
        cx: &mut Context<Self>,
    ) {
        let Some((ix, call)) = self.tool_call_mut(&id) else {
            return;
        };

        let new_status =
            match &call.status {
                ToolCallStatus::WaitingForConfirmation {
                    kind: AuthorizationKind::ActionChoice,
                    ..
                } => ToolCallStatus::InProgress,
                ToolCallStatus::WaitingForConfirmation { current_status, .. } => {
                    match outcome.option_kind {
                        acp::PermissionOptionKind::RejectOnce
                        | acp::PermissionOptionKind::RejectAlways => ToolCallStatus::Rejected,
                        acp::PermissionOptionKind::AllowOnce
                        | acp::PermissionOptionKind::AllowAlways => {
                            ToolCallStatus::status_after_permission_grant(*current_status)
                        }
                        _ => ToolCallStatus::status_after_permission_grant(*current_status),
                    }
                }
                _ => match outcome.option_kind {
                    acp::PermissionOptionKind::RejectOnce
                    | acp::PermissionOptionKind::RejectAlways => ToolCallStatus::Rejected,
                    acp::PermissionOptionKind::AllowOnce
                    | acp::PermissionOptionKind::AllowAlways => ToolCallStatus::InProgress,
                    _ => ToolCallStatus::InProgress,
                },
            };

        let curr_status = mem::replace(&mut call.status, new_status);

        if let ToolCallStatus::WaitingForConfirmation { respond_tx, .. } = curr_status {
            respond_tx.send(outcome).ok();
        }

        cx.emit(AcpThreadEvent::EntryUpdated(ix));
    }
}
