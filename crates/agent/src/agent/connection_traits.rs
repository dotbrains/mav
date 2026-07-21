use super::*;

pub static MAV_AGENT_ID: LazyLock<AgentId> = LazyLock::new(|| AgentId::new("Mav Agent"));

impl acp_thread::AgentConnection for NativeAgentConnection {
    fn agent_id(&self) -> AgentId {
        MAV_AGENT_ID.clone()
    }

    fn telemetry_id(&self) -> SharedString {
        "mav".into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        work_dirs: PathList,
        cx: &mut App,
    ) -> Task<Result<Entity<acp_thread::AcpThread>>> {
        log::debug!("Creating new thread for project at: {work_dirs:?}");
        Task::ready(Ok(self
            .0
            .update(cx, |agent, cx| agent.new_session(project, cx))))
    }

    fn supports_load_session(&self) -> bool {
        true
    }

    fn load_session(
        self: Rc<Self>,
        session_id: acp::SessionId,
        project: Entity<Project>,
        _work_dirs: PathList,
        _title: Option<SharedString>,
        cx: &mut App,
    ) -> Task<Result<Entity<acp_thread::AcpThread>>> {
        self.0
            .update(cx, |agent, cx| agent.open_thread(session_id, project, cx))
    }

    fn supports_close_session(&self) -> bool {
        true
    }

    fn close_session(
        self: Rc<Self>,
        session_id: &acp::SessionId,
        cx: &mut App,
    ) -> Task<Result<()>> {
        self.0
            .update(cx, |agent, cx| agent.close_session(session_id, cx))
    }

    fn auth_methods(&self) -> &[acp::AuthMethod] {
        &[] // No auth for in-process
    }

    fn authenticate(&self, _method: acp::AuthMethodId, _cx: &mut App) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn model_selector(&self, session_id: &acp::SessionId) -> Option<Rc<dyn AgentModelSelector>> {
        Some(Rc::new(NativeAgentModelSelector {
            session_id: session_id.clone(),
            connection: self.clone(),
        }) as Rc<dyn AgentModelSelector>)
    }

    fn client_user_message_ids(
        &self,
        _cx: &App,
    ) -> Option<Rc<dyn acp_thread::AgentSessionClientUserMessageIds>> {
        let prompt: Rc<dyn acp_thread::AgentSessionClientUserMessageIds> = Rc::new(self.clone());
        Some(prompt)
    }

    fn prompt(
        &self,
        params: acp::PromptRequest,
        cx: &mut App,
    ) -> Task<Result<acp::PromptResponse>> {
        acp_thread::AgentSessionClientUserMessageIds::prompt(
            self,
            acp_thread::AgentSessionClientUserMessageIds::new_id(self),
            params,
            cx,
        )
    }

    fn retry(
        &self,
        session_id: &acp::SessionId,
        _cx: &App,
    ) -> Option<Rc<dyn acp_thread::AgentSessionRetry>> {
        Some(Rc::new(NativeAgentSessionRetry {
            connection: self.clone(),
            session_id: session_id.clone(),
        }) as _)
    }

    fn cancel(&self, session_id: &acp::SessionId, cx: &mut App) {
        log::info!("Cancelling on session: {}", session_id);
        self.0.update(cx, |agent, cx| {
            if let Some(session) = agent.sessions.get(session_id) {
                session
                    .thread
                    .update(cx, |thread, cx| thread.cancel(cx))
                    .detach();
            }
        });
    }

    fn truncate(
        &self,
        session_id: &acp::SessionId,
        cx: &App,
    ) -> Option<Rc<dyn acp_thread::AgentSessionTruncate>> {
        self.0.read_with(cx, |agent, _cx| {
            agent.sessions.get(session_id).map(|session| {
                Rc::new(NativeAgentSessionTruncate {
                    thread: session.thread.clone(),
                    acp_thread: session.acp_thread.downgrade(),
                }) as _
            })
        })
    }

    fn set_title(
        &self,
        session_id: &acp::SessionId,
        cx: &App,
    ) -> Option<Rc<dyn acp_thread::AgentSessionSetTitle>> {
        self.0.read_with(cx, |agent, _cx| {
            agent
                .sessions
                .get(session_id)
                .filter(|s| !s.thread.read(cx).is_subagent())
                .map(|session| {
                    Rc::new(NativeAgentSessionSetTitle {
                        thread: session.thread.clone(),
                    }) as _
                })
        })
    }

    fn session_list(&self, cx: &mut App) -> Option<Rc<dyn AgentSessionList>> {
        let thread_store = self.0.read(cx).thread_store.clone();
        Some(Rc::new(NativeAgentSessionList::new(thread_store, cx)) as _)
    }

