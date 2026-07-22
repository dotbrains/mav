use super::*;

pub(super) struct LlamaCppLanguageModel {
    pub(super) id: LanguageModelId,
    /// The model id sent to the server.
    pub(super) name: String,
    pub(super) display_name: String,
    /// Live capabilities shared with the provider, read fresh on each access so an
    /// open conversation reflects the model's real values once it has loaded.
    pub(super) capability_cells: CapabilityCells,
    /// Used when `capability_cells` has no entry (e.g. model removed mid-conversation).
    pub(super) fallback_capabilities: LiveCapabilities,
    /// Available from `/v1/models` hints, so captured at build time.
    pub(super) supports_images: bool,
    /// Shared with the provider; this model's load progress, read by `name` so the
    /// selector can show a loading indicator.
    pub(super) loading_progress: LoadingProgress,
    pub(super) http_client: Arc<dyn HttpClient>,
    pub(super) request_limiter: RateLimiter,
    pub(super) state: Entity<State>,
}

impl LlamaCppLanguageModel {
    /// The model's live capabilities, or the build-time fallback if the map lacks it.
    pub(super) fn capabilities(&self) -> LiveCapabilities {
        read_recover(&self.capability_cells)
            .get(&self.name)
            .copied()
            .unwrap_or(self.fallback_capabilities)
    }

    /// This model's load-status label while loading, read live from the shared map.
    pub(super) fn loading_label(&self) -> Option<SharedString> {
        read_recover(&self.loading_progress)
            .get(&self.name)
            .cloned()
    }

    pub(super) fn to_llama_cpp_request(
        &self,
        request: LanguageModelRequest,
    ) -> llama_cpp::ChatCompletionRequest {
        build_llama_cpp_request(
            &self.name,
            self.supports_images,
            self.capabilities(),
            request,
        )
    }

    pub(super) fn stream_llama_cpp_completion(
        &self,
        request: llama_cpp::ChatCompletionRequest,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<futures::stream::BoxStream<'static, Result<llama_cpp::ResponseStreamEvent>>>,
    > {
        let http_client = self.http_client.clone();
        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, cx| {
            let api_url = LlamaCppLanguageModelProvider::api_url(cx);
            let extra_headers = LlamaCppLanguageModelProvider::settings(cx)
                .custom_headers
                .clone();
            (state.api_key_state.key(&api_url), api_url, extra_headers)
        });

        let future = self.request_limiter.stream(async move {
            let stream = stream_chat_completion(
                http_client.as_ref(),
                &api_url,
                api_key.as_deref(),
                request,
                &extra_headers,
            )
            .await?;
            Ok(stream)
        });

        async move { Ok(future.await?.boxed()) }.boxed()
    }
}
