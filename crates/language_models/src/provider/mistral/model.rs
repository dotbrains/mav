use super::*;

pub struct MistralLanguageModel {
    pub(super) id: LanguageModelId,
    pub(super) model: mistral::Model,
    pub(super) state: Entity<State>,
    pub(super) http_client: Arc<dyn HttpClient>,
    pub(super) request_limiter: RateLimiter,
}

impl MistralLanguageModel {
    fn stream_completion(
        &self,
        request: mistral::Request,
        affinity: Option<String>,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<futures::stream::BoxStream<'static, Result<mistral::StreamResponse>>>,
    > {
        let http_client = self.http_client.clone();

        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, cx| {
            let api_url = MistralLanguageModelProvider::api_url(cx);
            let extra_headers = MistralLanguageModelProvider::settings(cx)
                .custom_headers
                .clone();
            (state.api_key_state.key(&api_url), api_url, extra_headers)
        });

        let future = self.request_limiter.stream(async move {
            let Some(api_key) = api_key else {
                return Err(LanguageModelCompletionError::NoApiKey {
                    provider: PROVIDER_NAME,
                });
            };
            let request = mistral::stream_completion(
                http_client.as_ref(),
                &api_url,
                &api_key,
                request,
                affinity,
                &extra_headers,
            );
            let response = request.await?;
            Ok(response)
        });

        async move { Ok(future.await?.boxed()) }.boxed()
    }
}

impl LanguageModel for MistralLanguageModel {
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
        self.model.supports_tools()
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn supports_tool_choice(&self, _choice: LanguageModelToolChoice) -> bool {
        self.model.supports_tools()
    }

    fn supports_images(&self) -> bool {
        self.model.supports_images()
    }

    fn telemetry_id(&self) -> String {
        format!("mistral/{}", self.model.id())
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_token_count()
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.model.max_output_tokens()
    }

    fn stream_completion(
        &self,
        request: LanguageModelRequest,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            BoxStream<'static, Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>,
            LanguageModelCompletionError,
        >,
    > {
        let (request, affinity) =
            into_mistral(request, self.model.clone(), self.max_output_tokens());
        let stream = self.stream_completion(request, affinity, cx);

        async move {
            let stream = stream.await?;
            let mapper = MistralEventMapper::new();
            Ok(mapper.map_stream(stream).boxed())
        }
        .boxed()
    }
}
