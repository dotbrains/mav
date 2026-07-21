use super::*;

pub struct LanguageModels {
    /// Access language model by ID
    pub(super) models: HashMap<AgentModelId, Arc<dyn LanguageModel>>,
    /// Cached list for returning language model information
    pub(super) model_list: acp_thread::AgentModelList,
    pub(super) refresh_models_rx: watch::Receiver<()>,
    pub(super) refresh_models_tx: watch::Sender<()>,
    pub(super) _authenticate_all_providers_task: Task<()>,
}

impl LanguageModels {
    pub(super) fn new(cx: &mut App) -> Self {
        let (refresh_models_tx, refresh_models_rx) = watch::channel(());

        let mut this = Self {
            models: HashMap::default(),
            model_list: acp_thread::AgentModelList::Grouped(IndexMap::default()),
            refresh_models_rx,
            refresh_models_tx,
            _authenticate_all_providers_task: Self::authenticate_all_language_model_providers(cx),
        };
        this.refresh_list(cx);
        this
    }

    pub(super) fn refresh_list(&mut self, cx: &App) {
        let providers = LanguageModelRegistry::global(cx)
            .read(cx)
            .visible_providers()
            .into_iter()
            .filter(|provider| provider.is_authenticated(cx))
            .collect::<Vec<_>>();

        let mut language_model_list = IndexMap::default();
        let mut recommended_models = HashSet::default();

        let mut recommended = Vec::new();
        for provider in &providers {
            for model in provider.recommended_models(cx) {
                recommended_models.insert((model.provider_id(), model.id()));
                recommended.push(Self::map_language_model_to_info(&model, provider));
            }
        }
        if !recommended.is_empty() {
            language_model_list.insert(
                acp_thread::AgentModelGroupName("Recommended".into()),
                recommended,
            );
        }

        let mut models = HashMap::default();
        for provider in providers {
            let mut provider_models = Vec::new();
            for model in provider.provided_models(cx) {
                let model_info = Self::map_language_model_to_info(&model, &provider);
                let model_id = model_info.id.clone();
                provider_models.push(model_info);
                models.insert(model_id, model);
            }
            if !provider_models.is_empty() {
                language_model_list.insert(
                    acp_thread::AgentModelGroupName(provider.name().0.clone()),
                    provider_models,
                );
            }
        }

        self.models = models;
        self.model_list = acp_thread::AgentModelList::Grouped(language_model_list);
        self.refresh_models_tx.send(()).ok();
    }

    pub(super) fn watch(&self) -> watch::Receiver<()> {
        self.refresh_models_rx.clone()
    }

    pub fn notify_model_selection_changed(&mut self) {
        self.refresh_models_tx.send(()).ok();
    }

    pub fn model_from_id(&self, model_id: &AgentModelId) -> Option<Arc<dyn LanguageModel>> {
        self.models.get(model_id).cloned()
    }

    pub(super) fn map_language_model_to_info(
        model: &Arc<dyn LanguageModel>,
        provider: &Arc<dyn LanguageModelProvider>,
    ) -> acp_thread::AgentModelInfo {
        acp_thread::AgentModelInfo {
            id: Self::model_id(model),
            name: model.name().0,
            description: None,
            icon: Some(match provider.icon() {
                IconOrSvg::Svg(path) => acp_thread::AgentModelIcon::Path(path),
                IconOrSvg::Icon(name) => acp_thread::AgentModelIcon::Named(name),
            }),
            is_latest: model.is_latest(),
            cost: model.model_cost_info().map(|cost| cost.to_shared_string()),
            disabled: model.is_disabled(),
        }
    }

    pub(super) fn model_id(model: &Arc<dyn LanguageModel>) -> AgentModelId {
        AgentModelId::new(format!("{}/{}", model.provider_id().0, model.id().0))
    }

    pub(super) fn authenticate_all_language_model_providers(cx: &mut App) -> Task<()> {
        let authenticate_all_providers = LanguageModelRegistry::global(cx)
            .read(cx)
            .visible_providers()
            .iter()
            .map(|provider| (provider.id(), provider.name(), provider.authenticate(cx)))
            .collect::<Vec<_>>();

        cx.spawn(async move |cx| {
            for (provider_id, provider_name, authenticate_task) in authenticate_all_providers {
                if let Err(err) = authenticate_task.await {
                    match err {
                        language_model::AuthenticateError::CredentialsNotFound => {
                            // Since we're authenticating these providers in the
                            // background for the purposes of populating the
                            // language selector, we don't care about providers
                            // where the credentials are not found.
                        }
                        language_model::AuthenticateError::ConnectionRefused => {
                            // Not logging connection refused errors as they are mostly from LM Studio's noisy auth failures.
                            // LM Studio only has one auth method (endpoint call) which fails for users who haven't enabled it.
                            // TODO: Better manage LM Studio auth logic to avoid these noisy failures.
                        }
                        _ => {
                            // Some providers have noisy failure states that we
                            // don't want to spam the logs with every time the
                            // language model selector is initialized.
                            //
                            // Ideally these should have more clear failure modes
                            // that we know are safe to ignore here, like what we do
                            // with `CredentialsNotFound` above.
                            match provider_id.0.as_ref() {
                                "lmstudio" | "ollama" => {
                                    // LM Studio and Ollama both make fetch requests to the local APIs to determine if they are "authenticated".
                                    //
                                    // These fail noisily, so we don't log them.
                                }
                                "copilot_chat" => {
                                    // Copilot Chat returns an error if Copilot is not enabled, so we don't log those errors.
                                }
                                _ => {
                                    log::error!(
                                        "Failed to authenticate provider: {}: {err:#}",
                                        provider_name.0
                                    );
                                }
                            }
                        }
                    }
                }
            }

            cx.update(::language_models::update_environment_fallback_model);
        })
    }
}
