use super::*;

pub(super) struct OpenRouterLanguageModel {
    pub(super) id: LanguageModelId,
    pub(super) model: open_router::Model,
    pub(super) state: Entity<State>,
    pub(super) http_client: Arc<dyn HttpClient>,
    pub(super) request_limiter: RateLimiter,
}

impl OpenRouterLanguageModel {
    fn stream_completion(
        &self,
        request: open_router::Request,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            futures::stream::BoxStream<
                'static,
                Result<ResponseStreamEvent, open_router::OpenRouterError>,
            >,
            LanguageModelCompletionError,
        >,
    > {
        let http_client = self.http_client.clone();
        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, cx| {
            let api_url = OpenRouterLanguageModelProvider::api_url(cx);
            let extra_headers = OpenRouterLanguageModelProvider::settings(cx)
                .custom_headers
                .clone();
            (state.api_key_state.key(&api_url), api_url, extra_headers)
        });

        async move {
            let Some(api_key) = api_key else {
                return Err(LanguageModelCompletionError::NoApiKey {
                    provider: PROVIDER_NAME,
                });
            };
            let request = open_router::stream_completion(
                http_client.as_ref(),
                &api_url,
                &api_key,
                request,
                &extra_headers,
            );
            request.await.map_err(Into::into)
        }
        .boxed()
    }
}

impl LanguageModel for OpenRouterLanguageModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelName {
        LanguageModelName::from(self.model.display_name().to_string())
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn supports_tools(&self) -> bool {
        self.model.supports_tool_calls()
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn supports_thinking(&self) -> bool {
        matches!(self.model.mode, OpenRouterModelMode::Thinking { .. })
    }

    fn tool_input_format(&self) -> LanguageModelToolSchemaFormat {
        let model_id = self.model.id().trim().to_lowercase();
        if model_id.contains("gemini") || model_id.contains("grok") {
            LanguageModelToolSchemaFormat::JsonSchemaSubset
        } else {
            LanguageModelToolSchemaFormat::JsonSchema
        }
    }

    fn telemetry_id(&self) -> String {
        format!("openrouter/{}", self.model.id())
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_token_count()
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.model.max_output_tokens()
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        match choice {
            LanguageModelToolChoice::Auto => true,
            LanguageModelToolChoice::Any => true,
            LanguageModelToolChoice::None => true,
        }
    }

    fn supports_images(&self) -> bool {
        self.model.supports_images.unwrap_or(false)
    }

    fn stream_completion(
        &self,
        request: LanguageModelRequest,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            futures::stream::BoxStream<
                'static,
                Result<LanguageModelCompletionEvent, LanguageModelCompletionError>,
            >,
            LanguageModelCompletionError,
        >,
    > {
        let openrouter_request = into_open_router(request, &self.model, self.max_output_tokens());
        let request = self.stream_completion(openrouter_request, cx);
        let future = self.request_limiter.stream(async move {
            let response = request.await?;
            Ok(OpenRouterEventMapper::new().map_stream(response))
        });
        async move { Ok(future.await?.boxed()) }.boxed()
    }
}
