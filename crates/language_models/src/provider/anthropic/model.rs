use super::{
    AnthropicEventMapper, AnthropicLanguageModelProvider, AnthropicPromptCacheMode, PROVIDER_ID,
    PROVIDER_NAME, State, into_anthropic,
};
use anthropic::AnthropicError;
use anyhow::Result;
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::BoxStream};
use gpui::{App, AsyncApp, Entity};
use http_client::HttpClient;
use language_model::{
    LanguageModel, LanguageModelCompletionError, LanguageModelCompletionEvent, LanguageModelId,
    LanguageModelName, LanguageModelProviderId, LanguageModelProviderName, LanguageModelRequest,
    LanguageModelToolChoice, RateLimiter,
};
use std::sync::Arc;

pub struct AnthropicModel {
    id: LanguageModelId,
    model: anthropic::Model,
    state: Entity<State>,
    http_client: Arc<dyn HttpClient>,
    request_limiter: RateLimiter,
}

impl AnthropicModel {
    pub(super) fn new(
        model: anthropic::Model,
        state: Entity<State>,
        http_client: Arc<dyn HttpClient>,
    ) -> Self {
        Self {
            id: LanguageModelId::from(model.id.to_string()),
            model,
            state,
            http_client,
            request_limiter: RateLimiter::new(4),
        }
    }

    fn stream_completion(
        &self,
        request: anthropic::Request,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            BoxStream<'static, Result<anthropic::Event, AnthropicError>>,
            LanguageModelCompletionError,
        >,
    > {
        let http_client = self.http_client.clone();

        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, cx| {
            let api_url = AnthropicLanguageModelProvider::api_url(cx);
            let extra_headers = AnthropicLanguageModelProvider::settings(cx)
                .custom_headers
                .clone();
            (state.api_key_state.key(&api_url), api_url, extra_headers)
        });

        let beta_headers = self.model.beta_headers();

        async move {
            let Some(api_key) = api_key else {
                return Err(LanguageModelCompletionError::NoApiKey {
                    provider: PROVIDER_NAME,
                });
            };
            let request = anthropic::stream_completion(
                http_client.as_ref(),
                &api_url,
                &api_key,
                request,
                beta_headers,
                &extra_headers,
            );
            request.await.map_err(Into::into)
        }
        .boxed()
    }
}

impl LanguageModel for AnthropicModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
    }

    fn name(&self) -> LanguageModelName {
        LanguageModelName::from(self.model.display_name.clone())
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn supports_images(&self) -> bool {
        self.model.supports_images
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        match choice {
            LanguageModelToolChoice::Auto
            | LanguageModelToolChoice::Any
            | LanguageModelToolChoice::None => true,
        }
    }

    fn supports_thinking(&self) -> bool {
        self.model.supports_thinking
    }

    fn supports_fast_mode(&self) -> bool {
        self.model.supports_speed
    }

    fn refusal_fallback_model_id(&self) -> Option<&'static str> {
        if self.model.id.starts_with(anthropic::FABLE_MODEL_ID_PREFIX) {
            Some(anthropic::FABLE_FALLBACK_MODEL_ID)
        } else {
            None
        }
    }

    fn supports_server_side_compaction(&self) -> bool {
        self.model.supports_compaction
    }

    fn supported_effort_levels(&self) -> Vec<language_model::LanguageModelEffortLevel> {
        self.model
            .supported_effort_levels
            .iter()
            .map(|e| {
                let is_default = matches!(e, anthropic::Effort::High);
                let (name, value) = match e {
                    anthropic::Effort::Low => ("Low".into(), "low".into()),
                    anthropic::Effort::Medium => ("Medium".into(), "medium".into()),
                    anthropic::Effort::High => ("High".into(), "high".into()),
                    anthropic::Effort::XHigh => ("XHigh".into(), "xhigh".into()),
                    anthropic::Effort::Max => ("Max".into(), "max".into()),
                };
                language_model::LanguageModelEffortLevel {
                    name,
                    value,
                    is_default,
                }
            })
            .collect::<Vec<_>>()
    }

    fn telemetry_id(&self) -> String {
        format!("anthropic/{}", self.model.id)
    }

    fn api_key(&self, cx: &App) -> Option<String> {
        self.state.read_with(cx, |state, cx| {
            let api_url = AnthropicLanguageModelProvider::api_url(cx);
            state.api_key_state.key(&api_url).map(|key| key.to_string())
        })
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_input_tokens
    }

    fn max_output_tokens(&self) -> Option<u64> {
        Some(self.model.max_output_tokens)
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
        let has_tools = !request.tools.is_empty();
        let request_id = self.model.request_id(has_tools).to_string();
        let mut request = into_anthropic(
            request,
            request_id,
            self.model.default_temperature,
            self.model.max_output_tokens,
            self.model.mode.clone(),
            AnthropicPromptCacheMode::Automatic,
        );
        if !self.model.supports_speed {
            request.speed = None;
        }
        let request = self.stream_completion(request, cx);
        let future = self.request_limiter.stream(async move {
            let response = request.await?;
            Ok(AnthropicEventMapper::new(PROVIDER_NAME).map_stream(response))
        });
        async move { Ok(future.await?.boxed()) }.boxed()
    }
}
