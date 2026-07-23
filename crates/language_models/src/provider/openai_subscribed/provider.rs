use super::*;

impl State {
    pub(super) fn is_authenticated(&self) -> bool {
        self.credentials.is_some()
    }

    pub(super) fn email(&self) -> Option<&str> {
        self.credentials.as_ref().and_then(|c| c.email.as_deref())
    }

    pub(super) fn is_signing_in(&self) -> bool {
        self.sign_in_task.is_some()
    }
}

pub struct OpenAiSubscribedProvider {
    http_client: Arc<dyn HttpClient>,
    state: Entity<State>,
}

impl OpenAiSubscribedProvider {
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Self {
        let state = cx.new(|_cx| State {
            credentials: None,
            sign_in_task: None,
            refresh_task: None,
            load_task: None,
            credentials_provider,
            auth_generation: 0,
            last_auth_error: None,
        });

        let provider = Self { http_client, state };

        provider.load_credentials(cx);

        provider
    }

    fn load_credentials(&self, cx: &mut App) {
        let state = self.state.downgrade();
        let load_task = cx
            .spawn(async move |cx| {
                let credentials_provider =
                    state.read_with(&*cx, |s, _| s.credentials_provider.clone())?;
                let result = credentials_provider
                    .read_credentials(CREDENTIALS_KEY, &*cx)
                    .await;
                state.update(cx, |s, cx| {
                    if let Ok(Some((_, bytes))) = result {
                        match serde_json::from_slice::<CodexCredentials>(&bytes) {
                            Ok(creds) => s.credentials = Some(creds),
                            Err(err) => {
                                log::warn!(
                                    "Failed to deserialize ChatGPT subscription credentials: {err}"
                                );
                            }
                        }
                    }
                    s.load_task = None;
                    cx.notify();
                })?;
                Ok::<(), Arc<anyhow::Error>>(())
            })
            .shared();

        self.state.update(cx, |s, _| {
            s.load_task = Some(load_task);
        });
    }

    fn sign_out(&self, cx: &mut App) -> Task<Result<()>> {
        do_sign_out(&self.state.downgrade(), cx)
    }

    fn create_language_model(&self, model: ChatGptModel) -> Arc<dyn LanguageModel> {
        Arc::new(OpenAiSubscribedLanguageModel {
            id: LanguageModelId::from(model.id().to_string()),
            model,
            state: self.state.clone(),
            http_client: self.http_client.clone(),
            request_limiter: RateLimiter::new(4),
        })
    }
}

impl LanguageModelProviderState for OpenAiSubscribedProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for OpenAiSubscribedProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiOpenAi)
    }

    fn default_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_language_model(ChatGptModel::Gpt55))
    }

    fn default_fast_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        // No GPT-5.5 Mini exists yet; per the OpenAI Codex docs, gpt-5.4-mini
        // is the recommended fast/cheap default alongside gpt-5.5.
        Some(self.create_language_model(ChatGptModel::Gpt54Mini))
    }

    fn provided_models(&self, _cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        ChatGptModel::all()
            .into_iter()
            .map(|m| self.create_language_model(m))
            .collect()
    }

    fn is_authenticated(&self, cx: &App) -> bool {
        self.state.read(cx).is_authenticated()
    }

    fn authenticate(&self, cx: &mut App) -> Task<Result<(), AuthenticateError>> {
        if self.is_authenticated(cx) {
            return Task::ready(Ok(()));
        }
        let load_task = self.state.read(cx).load_task.clone();
        if let Some(load_task) = load_task {
            let weak_state = self.state.downgrade();
            cx.spawn(async move |cx| {
                let _ = load_task.await;
                let is_auth = weak_state
                    .read_with(&*cx, |s, _| s.is_authenticated())
                    .unwrap_or(false);
                if is_auth {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "Sign in with your ChatGPT Plus or Pro subscription to use this provider."
                    )
                    .into())
                }
            })
        } else {
            Task::ready(Err(anyhow!(
                "Sign in with your ChatGPT Plus or Pro subscription to use this provider."
            )
            .into()))
        }
    }

    fn configuration_view(
        &self,
        _target_agent: language_model::ConfigurationViewTargetAgent,
        _window: &mut Window,
        cx: &mut App,
    ) -> AnyView {
        let state = self.state.clone();
        let http_client = self.http_client.clone();
        cx.new(|_cx| ConfigurationView { state, http_client })
            .into()
    }

    fn reset_credentials(&self, cx: &mut App) -> Task<Result<()>> {
        self.sign_out(cx)
    }

    fn authentication_error_message(&self) -> SharedString {
        "Your ChatGPT subscription session is invalid or has expired. \
        Sign in again via the Agent Panel settings to continue."
            .into()
    }

    fn missing_credentials_error_message(&self) -> SharedString {
        "You are not signed in to your ChatGPT account. \
        Sign in via the Agent Panel settings to continue."
            .into()
    }

    fn fast_mode_confirmation(&self, _cx: &App) -> Option<FastModeConfirmation> {
        Some(FastModeConfirmation {
            title: "Enable Fast Mode for OpenAI?".into(),
            message: "Fast mode sends requests using OpenAI's Priority processing tier, which \
                targets significantly lower latency than the standard tier and is billed at a \
                premium per-token rate."
                .into(),
        })
    }
}
