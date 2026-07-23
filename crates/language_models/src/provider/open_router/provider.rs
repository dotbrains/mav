use super::*;

impl State {
    pub(super) fn is_authenticated(&self) -> bool {
        self.api_key_state.has_key()
    }

    pub(super) fn set_api_key(
        &mut self,
        api_key: Option<String>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = OpenRouterLanguageModelProvider::api_url(cx);
        let task = self.api_key_state.store(
            api_url,
            api_key,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        );

        cx.spawn(async move |this, cx| {
            let result = task.await?;
            this.update(cx, |this, cx| this.restart_fetch_models_task(cx))
                .ok();
            Ok(result)
        })
    }

    pub(super) fn authenticate(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), AuthenticateError>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = OpenRouterLanguageModelProvider::api_url(cx);
        let task = self.api_key_state.load_if_needed(
            api_url,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        );

        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.restart_fetch_models_task(cx))
                .ok();
            result
        })
    }

    fn fetch_models(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), LanguageModelCompletionError>> {
        let http_client = self.http_client.clone();
        let api_url = OpenRouterLanguageModelProvider::api_url(cx);
        let extra_headers = OpenRouterLanguageModelProvider::settings(cx)
            .custom_headers
            .clone();
        let Some(api_key) = self.api_key_state.key(&api_url) else {
            return Task::ready(Err(LanguageModelCompletionError::NoApiKey {
                provider: PROVIDER_NAME,
            }));
        };
        cx.spawn(async move |this, cx| {
            let models = list_models(http_client.as_ref(), &api_url, &api_key, &extra_headers)
                .await
                .map_err(LanguageModelCompletionError::from)?;

            this.update(cx, |this, cx| {
                this.available_models = models;
                cx.notify();
            })
            .map_err(|e| LanguageModelCompletionError::Other(e))?;

            Ok(())
        })
    }

    fn restart_fetch_models_task(&mut self, cx: &mut Context<Self>) {
        if self.is_authenticated() {
            let task = self.fetch_models(cx);
            self.fetch_models_task.replace(task);
        } else {
            self.available_models.clear();
        }
    }
}

impl OpenRouterLanguageModelProvider {
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Self {
        let state = cx.new(|cx| {
            cx.observe_global::<SettingsStore>({
                let mut last_settings = OpenRouterLanguageModelProvider::settings(cx).clone();
                move |this: &mut State, cx| {
                    let current_settings = OpenRouterLanguageModelProvider::settings(cx);
                    let settings_changed = current_settings != &last_settings;
                    if settings_changed {
                        last_settings = current_settings.clone();
                        this.authenticate(cx).detach();
                        cx.notify();
                    }
                }
            })
            .detach();
            State {
                api_key_state: ApiKeyState::new(Self::api_url(cx), (*API_KEY_ENV_VAR).clone()),
                credentials_provider,
                http_client: http_client.clone(),
                available_models: Vec::new(),
                fetch_models_task: None,
            }
        });

        Self { http_client, state }
    }

    pub(super) fn settings(cx: &App) -> &OpenRouterSettings {
        &crate::AllLanguageModelSettings::get_global(cx).open_router
    }

    pub(super) fn api_url(cx: &App) -> SharedString {
        let api_url = &Self::settings(cx).api_url;
        if api_url.is_empty() {
            OPEN_ROUTER_API_URL.into()
        } else {
            SharedString::new(api_url.as_str())
        }
    }

    fn create_language_model(&self, model: open_router::Model) -> Arc<dyn LanguageModel> {
        Arc::new(OpenRouterLanguageModel {
            id: LanguageModelId::from(model.id().to_string()),
            model,
            state: self.state.clone(),
            http_client: self.http_client.clone(),
            request_limiter: RateLimiter::new(4),
        })
    }
}

impl LanguageModelProviderState for OpenRouterLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for OpenRouterLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiOpenRouter)
    }

    fn default_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_language_model(open_router::Model::default()))
    }

    fn default_fast_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        None
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        let mut models_from_api = self.state.read(cx).available_models.clone();
        let mut settings_models = Vec::new();

        for model in &Self::settings(cx).available_models {
            settings_models.push(open_router::Model {
                name: model.name.clone(),
                display_name: model.display_name.clone(),
                max_tokens: model.max_tokens,
                supports_tools: model.supports_tools,
                supports_images: model.supports_images,
                mode: model.mode.unwrap_or_default(),
                provider: model.provider.clone(),
            });
        }

        for settings_model in &settings_models {
            if let Some(pos) = models_from_api
                .iter()
                .position(|m| m.name == settings_model.name)
            {
                models_from_api[pos] = settings_model.clone();
            } else {
                models_from_api.push(settings_model.clone());
            }
        }

        models_from_api
            .into_iter()
            .map(|model| self.create_language_model(model))
            .collect()
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
        cx.new(|cx| ConfigurationView::new(self.state.clone(), window, cx))
            .into()
    }

    fn reset_credentials(&self, cx: &mut App) -> Task<Result<()>> {
        self.state
            .update(cx, |state, cx| state.set_api_key(None, cx))
    }

    fn configuration_view_v2(
        &self,
        _target_agent: language_model::ConfigurationViewTargetAgent,
        window: &mut Window,
        cx: &mut App,
    ) -> ProviderConfigurationView {
        let state = self.state.clone();
        ProviderConfigurationView::Inline(
            cx.new(|cx| {
                crate::ApiKeyEditor::new(
                    state,
                    "https://openrouter.ai/keys",
                    "sk-or-...",
                    |state, _cx| crate::api_key_status(&state.api_key_state),
                    |state, key, cx| state.update(cx, |state, cx| state.set_api_key(Some(key), cx)),
                    |state, cx| state.update(cx, |state, cx| state.set_api_key(None, cx)),
                    window,
                    cx,
                )
            })
            .into(),
        )
    }
}
