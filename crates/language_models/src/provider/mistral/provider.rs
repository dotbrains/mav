use super::*;

pub struct MistralLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    pub state: Entity<State>,
}
struct GlobalMistralLanguageModelProvider(Arc<MistralLanguageModelProvider>);

impl Global for GlobalMistralLanguageModelProvider {}

impl MistralLanguageModelProvider {
    pub fn try_global(cx: &App) -> Option<&Arc<MistralLanguageModelProvider>> {
        cx.try_global::<GlobalMistralLanguageModelProvider>()
            .map(|this| &this.0)
    }

    pub fn global(
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Arc<Self> {
        if let Some(this) = cx.try_global::<GlobalMistralLanguageModelProvider>() {
            return this.0.clone();
        }
        let state = cx.new(|cx| {
            cx.observe_global::<SettingsStore>(|this: &mut State, cx| {
                let credentials_provider = this.credentials_provider.clone();
                let api_url = Self::api_url(cx);
                this.api_key_state.handle_url_change(
                    api_url,
                    |this| &mut this.api_key_state,
                    credentials_provider,
                    cx,
                );
                cx.notify();
            })
            .detach();
            State {
                api_key_state: ApiKeyState::new(Self::api_url(cx), (*API_KEY_ENV_VAR).clone()),
                credentials_provider,
            }
        });

        let this = Arc::new(Self { http_client, state });
        cx.set_global(GlobalMistralLanguageModelProvider(this));
        cx.global::<GlobalMistralLanguageModelProvider>().0.clone()
    }

    pub(super) fn create_language_model(&self, model: mistral::Model) -> Arc<dyn LanguageModel> {
        Arc::new(MistralLanguageModel {
            id: LanguageModelId::from(model.id().to_string()),
            model,
            state: self.state.clone(),
            http_client: self.http_client.clone(),
            request_limiter: RateLimiter::new(4),
        })
    }

    pub(super) fn settings(cx: &App) -> &MistralSettings {
        &crate::AllLanguageModelSettings::get_global(cx).mistral
    }

    pub fn api_url(cx: &App) -> SharedString {
        let api_url = &Self::settings(cx).api_url;
        if api_url.is_empty() {
            mistral::MISTRAL_API_URL.into()
        } else {
            SharedString::new(api_url.as_str())
        }
    }
}

impl LanguageModelProviderState for MistralLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for MistralLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiMistral)
    }

    fn default_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_language_model(mistral::Model::default()))
    }

    fn default_fast_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_language_model(mistral::Model::default_fast()))
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        let mut models = BTreeMap::default();

        // Add base models from mistral::Model::iter()
        for model in mistral::Model::iter() {
            if !matches!(model, mistral::Model::Custom { .. }) {
                models.insert(model.id().to_string(), model);
            }
        }

        // Override with available models from settings
        for model in &Self::settings(cx).available_models {
            models.insert(
                model.name.clone(),
                mistral::Model::Custom {
                    name: model.name.clone(),
                    display_name: model.display_name.clone(),
                    max_tokens: model.max_tokens,
                    max_output_tokens: model.max_output_tokens,
                    max_completion_tokens: model.max_completion_tokens,
                    supports_tools: model.supports_tools,
                    supports_images: model.supports_images,
                    supports_thinking: model.supports_thinking,
                },
            );
        }

        models
            .into_values()
            .map(|model| {
                Arc::new(MistralLanguageModel {
                    id: LanguageModelId::from(model.id().to_string()),
                    model,
                    state: self.state.clone(),
                    http_client: self.http_client.clone(),
                    request_limiter: RateLimiter::new(4),
                }) as Arc<dyn LanguageModel>
            })
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
                    "https://console.mistral.ai/api-keys",
                    "Paste your Mistral API key",
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
