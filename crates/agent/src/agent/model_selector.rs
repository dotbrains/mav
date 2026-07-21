use super::*;

pub(super) struct NativeAgentModelSelector {
    pub(super) session_id: acp::SessionId,
    pub(super) connection: NativeAgentConnection,
}

impl acp_thread::AgentModelSelector for NativeAgentModelSelector {
    fn list_models(&self, cx: &mut App) -> Task<Result<acp_thread::AgentModelList>> {
        log::debug!("NativeAgentConnection::list_models called");
        let list = self.connection.0.read(cx).models.model_list.clone();
        Task::ready(if list.is_empty() {
            Err(anyhow::anyhow!("No models available"))
        } else {
            Ok(list)
        })
    }

    fn select_model(&self, model_id: AgentModelId, cx: &mut App) -> Task<Result<()>> {
        log::debug!(
            "Setting model for session {}: {}",
            self.session_id,
            model_id
        );
        let Some(thread) = self
            .connection
            .0
            .read(cx)
            .sessions
            .get(&self.session_id)
            .map(|session| session.thread.clone())
        else {
            return Task::ready(Err(anyhow!("Session not found")));
        };

        let Some(model) = self.connection.0.read(cx).models.model_from_id(&model_id) else {
            return Task::ready(Err(anyhow!("Invalid model ID {}", model_id)));
        };

        let favorite = agent_settings::AgentSettings::get_global(cx)
            .favorite_models
            .iter()
            .find(|favorite| {
                favorite.provider.0 == model.provider_id().0.as_ref()
                    && favorite.model == model.id().0.as_ref()
            })
            .cloned();

        let LanguageModelSelection {
            enable_thinking,
            effort,
            speed,
            ..
        } = agent_settings::language_model_to_selection(&model, favorite.as_ref());

        thread.update(cx, |thread, cx| {
            thread.set_model(model.clone(), cx);
            thread.set_thinking_effort(effort.clone(), cx);
            thread.set_thinking_enabled(enable_thinking, cx);
            if let Some(speed) = speed {
                thread.set_speed(speed, cx);
            }
        });

        update_settings_file(
            self.connection.0.read(cx).fs.clone(),
            cx,
            move |settings, cx| {
                let provider = model.provider_id().0.to_string();
                let model = model.id().0.to_string();
                let enable_thinking = thread.read(cx).thinking_enabled();
                let speed = thread.read(cx).speed();
                settings
                    .agent
                    .get_or_insert_default()
                    .set_model(LanguageModelSelection {
                        provider: provider.into(),
                        model,
                        enable_thinking,
                        effort,
                        speed,
                    });
            },
        );

        Task::ready(Ok(()))
    }

    fn selected_model(&self, cx: &mut App) -> Task<Result<acp_thread::AgentModelInfo>> {
        let Some(thread) = self
            .connection
            .0
            .read(cx)
            .sessions
            .get(&self.session_id)
            .map(|session| session.thread.clone())
        else {
            return Task::ready(Err(anyhow!("Session not found")));
        };
        let Some(model) = thread.read(cx).model() else {
            return Task::ready(Err(anyhow!("Model not found")));
        };
        let Some(provider) = LanguageModelRegistry::read_global(cx).provider(&model.provider_id())
        else {
            return Task::ready(Err(anyhow!("Provider not found")));
        };
        Task::ready(Ok(LanguageModels::map_language_model_to_info(
            model, &provider,
        )))
    }

    fn favorite_model_ids(&self, cx: &mut App) -> HashSet<AgentModelId> {
        agent_settings::AgentSettings::get_global(cx)
            .favorite_model_ids()
            .into_iter()
            .map(AgentModelId::from)
            .collect()
    }

    fn toggle_favorite_model(&self, model_id: AgentModelId, should_be_favorite: bool, cx: &App) {
        let selection = model_id_to_selection(&model_id, cx);
        let fs = self.connection.0.read(cx).fs.clone();
        update_settings_file(fs, cx, move |settings, _| {
            let agent = settings.agent.get_or_insert_default();
            if should_be_favorite {
                agent.add_favorite_model(selection.clone());
            } else {
                agent.remove_favorite_model(&selection);
            }
        });
    }

    fn watch(&self, cx: &mut App) -> Option<watch::Receiver<()>> {
        Some(self.connection.0.read(cx).models.watch())
    }

    fn should_render_footer(&self) -> bool {
        true
    }
}

fn model_id_to_selection(model_id: &AgentModelId, cx: &App) -> LanguageModelSelection {
    let id = model_id.as_ref();
    let (provider, model) = id.split_once('/').unwrap_or(("", id));

    let provider_id = LanguageModelProviderId(provider.to_string().into());
    let model_id = LanguageModelId(model.to_string().into());
    let resolved = LanguageModelRegistry::global(cx)
        .read(cx)
        .provider(&provider_id)
        .and_then(|provider| {
            provider
                .provided_models(cx)
                .into_iter()
                .find(|model| model.id() == model_id)
        });

    let Some(resolved) = resolved else {
        return LanguageModelSelection {
            provider: provider.to_owned().into(),
            model: model.to_owned(),
            enable_thinking: false,
            effort: None,
            speed: None,
        };
    };

    let current_user_selection = agent_settings::AgentSettings::get_global(cx)
        .default_model
        .as_ref()
        .filter(|selection| {
            selection.provider.0 == resolved.provider_id().0.as_ref()
                && selection.model == resolved.id().0.as_ref()
        })
        .cloned();

    agent_settings::language_model_to_selection(&resolved, current_user_selection.as_ref())
}
