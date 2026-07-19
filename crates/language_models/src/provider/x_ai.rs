use anyhow::Result;
use collections::BTreeMap;
use credentials_provider::CredentialsProvider;
use futures::{FutureExt, StreamExt, future::BoxFuture};
use gpui::{AnyView, App, AsyncApp, Context, Entity, Task, Window};
use http_client::{CustomHeaders, HttpClient};
use language_model::{
    ApiKeyState, AuthenticateError, EnvVar, IconOrSvg, LanguageModel, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelEffortLevel, LanguageModelId, LanguageModelName,
    LanguageModelProvider, LanguageModelProviderId, LanguageModelProviderName,
    LanguageModelProviderState, LanguageModelRequest, LanguageModelToolChoice,
    LanguageModelToolSchemaFormat, ProviderConfigurationView, RateLimiter, env_var,
};
use open_ai::ResponseStreamEvent;
pub use settings::XaiAvailableModel as AvailableModel;
use settings::{Settings, SettingsStore};
use std::sync::{Arc, LazyLock};
use strum::IntoEnumIterator;
use ui::prelude::*;
use x_ai::XAI_API_URL;

use configuration_view::ConfigurationView;

#[path = "x_ai/configuration_view.rs"]
mod configuration_view;

#[cfg(test)]
#[path = "x_ai/tests.rs"]
mod tests;

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("x_ai");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("xAI");

const API_KEY_ENV_VAR_NAME: &str = "XAI_API_KEY";
static API_KEY_ENV_VAR: LazyLock<EnvVar> = env_var!(API_KEY_ENV_VAR_NAME);

#[derive(Default, Clone, Debug, PartialEq)]
pub struct XAiSettings {
    pub api_url: String,
    pub available_models: Vec<AvailableModel>,
    pub custom_headers: CustomHeaders,
}

pub struct XAiLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    state: Entity<State>,
}

pub struct State {
    api_key_state: ApiKeyState,
    credentials_provider: Arc<dyn CredentialsProvider>,
}

impl State {
    fn is_authenticated(&self) -> bool {
        self.api_key_state.has_key()
    }

    fn set_api_key(&mut self, api_key: Option<String>, cx: &mut Context<Self>) -> Task<Result<()>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = XAiLanguageModelProvider::api_url(cx);
        self.api_key_state.store(
            api_url,
            api_key,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        )
    }

    fn authenticate(&mut self, cx: &mut Context<Self>) -> Task<Result<(), AuthenticateError>> {
        let credentials_provider = self.credentials_provider.clone();
        let api_url = XAiLanguageModelProvider::api_url(cx);
        self.api_key_state.load_if_needed(
            api_url,
            |this| &mut this.api_key_state,
            credentials_provider,
            cx,
        )
    }
}

impl XAiLanguageModelProvider {
    pub fn new(
        http_client: Arc<dyn HttpClient>,
        credentials_provider: Arc<dyn CredentialsProvider>,
        cx: &mut App,
    ) -> Self {
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

        Self { http_client, state }
    }

    fn create_language_model(&self, model: x_ai::Model) -> Arc<dyn LanguageModel> {
        Arc::new(XAiLanguageModel {
            id: LanguageModelId::from(model.id().to_string()),
            model,
            state: self.state.clone(),
            http_client: self.http_client.clone(),
            request_limiter: RateLimiter::new(4),
        })
    }

    fn settings(cx: &App) -> &XAiSettings {
        &crate::AllLanguageModelSettings::get_global(cx).x_ai
    }

    fn api_url(cx: &App) -> SharedString {
        let api_url = &Self::settings(cx).api_url;
        if api_url.is_empty() {
            XAI_API_URL.into()
        } else {
            SharedString::new(api_url.as_str())
        }
    }
}

