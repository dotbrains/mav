use super::*;

pub struct State {
    pub(super) api_key_state: ApiKeyState,
    pub(super) credentials_provider: Arc<dyn CredentialsProvider>,
    pub(super) http_client: Arc<dyn HttpClient>,
    pub(super) fetched_models: Vec<ollama::Model>,
    pub(super) fetch_model_task: Option<Task<Result<()>>>,
}

impl State {
    pub(super) fn is_authenticated(&self) -> bool {
        !self.fetched_models.is_empty()
    }

    pub(super) fn set_api_key(
        &mut self,
        api_key: Option<String>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = OllamaLanguageModelProvider::api_url(cx);
        let task = self.api_key_state.store(
            api_url,
            api_key,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        );

        self.fetched_models.clear();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.restart_fetch_models_task(cx))
                .ok();
            result
        })
    }

    pub(super) fn authenticate(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), AuthenticateError>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = OllamaLanguageModelProvider::api_url(cx);
        let task = self.api_key_state.load_if_needed(
            api_url,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        );

        // Always try to fetch models - if no API key is needed (local Ollama), it will work
        // If API key is needed and provided, it will work
        // If API key is needed and not provided, it will fail gracefully
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.restart_fetch_models_task(cx))
                .ok();
            result
        })
    }

    fn fetch_models(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        let http_client = Arc::clone(&self.http_client);
        let settings = OllamaLanguageModelProvider::settings(cx);
        let api_url = OllamaLanguageModelProvider::api_url(cx);
        let api_key = self.api_key_state.key(&api_url);
        let extra_headers = settings.custom_headers.clone();

        // As a proxy for the server being "authenticated", we'll check if its up by fetching the models
        cx.spawn(async move |this, cx| {
            let models = get_models(
                http_client.as_ref(),
                &api_url,
                api_key.as_deref(),
                &extra_headers,
            )
            .await?;

            let tasks = models
                .into_iter()
                // Since there is no metadata from the Ollama API
                // indicating which models are embedding models,
                // simply filter out models with "-embed" in their name
                .filter(|model| !model.name.contains("-embed"))
                .map(|model| {
                    let http_client = Arc::clone(&http_client);
                    let api_url = api_url.clone();
                    let api_key = api_key.clone();
                    let extra_headers = extra_headers.clone();
                    async move {
                        let name = model.name.as_str();

                        show_model(
                            http_client.as_ref(),
                            &api_url,
                            api_key.as_deref(),
                            name,
                            &extra_headers,
                        )
                        .await
                        .map_or_else(
                            |error| {
                                ollama::Model::new_disabled(
                                    name,
                                    format!("Failed to fetch model from API: {error}",),
                                )
                            },
                            |model| {
                                ollama::Model::new(
                                    name,
                                    model.context_length,
                                    Some(model.supports_tools()),
                                    Some(model.supports_vision()),
                                    Some(model.supports_thinking()),
                                )
                            },
                        )
                    }
                });

            // Rate-limit capability fetches
            // since there is an arbitrary number of models available
            let mut ollama_models: Vec<_> = futures::stream::iter(tasks)
                .buffer_unordered(5)
                .collect()
                .await;

            ollama_models.sort_by(|a, b| a.name.cmp(&b.name));

            this.update(cx, |this, cx| {
                this.fetched_models = ollama_models;
                cx.notify();
            })
        })
    }

    pub(super) fn restart_fetch_models_task(&mut self, cx: &mut Context<Self>) {
        let task = self.fetch_models(cx);
        self.fetch_model_task.replace(task);
    }
}
