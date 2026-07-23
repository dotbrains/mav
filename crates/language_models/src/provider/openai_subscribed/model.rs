use super::*;

//
// The ChatGPT Subscription provider routes requests to chatgpt.com/backend-api/codex,
// which only supports a subset of OpenAI models. This list is maintained separately
// from the standard OpenAI API model list (open_ai::Model).
//
// TODO: The Codex CLI fetches this list dynamically from
// `GET <codex_base_url>/models?client_version=...` (see
// codex-rs/codex-api/src/endpoint/models.rs in openai/codex) and falls back to
// a bundled models.json. Beyond going stale, the static approach also can't
// model per-account access (e.g. free accounts cannot use gpt-5.4 even though
// paid accounts can), so the backend still rejects some requests. The bundled
// list at
// codex-rs/models-manager/models.json (openai/codex) is the closest
// approximation; the entries below mirror that file's picker-visible models.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum ChatGptModel {
    Gpt55,
    Gpt54,
    Gpt54Mini,
}

impl ChatGptModel {
    pub(super) fn all() -> Vec<Self> {
        vec![Self::Gpt55, Self::Gpt54, Self::Gpt54Mini]
    }

    pub(super) fn id(&self) -> &str {
        match self {
            Self::Gpt55 => "gpt-5.5",
            Self::Gpt54 => "gpt-5.4",
            Self::Gpt54Mini => "gpt-5.4-mini",
        }
    }

    fn display_name(&self) -> &str {
        match self {
            Self::Gpt55 => "GPT-5.5",
            Self::Gpt54 => "GPT-5.4",
            Self::Gpt54Mini => "GPT-5.4 Mini",
        }
    }

    fn max_token_count(&self) -> u64 {
        // All Codex-supported models use a 272K context window in the Codex
        // backend, even when the raw model exposes a larger context window via the
        // public API (e.g. gpt-5.4 has max_context_window 1M, but Codex uses
        // context_window 272K). Source: openai/codex models-manager/models.json.
        272_000
    }

    fn max_output_tokens(&self) -> Option<u64> {
        // Codex model metadata does not expose a max output token cap for these
        // models. Source: openai/codex models-manager/models.json.
        None
    }

    fn supports_images(&self) -> bool {
        true
    }

    fn default_reasoning_effort(&self) -> Option<ReasoningEffort> {
        // Codex bundled models all default to Medium reasoning effort.
        Some(ReasoningEffort::Medium)
    }

    fn supported_reasoning_efforts(&self) -> &'static [ReasoningEffort] {
        // The Codex backend's supported_reasoning_levels for every model in this list is low/medium/high/xhigh
        &[
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::XHigh,
        ]
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        true
    }

    fn supports_prompt_cache_key(&self) -> bool {
        true
    }

    fn supports_priority(&self) -> bool {
        match self {
            Self::Gpt55 | Self::Gpt54 => true,
            Self::Gpt54Mini => false,
        }
    }
}

pub(super) struct OpenAiSubscribedLanguageModel {
    pub(super) id: LanguageModelId,
    pub(super) model: ChatGptModel,
    pub(super) state: Entity<State>,
    pub(super) http_client: Arc<dyn HttpClient>,
    pub(super) request_limiter: RateLimiter,
}

impl LanguageModel for OpenAiSubscribedLanguageModel {
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
        true
    }

    fn supports_images(&self) -> bool {
        self.model.supports_images()
    }

    fn supports_tool_choice(&self, _choice: LanguageModelToolChoice) -> bool {
        true
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn supports_thinking(&self) -> bool {
        true
    }

    fn supports_fast_mode(&self) -> bool {
        self.model.supports_priority()
    }

    fn supported_effort_levels(&self) -> Vec<LanguageModelEffortLevel> {
        let default_effort = self.model.default_reasoning_effort();
        self.model
            .supported_reasoning_efforts()
            .iter()
            .copied()
            .filter_map(|effort| {
                let (name, value) = match effort {
                    ReasoningEffort::None => return None,
                    ReasoningEffort::Minimal => ("Minimal", "minimal"),
                    ReasoningEffort::Low => ("Low", "low"),
                    ReasoningEffort::Medium => ("Medium", "medium"),
                    ReasoningEffort::High => ("High", "high"),
                    ReasoningEffort::XHigh => ("Extra High", "xhigh"),
                    ReasoningEffort::Max => return None, // Not supported by any OpenAI models
                };

                Some(LanguageModelEffortLevel {
                    name: name.into(),
                    value: value.into(),
                    is_default: Some(effort) == default_effort,
                })
            })
            .collect()
    }

    fn telemetry_id(&self) -> String {
        format!("openai-subscribed/{}", self.model.id())
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_token_count()
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.model.max_output_tokens()
    }

    fn stream_completion(
        &self,
        mut request: LanguageModelRequest,
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
        if !self.model.supports_priority() {
            request.speed = None;
        }

        // The Codex backend rejects `max_output_tokens` (`Unsupported parameter`),
        // unlike the public OpenAI Responses API. Pass `None` so the field is
        // omitted from the serialized request body entirely.
        let mut responses_request = into_open_ai_response(
            request,
            self.model.id(),
            self.model.supports_parallel_tool_calls(),
            self.model.supports_prompt_cache_key(),
            /*max_output_tokens*/ None,
            self.model.default_reasoning_effort(),
            self.model
                .supported_reasoning_efforts()
                .contains(&ReasoningEffort::None),
        );
        responses_request.store = Some(false);

        // The Codex backend requires system messages to be in the top-level
        // `instructions` field rather than as input items.
        let mut instructions = Vec::new();
        responses_request.input.retain(|item| {
            if let open_ai::responses::ResponseInputItem::Message(msg) = item {
                if msg.role == open_ai::Role::System {
                    for part in &msg.content {
                        if let open_ai::responses::ResponseInputContent::Text { text } = part {
                            instructions.push(text.clone());
                        }
                    }
                    return false;
                }
            }
            true
        });
        responses_request.instructions = Some(instructions.join("\n\n"));

        let state = self.state.downgrade();
        let http_client = self.http_client.clone();
        let request_limiter = self.request_limiter.clone();

        let future = cx.spawn(async move |cx| {
            let creds = get_fresh_credentials(&state, &http_client, cx).await?;

            let mut header_pairs: Vec<(HeaderName, HeaderValue)> = vec![
                (
                    HeaderName::from_static("originator"),
                    HeaderValue::from_static("mav"),
                ),
                (
                    HeaderName::from_static("openai-beta"),
                    HeaderValue::from_static("responses=experimental"),
                ),
            ];
            if let Some(ref id) = creds.account_id {
                if !id.is_empty() {
                    if let Ok(value) = HeaderValue::from_str(id) {
                        header_pairs.push((HeaderName::from_static("chatgpt-account-id"), value));
                    }
                }
            }
            let extra_headers = CustomHeaders::new(header_pairs);

            let access_token = creds.access_token.clone();
            request_limiter
                .stream(async move {
                    stream_response(
                        http_client.as_ref(),
                        PROVIDER_NAME.0.as_str(),
                        CODEX_BASE_URL,
                        &access_token,
                        responses_request,
                        &extra_headers,
                    )
                    .await
                    .map_err(LanguageModelCompletionError::from)
                })
                .await
        });

        async move {
            let mapper = OpenAiResponseEventMapper::new();
            Ok(mapper.map_stream(future.await?.boxed()).boxed())
        }
        .boxed()
    }
}
