use super::*;

impl Thread {
    pub fn from_db(
        id: acp::SessionId,
        db_thread: DbThread,
        project: Entity<Project>,
        project_context: Entity<ProjectContext>,
        context_server_registry: Entity<ContextServerRegistry>,
        templates: Arc<Templates>,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = AgentSettings::get_global(cx);
        let profile_id = db_thread
            .profile
            .unwrap_or_else(|| settings.default_profile.clone());

        let saved_selection = db_thread.model.map(|model| SelectedModel {
            provider: model.provider.into(),
            model: model.model.into(),
        });

        let resolved_saved_model = LanguageModelRegistry::global(cx).update(cx, |registry, cx| {
            saved_selection
                .as_ref()
                .and_then(|selection| registry.select_model(selection, cx))
                .map(|configured| configured.model)
        });

        let model = match (resolved_saved_model, saved_selection) {
            (Some(model), _) => ThreadModel::Ready(model),
            (None, Some(selection)) => ThreadModel::Unresolved(selection),
            (None, None) => Self::resolve_profile_model(&profile_id, cx)
                .or_else(|| {
                    LanguageModelRegistry::global(cx).update(cx, |registry, _cx| {
                        registry.default_model().map(|model| model.model)
                    })
                })
                .map_or(ThreadModel::Unset, ThreadModel::Ready),
        };

        let (prompt_capabilities_tx, prompt_capabilities_rx) = watch::channel(
            Self::prompt_capabilities(model.as_model().map(|model| model.as_ref())),
        );

        let action_log = cx.new(|_| ActionLog::new(project.clone()));

        Self {
            id,
            prompt_id: PromptId::new(),
            title: if db_thread.title.is_empty() {
                None
            } else {
                Some(db_thread.title.clone())
            },
            pending_title_generation: None,
            title_generation_failed: false,
            pending_summary_generation: None,
            summary: db_thread.detailed_summary,
            messages: db_thread.messages,
            user_store: project.read(cx).user_store(),
            running_turn: None,
            end_turn_at_next_boundary: false,
            pending_message: None,
            tools: BTreeMap::default(),
            request_token_usage: db_thread.request_token_usage.clone(),
            cumulative_token_usage: db_thread.cumulative_token_usage,
            current_request_token_usage: TokenUsage::default(),
            pending_compaction_telemetry: None,
            initial_project_snapshot: Task::ready(db_thread.initial_project_snapshot).shared(),
            context_server_registry,
            profile_id,
            project_context,
            templates,
            model,
            summarization_model: None,
            thinking_enabled: db_thread.thinking_enabled,
            thinking_effort: db_thread.thinking_effort,
            speed: db_thread.speed,
            project,
            action_log,
            updated_at: db_thread.updated_at,
            prompt_capabilities_tx,
            prompt_capabilities_rx,
            subagent_context: db_thread.subagent_context,
            draft_prompt: db_thread.draft_prompt,
            ui_scroll_position: db_thread.ui_scroll_position.map(|sp| gpui::ListOffset {
                item_ix: sp.item_ix,
                offset_in_item: gpui::px(sp.offset_in_item),
            }),
            running_subagents: Vec::new(),
            inherits_parent_model_settings: true,
            sandboxed_terminal_temp_dir: db_thread.sandboxed_terminal_temp_dir,
            sandbox_grants: Rc::new(RefCell::new(ThreadSandboxGrants::from_db(
                &db_thread.sandbox_grants,
            ))),
        }
    }
}