impl LanguageModelProviderState for XAiLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for XAiLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::AiXAi)
    }

    fn default_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_language_model(x_ai::Model::default()))
    }

    fn default_fast_model(&self, _cx: &App) -> Option<Arc<dyn LanguageModel>> {
        Some(self.create_language_model(x_ai::Model::default_fast()))
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        let mut models = BTreeMap::default();

        for model in x_ai::Model::iter() {
            if !matches!(model, x_ai::Model::Custom { .. }) {
                models.insert(model.id().to_string(), model);
            }
        }

        for model in &Self::settings(cx).available_models {
            models.insert(
                model.name.clone(),
                x_ai::Model::Custom {
                    name: model.name.clone(),
                    display_name: model.display_name.clone(),
                    max_tokens: model.max_tokens,
                    max_output_tokens: model.max_output_tokens,
                    max_completion_tokens: model.max_completion_tokens,
                    supports_images: model.supports_images,
                    supports_tools: model.supports_tools,
                    parallel_tool_calls: model.parallel_tool_calls,
                },
            );
        }

        models
            .into_values()
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
                    "https://console.x.ai/team/default/api-keys",
                    "xai-...",
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

pub struct XAiLanguageModel {
    id: LanguageModelId,
    model: x_ai::Model,
    state: Entity<State>,
    http_client: Arc<dyn HttpClient>,
    request_limiter: RateLimiter,
}

impl XAiLanguageModel {
    fn stream_completion(
        &self,
        request: open_ai::Request,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            futures::stream::BoxStream<'static, Result<ResponseStreamEvent>>,
            LanguageModelCompletionError,
        >,
    > {
        let http_client = self.http_client.clone();

        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, cx| {
            let api_url = XAiLanguageModelProvider::api_url(cx);
            let extra_headers = XAiLanguageModelProvider::settings(cx)
                .custom_headers
                .clone();
            (state.api_key_state.key(&api_url), api_url, extra_headers)
        });

        let future = self.request_limiter.stream(async move {
            let provider = PROVIDER_NAME;
            let Some(api_key) = api_key else {
                return Err(LanguageModelCompletionError::NoApiKey { provider });
            };
            let request = open_ai::stream_completion(
                http_client.as_ref(),
                provider.0.as_str(),
                &api_url,
                &api_key,
                request,
                &extra_headers,
            );
            let response = request.await?;
            Ok(response)
        });

        async move { Ok(future.await?.boxed()) }.boxed()
    }
}

fn x_ai_reasoning_efforts(model: &x_ai::Model) -> &'static [open_ai::ReasoningEffort] {
    if model.supports_reasoning_effort() {
        &[
            open_ai::ReasoningEffort::None,
            open_ai::ReasoningEffort::Low,
            open_ai::ReasoningEffort::Medium,
            open_ai::ReasoningEffort::High,
        ]
    } else {
        &[]
    }
}

fn default_thinking_reasoning_effort(model: &x_ai::Model) -> Option<open_ai::ReasoningEffort> {
    if model.supports_reasoning_effort() {
        Some(open_ai::ReasoningEffort::Low)
    } else {
        None
    }
}

fn reasoning_effort_for_request(
    request: &LanguageModelRequest,
    model: &x_ai::Model,
) -> Option<open_ai::ReasoningEffort> {
    let supported_efforts = x_ai_reasoning_efforts(model);
    if supported_efforts.is_empty() {
        return None;
    }

    if request.thinking_allowed {
        request
            .thinking_effort
            .as_deref()
            .and_then(|effort| effort.parse::<open_ai::ReasoningEffort>().ok())
            .filter(|effort| supported_efforts.contains(effort))
            .filter(|effort| *effort != open_ai::ReasoningEffort::None)
            .or_else(|| default_thinking_reasoning_effort(model))
    } else if supported_efforts.contains(&open_ai::ReasoningEffort::None) {
        Some(open_ai::ReasoningEffort::None)
    } else {
        None
    }
}

fn supported_thinking_effort_levels(model: &x_ai::Model) -> Vec<LanguageModelEffortLevel> {
    let default_effort = default_thinking_reasoning_effort(model);
    x_ai_reasoning_efforts(model)
        .iter()
        .copied()
        .filter_map(|effort| {
            let (name, value) = match effort {
                open_ai::ReasoningEffort::None => return None,
                open_ai::ReasoningEffort::Minimal => ("Minimal", "minimal"),
                open_ai::ReasoningEffort::Low => ("Low", "low"),
                open_ai::ReasoningEffort::Medium => ("Medium", "medium"),
                open_ai::ReasoningEffort::High => ("High", "high"),
                open_ai::ReasoningEffort::XHigh => ("Extra High", "xhigh"),
                open_ai::ReasoningEffort::Max => return None, // Not supported by any xAI models
            };

            Some(LanguageModelEffortLevel {
                name: name.into(),
                value: value.into(),
                is_default: Some(effort) == default_effort,
            })
        })
        .collect()
}

impl LanguageModel for XAiLanguageModel {
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
        self.model.supports_tool()
    }

    fn supports_images(&self) -> bool {
        self.model.supports_images()
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
        self.model.supports_reasoning_effort()
    }

    fn supported_effort_levels(&self) -> Vec<LanguageModelEffortLevel> {
        supported_thinking_effort_levels(&self.model)
    }

    fn tool_input_format(&self) -> LanguageModelToolSchemaFormat {
        if self.model.requires_json_schema_subset() {
            LanguageModelToolSchemaFormat::JsonSchemaSubset
        } else {
            LanguageModelToolSchemaFormat::JsonSchema
        }
    }

    fn telemetry_id(&self) -> String {
        format!("x_ai/{}", self.model.id())
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_token_count()
    }

    fn max_output_tokens(&self) -> Option<u64> {
        self.model.max_output_tokens()
    }

    fn supports_split_token_display(&self) -> bool {
        true
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
        let reasoning_effort = reasoning_effort_for_request(&request, &self.model);
        let request = crate::provider::open_ai::into_open_ai(
            request,
            self.model.id(),
            self.model.supports_parallel_tool_calls(),
            self.model.supports_prompt_cache_key(),
            self.max_output_tokens(),
            crate::provider::open_ai::ChatCompletionMaxTokensParameter::MaxCompletionTokens,
            reasoning_effort,
            false,
        );
        let completions = self.stream_completion(request, cx);
        async move {
            let mapper = crate::provider::open_ai::OpenAiEventMapper::new();
            Ok(mapper.map_stream(completions.await?).boxed())
        }
        .boxed()
    }
}
