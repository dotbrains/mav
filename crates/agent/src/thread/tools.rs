use super::*;

impl Thread {
    pub fn add_default_tools(
        &mut self,
        environment: Rc<dyn ThreadEnvironment>,
        cx: &mut Context<Self>,
    ) {
        // Only update the agent location for the root thread, not for subagents.
        let update_agent_location = self.parent_thread_id().is_none();

        let language_registry = self.project.read(cx).languages().clone();
        self.add_tool(CopyPathTool::new(self.project.clone()));
        self.add_tool(CreateDirectoryTool::new(self.project.clone()));
        self.add_tool(DeletePathTool::new(
            self.project.clone(),
            self.action_log.clone(),
        ));
        self.add_tool(EditFileTool::new(
            self.project.clone(),
            cx.weak_entity(),
            self.action_log.clone(),
            language_registry.clone(),
        ));
        self.add_tool(WriteFileTool::new(
            self.project.clone(),
            cx.weak_entity(),
            self.action_log.clone(),
            language_registry,
        ));
        self.add_tool(FetchTool::new(self.project.read(cx).client().http_client()));
        self.add_tool(FindPathTool::new(self.project.clone()));
        self.add_tool(GrepTool::new(self.project.clone()));
        self.add_tool(ListDirectoryTool::new(self.project.clone()));
        self.add_tool(MovePathTool::new(self.project.clone()));
        self.add_tool(ReadFileTool::new(
            self.project.clone(),
            self.action_log.clone(),
            update_agent_location,
        ));
        // Register terminal tool variants; `enabled_tools` exposes the one
        // matching the current sandbox state to the model as `terminal`.
        self.add_tool(TerminalTool::new(self.project.clone(), environment.clone()));
        self.add_tool(SandboxedTerminalTool::new(
            self.project.clone(),
            environment.clone(),
        ));
        self.add_tool(WebSearchTool);

        self.add_tool(DiagnosticsTool::new(self.project.clone()));

        let code_action_store: CodeActionStore = cx.new(|_cx| None);
        self.add_tool(FindReferencesTool::new(self.project.clone()));
        self.add_tool(GetCodeActionsTool::new(
            self.project.clone(),
            code_action_store.clone(),
        ));
        self.add_tool(ApplyCodeActionTool::new(
            self.project.clone(),
            code_action_store,
        ));
        self.add_tool(GoToDefinitionTool::new(self.project.clone()));
        self.add_tool(RenameTool::new(self.project.clone()));

        if self.depth() < MAX_SUBAGENT_DEPTH {
            self.add_tool(SpawnAgentTool::new(environment.clone()));
        }

        // Sibling-thread tools are exposed at every depth: a subagent should
        // still be able to kick off independent sibling work on behalf of the
        // user, even when it can no longer nest further subagents. Visibility
        // to the model is gated by `CreateThreadToolFeatureFlag` in
        // `Thread::enabled_tools`.
        self.add_tool(CreateThreadTool::new(environment.clone()));
        self.add_tool(ListAgentsAndModelsTool::new(environment));
    }

    pub fn add_tool<T: AgentTool>(&mut self, tool: T) {
        debug_assert!(
            !self.tools.contains_key(T::NAME),
            "Duplicate tool name: {}",
            T::NAME,
        );
        self.tools.insert(T::NAME.into(), tool.erase());
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn remove_tool(&mut self, name: &str) -> bool {
        self.tools.remove(name).is_some()
    }
    pub(super) fn enabled_tools(&self, cx: &App) -> BTreeMap<SharedString, Arc<dyn AnyAgentTool>> {
        let Some(model) = self.model() else {
            return BTreeMap::new();
        };
        let Some(profile) = AgentSettings::get_global(cx).profiles.get(&self.profile_id) else {
            return BTreeMap::new();
        };
        fn truncate(tool_name: &SharedString) -> SharedString {
            if tool_name.len() > MAX_TOOL_NAME_LENGTH {
                let mut truncated = tool_name.to_string();
                truncated.truncate(MAX_TOOL_NAME_LENGTH);
                truncated.into()
            } else {
                tool_name.clone()
            }
        }

        // Terminal variants are configured by users under the canonical
        // `terminal` name. Expose the one matching the current sandbox state
        // to the model under that name.
        let use_sandboxed_terminal = sandboxing_enabled_for_project(self.project.read(cx), cx);

        let mut tools = self
            .tools
            .iter()
            .filter_map(|(tool_name, tool)| {
                let terminal_variant = matches!(
                    tool_name.as_ref(),
                    TerminalTool::NAME | SandboxedTerminalTool::NAME
                );
                let profile_tool_name = if terminal_variant {
                    TerminalTool::NAME
                } else {
                    tool_name.as_ref()
                };

                if tool.supports_provider(&model.provider_id())
                    && profile.is_tool_enabled(profile_tool_name)
                {
                    match (tool_name.as_ref(), use_sandboxed_terminal) {
                        (TerminalTool::NAME, false) | (SandboxedTerminalTool::NAME, true) => {
                            Some((SharedString::from(TerminalTool::NAME), tool.clone()))
                        }
                        (TerminalTool::NAME | SandboxedTerminalTool::NAME, _) => None,
                        _ => Some((truncate(tool_name), tool.clone())),
                    }
                } else {
                    None
                }
            })
            .filter(|(tool_name, _)| crate::tools::tool_feature_flag_enabled(tool_name, cx))
            .collect::<BTreeMap<_, _>>();

        let mut context_server_tools = Vec::new();
        let mut seen_tools = tools.keys().cloned().collect::<HashSet<_>>();
        let mut duplicate_tool_names = HashSet::default();
        for (server_id, server_tools) in self.context_server_registry.read(cx).servers() {
            for (tool_name, tool) in server_tools {
                if profile.is_context_server_tool_enabled(&server_id.0, &tool_name) {
                    let tool_name = truncate(tool_name);
                    if !seen_tools.insert(tool_name.clone()) {
                        duplicate_tool_names.insert(tool_name.clone());
                    }
                    context_server_tools.push((server_id.clone(), tool_name, tool.clone()));
                }
            }
        }

        // When there are duplicate tool names, disambiguate by prefixing them
        // with the server ID (converted to snake_case for API compatibility).
        // In the rare case there isn't enough space for the disambiguated tool
        // name, keep only the last tool with this name.
        for (server_id, tool_name, tool) in context_server_tools {
            if duplicate_tool_names.contains(&tool_name) {
                let available = MAX_TOOL_NAME_LENGTH.saturating_sub(tool_name.len());
                if available >= 2 {
                    let mut disambiguated = server_id.0.to_snake_case();
                    disambiguated.truncate(available - 1);
                    disambiguated.push('_');
                    disambiguated.push_str(&tool_name);
                    tools.insert(disambiguated.into(), tool.clone());
                } else {
                    tools.insert(tool_name, tool.clone());
                }
            } else {
                tools.insert(tool_name, tool.clone());
            }
        }

        tools
    }

    pub(super) fn refresh_turn_tools(&mut self, cx: &App) {
        let tools = self.enabled_tools(cx);
        if let Some(turn) = self.running_turn.as_mut() {
            turn.tools = tools;
        }
    }

    pub(super) fn tool(&self, name: &str) -> Option<Arc<dyn AnyAgentTool>> {
        self.running_turn.as_ref()?.tools.get(name).cloned()
    }

    pub fn has_tool(&self, name: &str) -> bool {
        self.running_turn
            .as_ref()
            .is_some_and(|turn| turn.tools.contains_key(name))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn has_registered_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }
}
