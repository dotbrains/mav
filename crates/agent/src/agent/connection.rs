use super::*;

/// Wrapper struct that implements the AgentConnection trait
#[derive(Clone)]
pub struct NativeAgentConnection(pub Entity<NativeAgent>);

impl NativeAgentConnection {
    pub fn thread(&self, session_id: &acp::SessionId, cx: &App) -> Option<Entity<Thread>> {
        self.0
            .read(cx)
            .sessions
            .get(session_id)
            .map(|session| session.thread.clone())
    }

    /// Forwards to [`NativeAgent::ensure_skills_scan_started`]. The
    /// agent panel calls this from its three user-interaction trigger
    /// points (input box focus, slash-autocomplete invocation, and
    /// conversation submit) so that the skills directory is observed
    /// only when the user is actually engaging with the panel.
    pub fn ensure_skills_scan_started(&self, cx: &mut App) {
        self.0
            .update(cx, |agent, cx| agent.ensure_skills_scan_started(cx));
    }

    pub fn refresh_skills_for_project(&self, project: Entity<Project>, cx: &mut App) {
        self.0.update(cx, |agent, cx| {
            let project_id = agent.get_or_create_project_state(&project, cx);
            agent.ensure_skills_scan_started(cx);
            if let Some(state) = agent.projects.get_mut(&project_id) {
                state.project_context_needs_refresh.send(()).ok();
            }
        });
    }

    pub fn available_skills(
        &self,
        session_id: &acp::SessionId,
        cx: &App,
    ) -> Vec<NativeAvailableSkill> {
        self.0
            .read(cx)
            .session_project_state(session_id)
            .map(|state| {
                state
                    .skills
                    .iter()
                    .map(NativeAvailableSkill::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn load_thread(
        &self,
        id: acp::SessionId,
        project: Entity<Project>,
        cx: &mut App,
    ) -> Task<Result<Entity<Thread>>> {
        self.0
            .update(cx, |this, cx| this.load_thread(id, project, cx))
    }

    pub(super) fn run_turn(
        &self,
        session_id: acp::SessionId,
        cx: &mut App,
        f: impl 'static
        + FnOnce(Entity<Thread>, &mut App) -> Result<mpsc::UnboundedReceiver<Result<ThreadEvent>>>,
    ) -> Task<Result<acp::PromptResponse>> {
        let Some((thread, acp_thread)) = self.0.update(cx, |agent, _cx| {
            agent
                .sessions
                .get_mut(&session_id)
                .map(|s| (s.thread.clone(), s.acp_thread.clone()))
        }) else {
            log::error!("Session not found in run_turn: {}", session_id);
            return Task::ready(Err(anyhow!("Session not found")));
        };
        log::debug!("Found session for: {}", session_id);

        let response_stream = match f(thread, cx) {
            Ok(stream) => stream,
            Err(err) => return Task::ready(Err(err)),
        };
        Self::handle_thread_events(
            response_stream,
            acp_thread.downgrade(),
            Some(self.clone()),
            cx,
        )
    }

    pub(super) fn handle_thread_events(
        mut events: mpsc::UnboundedReceiver<Result<ThreadEvent>>,
        acp_thread: WeakEntity<AcpThread>,
        connection: Option<NativeAgentConnection>,
        cx: &App,
    ) -> Task<Result<acp::PromptResponse>> {
        cx.spawn(async move |cx| {
            // Handle response stream and forward to session.acp_thread
            while let Some(result) = events.next().await {
                match result {
                    Ok(event) => {
                        log::trace!("Received completion event: {:?}", event);

                        match event {
                            ThreadEvent::UserMessage(message) => {
                                acp_thread.update(cx, |thread, cx| {
                                    for content in &*message.content {
                                        thread.push_user_content_block(
                                            Some(message.id.clone()),
                                            content.clone().into(),
                                            cx,
                                        );
                                    }
                                })?;
                            }
                            ThreadEvent::AgentText(text) => {
                                acp_thread.update(cx, |thread, cx| {
                                    thread.push_assistant_content_block(text.into(), false, cx)
                                })?;
                            }
                            ThreadEvent::AgentThinking(text) => {
                                acp_thread.update(cx, |thread, cx| {
                                    thread.push_assistant_content_block(text.into(), true, cx)
                                })?;
                            }
                            ThreadEvent::ToolCallAuthorization(ToolCallAuthorization {
                                tool_call,
                                options,
                                response,
                                context: _,
                                kind,
                            }) => {
                                let outcome_task = acp_thread.update(cx, |thread, cx| {
                                    thread.request_tool_call_authorization(
                                        tool_call, options, kind, cx,
                                    )
                                })??;
                                cx.background_spawn(async move {
                                    if let acp_thread::RequestPermissionOutcome::Selected(outcome) =
                                        outcome_task.await
                                    {
                                        response
                                            .send(outcome)
                                            .map_err(|_| {
                                                anyhow!("authorization receiver was dropped")
                                            })
                                            .log_err();
                                    }
                                })
                                .detach();
                            }
                            ThreadEvent::ToolCallAuthorizationResolved {
                                tool_call_id,
                                outcome,
                            } => {
                                acp_thread.update(cx, |thread, cx| {
                                    thread.authorize_tool_call(tool_call_id, outcome, cx);
                                })?;
                            }
                            ThreadEvent::ToolCall(tool_call) => {
                                acp_thread.update(cx, |thread, cx| {
                                    thread.upsert_tool_call(tool_call, cx)
                                })??;
                            }
                            ThreadEvent::ToolCallUpdate(update) => {
                                acp_thread.update(cx, |thread, cx| {
                                    thread.update_tool_call(update, cx)
                                })??;
                            }
                            ThreadEvent::SubagentSpawned(session_id) => {
                                acp_thread.update(cx, |thread, cx| {
                                    thread.subagent_spawned(session_id, cx);
                                })?;
                            }
                            ThreadEvent::Retry(status) => {
                                if acp_thread::refusal_fallback_model_from_meta(&status.meta)
                                    .is_some()
                                {
                                    if let Some(connection) = &connection {
                                        cx.update(|cx| {
                                            connection.0.update(cx, |agent, _| {
                                                agent.models.notify_model_selection_changed();
                                            });
                                        });
                                    }
                                }
                                acp_thread.update(cx, |thread, cx| {
                                    thread.update_retry_status(status, cx)
                                })?;
                            }
                            ThreadEvent::ContextCompaction(compaction) => {
                                acp_thread.update(cx, |thread, cx| {
                                    thread.push_context_compaction(compaction, cx);
                                })?;
                            }
                            ThreadEvent::ContextCompactionUpdate(update) => {
                                acp_thread.update(cx, |thread, cx| {
                                    thread.update_context_compaction(update, cx);
                                })?;
                            }
                            ThreadEvent::Stop(stop_reason) => {
                                log::debug!("Assistant message complete: {:?}", stop_reason);
                                return Ok(acp::PromptResponse::new(stop_reason));
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Error in model response stream: {:?}", e);
                        return Err(e);
                    }
                }
            }

            log::debug!("Response stream completed");
            anyhow::Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
        })
    }
}
