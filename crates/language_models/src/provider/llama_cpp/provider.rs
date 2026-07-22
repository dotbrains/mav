use super::*;

impl LlamaCppLanguageModelProvider {
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Self {
        let capability_cells: CapabilityCells = Arc::new(RwLock::new(HashMap::default()));
        let loading_progress: LoadingProgress = Arc::new(RwLock::new(HashMap::default()));
        let this = Self {
            http_client: http_client.clone(),
            capability_cells: capability_cells.clone(),
            loading_progress: loading_progress.clone(),
            state: cx.new(|cx| {
                cx.observe_global::<SettingsStore>({
                    let mut last_settings = LlamaCppLanguageModelProvider::settings(cx).clone();
                    move |this: &mut State, cx| {
                        let current_settings = LlamaCppLanguageModelProvider::settings(cx);
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
                                // Drop the event stream so it reconnects against
                                // the new URL (re-auth below restarts it).
                                this.model_event_task = None;
                                write_recover(&this.loading_progress).clear();
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
                    model_event_task: None,
                    capability_cells,
                    loading_progress,
                    api_key_state: ApiKeyState::new(Self::api_url(cx), (*API_KEY_ENV_VAR).clone()),
                    credentials_provider,
                }
            }),
        };
        // Discover eagerly so a running server is picked up without opening settings.
        this.state
            .update(cx, |state, cx| state.restart_fetch_models_task(cx));
        this
    }

    pub(super) fn settings(cx: &App) -> &LlamaCppSettings {
        &AllLanguageModelSettings::get_global(cx).llama_cpp
    }

    pub(super) fn api_url(cx: &App) -> SharedString {
        let api_url = &Self::settings(cx).api_url;
        if api_url.is_empty() {
            LLAMA_CPP_API_URL.into()
        } else {
            SharedString::new(api_url.as_str())
        }
    }

    pub(super) fn has_custom_url(cx: &App) -> bool {
        Self::settings(cx).api_url != LLAMA_CPP_API_URL
    }
}

impl LanguageModelProviderState for LlamaCppLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for LlamaCppLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiLlamaCpp)
    }

    fn default_model(&self, _: &App) -> Option<Arc<dyn LanguageModel>> {
        // No default model: in router mode it could trigger an expensive load of
        // an unloaded model on a constrained machine.
        None
    }

    fn default_fast_model(&self, _: &App) -> Option<Arc<dyn LanguageModel>> {
        // See explanation for default_model.
        None
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        let settings = LlamaCppLanguageModelProvider::settings(cx);
        let effective = compute_effective_models(&self.state.read(cx).fetched_models, settings);

        // Refresh the shared capability map so open conversations pick up settings changes.
        sync_capability_cells(&self.capability_cells, &effective);
        let mut models = effective
            .into_values()
            .map(|model| {
                Arc::new(LlamaCppLanguageModel {
                    id: LanguageModelId::from(model.name.clone()),
                    name: model.name.clone(),
                    display_name: model.display_name().to_string(),
                    fallback_capabilities: LiveCapabilities::of(&model),
                    supports_images: model.supports_images,
                    capability_cells: self.capability_cells.clone(),
                    loading_progress: self.loading_progress.clone(),
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
