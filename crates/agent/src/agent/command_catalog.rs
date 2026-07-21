use super::*;

impl NativeAgent {
    pub(super) fn handle_models_updated_event(
        &mut self,
        _registry: Entity<LanguageModelRegistry>,
        event: &language_model::Event,
        cx: &mut Context<Self>,
    ) {
        self.models.refresh_list(cx);

        let registry = LanguageModelRegistry::read_global(cx);
        let default_model = registry.default_model().map(|m| m.model);
        let summarization_model = registry.thread_summary_model(cx).map(|m| m.model);

        for session in self.sessions.values_mut() {
            session.thread.update(cx, |thread, cx| {
                thread.ensure_model(default_model.as_ref(), cx);

                if let Some(model) = summarization_model.clone() {
                    if thread.summarization_model().is_none()
                        || matches!(event, language_model::Event::ThreadSummaryModelChanged)
                    {
                        thread.set_summarization_model(Some(model), cx);
                    }
                }
            });
        }
    }

    pub(super) fn handle_context_server_store_updated(
        &mut self,
        store: Entity<project::context_server_store::ContextServerStore>,
        _event: &project::context_server_store::ServerStatusChangedEvent,
        cx: &mut Context<Self>,
    ) {
        let project_id = self.projects.iter().find_map(|(id, state)| {
            if *state.context_server_registry.read(cx).server_store() == store {
                Some(*id)
            } else {
                None
            }
        });
        if let Some(project_id) = project_id {
            self.update_available_commands_for_project(project_id, cx);
        }
    }

    pub(super) fn handle_context_server_registry_event(
        &mut self,
        registry: Entity<ContextServerRegistry>,
        event: &ContextServerRegistryEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ContextServerRegistryEvent::ToolsChanged => {}
            ContextServerRegistryEvent::PromptsChanged => {
                let project_id = self.projects.iter().find_map(|(id, state)| {
                    if state.context_server_registry == registry {
                        Some(*id)
                    } else {
                        None
                    }
                });
                if let Some(project_id) = project_id {
                    self.update_available_commands_for_project(project_id, cx);
                }
            }
        }
    }

    pub(super) fn publish_skill_index(&self, cx: &mut Context<Self>) {
        let mut global_skills = Vec::new();
        let mut project_groups: Vec<ProjectSkillGroup> = Vec::new();
        let mut seen_global = false;

        for state in self.projects.values() {
            for skill in state.skills.iter() {
                match &skill.source {
                    SkillSource::BuiltIn => {}
                    SkillSource::Global => {
                        if !seen_global {
                            global_skills.push(skill.clone());
                        }
                    }
                    SkillSource::ProjectLocal {
                        worktree_id,
                        worktree_root_name,
                    } => {
                        if let Some(group) = project_groups
                            .iter_mut()
                            .find(|g| g.worktree_id == *worktree_id)
                        {
                            group.skills.push(skill.clone());
                        } else {
                            project_groups.push(ProjectSkillGroup {
                                worktree_id: *worktree_id,
                                worktree_root_name: SharedString::from(worktree_root_name.clone()),
                                skills: vec![skill.clone()],
                            });
                        }
                    }
                }
            }
            if !global_skills.is_empty() {
                seen_global = true;
            }
        }

        cx.set_global(SkillIndex {
            global_skills,
            project_skills: project_groups,
        });
    }

    pub(super) fn update_available_commands_for_project(
        &self,
        project_id: EntityId,
        cx: &mut Context<Self>,
    ) {
        let available_commands =
            Self::build_available_commands_for_project(self.projects.get(&project_id), cx);
        for session in self.sessions.values() {
            if session.project_id != project_id {
                continue;
            }
            session.acp_thread.update(cx, |thread, cx| {
                thread
                    .handle_session_update(
                        acp::SessionUpdate::AvailableCommandsUpdate(
                            acp::AvailableCommandsUpdate::new(available_commands.clone()),
                        ),
                        cx,
                    )
                    .log_err();
            });
        }
    }

    pub(super) fn build_available_commands_for_project(
        project_state: Option<&ProjectState>,
        cx: &App,
    ) -> Vec<acp::AvailableCommand> {
        let Some(state) = project_state else {
            return Vec::new();
        };
        let compact_command = acp::AvailableCommand::new(
            COMPACT_COMMAND_NAME,
            "Summarize the conversation so far to free up context",
        )
        .meta(acp_thread::meta_with_command_category(
            acp_thread::CommandCategory::Native,
        ));

        let registry = state.context_server_registry.read(cx);

        // Reserve the built-in command name so a same-named MCP prompt is
        // force-prefixed (`/<server>.compact`) and stays reachable: an
        // unqualified `/compact` always routes to the native command.
        let ambiguous_prompt_names = ambiguous_mcp_prompt_names(
            [COMPACT_COMMAND_NAME],
            registry.prompts().map(|p| p.prompt.name.as_str()),
        );

        let mcp_commands = registry.prompts().flat_map(|context_server_prompt| {
            let prompt = &context_server_prompt.prompt;

            let should_prefix = ambiguous_prompt_names.contains(prompt.name.as_str());

            let name = if should_prefix {
                format!("{}.{}", context_server_prompt.server_id, prompt.name)
            } else {
                prompt.name.clone()
            };

            let mut command =
                acp::AvailableCommand::new(name, prompt.description.clone().unwrap_or_default())
                    .meta(acp_thread::meta_with_command_category(
                        acp_thread::CommandCategory::Mcp,
                    ));

            match prompt.arguments.as_deref() {
                Some([arg]) => {
                    let hint = format!("<{}>", arg.name);

                    command = command.input(acp::AvailableCommandInput::Unstructured(
                        acp::UnstructuredCommandInput::new(hint),
                    ));
                }
                Some([]) | None => {}
                Some(_) => {
                    // skip >1 argument commands since we don't support them yet
                    return None;
                }
            }

            Some(command)
        });

        std::iter::once(compact_command)
            .chain(mcp_commands)
            .collect()
    }
}
