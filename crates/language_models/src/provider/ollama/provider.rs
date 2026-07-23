use super::*;

pub struct OllamaLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    state: Entity<State>,
}
impl OllamaLanguageModelProvider {
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Self {
        let this = Self {
            http_client: http_client.clone(),
            state: cx.new(|cx| {
                cx.observe_global::<SettingsStore>({
                    let mut last_settings = OllamaLanguageModelProvider::settings(cx).clone();
                    move |this: &mut State, cx| {
                        let current_settings = OllamaLanguageModelProvider::settings(cx);
                        let settings_changed = current_settings != &last_settings;
                        if settings_changed {
                            let url_changed = last_settings.api_url != current_settings.api_url;
                            last_settings = current_settings.clone();
                            if url_changed {
                                let credentials_provider = this.credentials_provider.clone();
                                let api_url = Self::api_url(cx);
                                this.api_key_state.handle_url_change(
                                    api_url,
                                    |this| &mut this.api_key_state,
                                    credentials_provider,
                                    cx,
                                );
                                this.fetched_models.clear();
                                this.authenticate(cx).detach();
                            }
                            cx.notify();
                        }
                    }
                })
                .detach();

                State {
                    http_client,
                    fetched_models: Default::default(),
                    fetch_model_task: None,
                    api_key_state: ApiKeyState::new(Self::api_url(cx), (*API_KEY_ENV_VAR).clone()),
                    credentials_provider,
                }
            }),
        };
        this
    }

    pub(super) fn settings(cx: &App) -> &OllamaSettings {
        &AllLanguageModelSettings::get_global(cx).ollama
    }

    pub(super) fn api_url(cx: &App) -> SharedString {
        let api_url = &Self::settings(cx).api_url;
        if api_url.is_empty() {
            OLLAMA_API_URL.into()
        } else {
            SharedString::new(api_url.as_str())
        }
    }

    pub(super) fn has_custom_url(cx: &App) -> bool {
        Self::settings(cx).api_url != OLLAMA_API_URL
    }
}

impl LanguageModelProviderState for OllamaLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for OllamaLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiOllama)
    }

    fn default_model(&self, _: &App) -> Option<Arc<dyn LanguageModel>> {
        // We shouldn't try to select default model, because it might lead to a load call for an unloaded model.
        // In a constrained environment where user might not have enough resources it'll be a bad UX to select something
        // to load by default.
        None
    }

    fn default_fast_model(&self, _: &App) -> Option<Arc<dyn LanguageModel>> {
        // See explanation for default_model.
        None
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        let mut models: HashMap<String, ollama::Model> = HashMap::default();
        let settings = OllamaLanguageModelProvider::settings(cx);

        if settings.auto_discover {
            // Add models from the Ollama API
            for model in self.state.read(cx).fetched_models.iter() {
                let mut model = model.clone();
                if let Some(context_window) = settings.context_window {
                    model.max_tokens = context_window;
                }
                models.insert(model.name.clone(), model);
            }
        }

        // Override with available models from settings
        merge_settings_into_models(
            &mut models,
            &settings.available_models,
            settings.context_window,
        );

        let mut models = models
            .into_values()
            .map(|model| {
                Arc::new(OllamaLanguageModel {
                    id: LanguageModelId::from(model.name.clone()),
                    disabled: model.disabled.as_ref().map(|d| DisabledReason::new(d)),
                    model,
                    http_client: self.http_client.clone(),
                    request_limiter: RateLimiter::new(4),
                    state: self.state.clone(),
                }) as Arc<dyn LanguageModel>
            })
            .collect::<Vec<_>>();
        models.sort_by_key(|model| model.name());
        models
    }

    fn is_authenticated(&self, cx: &App) -> bool {
        self.state.read(cx).is_authenticated()
    }

    fn authenticate(&self, cx: &mut App) -> Task<Result<(), AuthenticateError>> {
        self.state.update(cx, |state, cx| state.authenticate(cx))
    }

    fn configuration_view(
        &self,
        _target_agent: language_model::ConfigurationViewTargetAgent,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyView {
        let state = self.state.clone();
        cx.new(|cx| ConfigurationView::new(state, window, cx))
            .into()
    }

    fn reset_credentials(&self, cx: &mut App) -> Task<Result<()>> {
        self.state
            .update(cx, |state, cx| state.set_api_key(None, cx))
    }
}
fn merge_settings_into_models(
    models: &mut HashMap<String, ollama::Model>,
    available_models: &[AvailableModel],
    context_window: Option<u64>,
) {
    for setting_model in available_models {
        if let Some(model) = models.get_mut(&setting_model.name) {
            if context_window.is_none() {
                model.max_tokens = setting_model.max_tokens;
            }
            model.display_name = setting_model.display_name.clone();
            model.keep_alive = setting_model.keep_alive.clone();
            model.supports_tools = setting_model.supports_tools;
            model.supports_vision = setting_model.supports_images;
            model.supports_thinking = setting_model.supports_thinking;
        } else {
            models.insert(
                setting_model.name.clone(),
                ollama::Model {
                    name: setting_model.name.clone(),
                    display_name: setting_model.display_name.clone(),
                    max_tokens: context_window.unwrap_or(setting_model.max_tokens),
                    keep_alive: setting_model.keep_alive.clone(),
                    supports_tools: setting_model.supports_tools,
                    supports_vision: setting_model.supports_images,
                    supports_thinking: setting_model.supports_thinking,
                    disabled: None,
                },
            );
        }
    }
}