    fn telemetry(&self) -> Option<Rc<dyn acp_thread::AgentTelemetry>> {
        Some(Rc::new(self.clone()) as Rc<dyn acp_thread::AgentTelemetry>)
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

impl acp_thread::AgentSessionClientUserMessageIds for NativeAgentConnection {
    fn prompt(
        &self,
        client_user_message_id: acp_thread::ClientUserMessageId,
        params: acp::PromptRequest,
        cx: &mut App,
    ) -> Task<Result<acp::PromptResponse>> {
        let session_id = params.session_id.clone();
        log::info!("Received prompt request for session: {}", session_id);
        log::debug!("Prompt blocks count: {}", params.prompt.len());

        let Some(project_state) = self.0.read(cx).session_project_state(&session_id) else {
            log::error!("Session not found in prompt: {}", session_id);
            if self.0.read(cx).sessions.contains_key(&session_id) {
                log::error!(
                    "Session found in sessions map, but not in project state: {}",
                    session_id
                );
            }
            return Task::ready(Err(anyhow::anyhow!("Session not found")));
        };

        if let Some(parsed_command) = Command::parse(&params.prompt) {
            if parsed_command.is_unqualified(COMPACT_COMMAND_NAME) {
                return self.0.update(cx, |agent, cx| {
                    agent.send_compact_command(client_user_message_id, session_id, cx)
                });
            }

            // Skill scope qualifiers (`/:<name>` and
            // `/<worktree>:<name>`) use a colon separator that can't
            // collide with MCP's `/<server>.<name>` grammar. The popup
            // inserts a qualified form for every skill so picking the
            // global row unambiguously runs the global skill even when
            // a same-named project-local one exists.
            if let Some(scope) = parsed_command.skill_scope
                && let Some(skill) = project_state.skills.iter().find(|skill| {
                    skill.name == parsed_command.prompt_name && skill.source.matches_scope(scope)
                })
            {
                let skill = skill.clone();
                return self.0.update(cx, |agent, cx| {
                    agent.send_skill_invocation(
                        client_user_message_id,
                        session_id.clone(),
                        skill,
                        params.prompt,
                        cx,
                    )
                });
            }

            // MCP prompts and skills both register slash commands. MCP
            // prompts are checked first — if a user has both an MCP prompt
            // and a skill with the same name, the MCP prompt wins (matching
            // the order they appear in the catalog).
            let registry = project_state.context_server_registry.read(cx);

            let explicit_server_id = parsed_command
                .explicit_server_id
                .map(|server_id| ContextServerId(server_id.into()));

            if let Some(prompt) =
                registry.find_prompt(explicit_server_id.as_ref(), parsed_command.prompt_name)
            {
                let arguments = if !parsed_command.arg_value.is_empty()
                    && let Some(arg_name) = prompt
                        .prompt
                        .arguments
                        .as_ref()
                        .and_then(|args| args.first())
                        .map(|arg| arg.name.clone())
                {
                    HashMap::from_iter([(arg_name, parsed_command.arg_value.to_string())])
                } else {
                    Default::default()
                };

                let prompt_name = prompt.prompt.name.clone();
                let server_id = prompt.server_id.clone();

                return self.0.update(cx, |agent, cx| {
                    agent.send_mcp_prompt(
                        client_user_message_id,
                        session_id.clone(),
                        prompt_name,
                        server_id,
                        arguments,
                        params.prompt,
                        cx,
                    )
                });
            }

            // Unqualified skill match (`/skill-name` with no scope
            // prefix and no MCP server prefix). Slash commands work
            // for *all* skills regardless of `disable_model_invocation`
            // — that flag only hides the skill from the model's catalog.
            // The user explicitly typed the name, so they get to invoke
            // it.
            //
            // Inlined rather than calling `apply_skill_overrides` so
            // we don't clone the entire skill list on every prompt
            // (including prompts like `/help` that aren't skills at
            // all). The resolution rule matches the override-applied
            // view: among skills with the matching name, pick the one
            // with the highest source precedence, so the slash command
            // picks the same entry the model sees in its catalog.
            // Ties (e.g. two project-local skills from different
            // worktrees) resolve to the first in iteration order to
            // match `apply_skill_overrides`.
            if parsed_command.explicit_server_id.is_none()
                && parsed_command.skill_scope.is_none()
                && !project_state.skills.is_empty()
            {
                let prompt_name = parsed_command.prompt_name;
                let resolved = project_state
                    .skills
                    .iter()
                    .filter(|skill| skill.name == prompt_name)
                    .reduce(|best, candidate| {
                        if candidate.source.precedence() > best.source.precedence() {
                            candidate
                        } else {
                            best
                        }
                    });
                if let Some(skill) = resolved {
                    let skill = skill.clone();
                    return self.0.update(cx, |agent, cx| {
                        agent.send_skill_invocation(
                            client_user_message_id,
                            session_id.clone(),
                            skill,
                            params.prompt,
                            cx,
                        )
                    });
                }
            }
        };

        let path_style = project_state.project.read(cx).path_style(cx);

        self.run_turn(session_id, cx, move |thread, cx| {
            let content: Vec<UserMessageContent> = params
                .prompt
                .into_iter()
                .map(|block| UserMessageContent::from_content_block(block, path_style))
                .collect::<Vec<_>>();
            log::debug!("Converted prompt to message: {} chars", content.len());
            log::debug!("Client user message id: {:?}", client_user_message_id);
            log::debug!("Message content: {:?}", content);

            thread.update(cx, |thread, cx| {
                thread.send(client_user_message_id, content, cx)
            })
        })
    }
}

impl acp_thread::AgentTelemetry for NativeAgentConnection {
    fn thread_data(
        &self,
        session_id: &acp::SessionId,
        cx: &mut App,
    ) -> Task<Result<serde_json::Value>> {
        let Some(session) = self.0.read(cx).sessions.get(session_id) else {
            return Task::ready(Err(anyhow!("Session not found")));
        };

        let task = session.thread.read(cx).to_db(cx);
        cx.background_spawn(async move {
            serde_json::to_value(task.await).context("Failed to serialize thread")
        })
    }
}
