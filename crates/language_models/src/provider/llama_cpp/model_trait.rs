use super::*;

impl LanguageModel for LlamaCppLanguageModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelName {
        match self.loading_label() {
            // Surface load progress in the display name so it shows wherever the
            // model is named, without provider-agnostic UI changes. The agent
            // rebuilds the name on `ProviderStateChanged`, which our ticks emit.
            Some(label) => LanguageModelName::from(format!("{} · {}", self.display_name, label)),
            None => LanguageModelName::from(self.display_name.clone()),
        }
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn supports_tools(&self) -> bool {
        self.capabilities().supports_tools
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        self.supports_tools()
            && match choice {
                LanguageModelToolChoice::Auto => true,
                LanguageModelToolChoice::Any => true,
                LanguageModelToolChoice::None => true,
            }
    }

    fn supports_images(&self) -> bool {
        self.supports_images
    }

    fn supports_thinking(&self) -> bool {
        self.capabilities().supports_thinking
    }

    fn telemetry_id(&self) -> String {
        telemetry_id_for(&self.name)
    }

    fn max_token_count(&self) -> u64 {
        self.capabilities().max_tokens
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
        let request = self.to_llama_cpp_request(request);
        let completions = self.stream_llama_cpp_completion(request, cx);
        async move {
            let mapper = LlamaCppEventMapper::new();
            Ok(mapper.map_stream(completions.await?).boxed())
        }
        .boxed()
    }
}
