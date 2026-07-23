use anyhow::Result;
use collections::HashMap;
use credentials_provider::CredentialsProvider;
use futures::{FutureExt, Stream, StreamExt, future::BoxFuture};
use gpui::{AnyView, App, AsyncApp, Context, Entity, SharedString, Task, TaskExt};
use http_client::{CustomHeaders, HttpClient};
use language_model::{
    ApiKeyState, AuthenticateError, EnvVar, IconOrSvg, LanguageModel, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelId, LanguageModelName, LanguageModelProvider,
    LanguageModelProviderId, LanguageModelProviderName, LanguageModelProviderState,
    LanguageModelRequest, LanguageModelToolChoice, LanguageModelToolResultContent,
    LanguageModelToolSchemaFormat, LanguageModelToolUse, MessageContent, ProviderConfigurationView,
    RateLimiter, Role, StopReason, TokenUsage, env_var,
};
use open_router::{
    Model, ModelMode as OpenRouterModelMode, OPEN_ROUTER_API_URL, ResponseStreamEvent, list_models,
};
use settings::{OpenRouterAvailableModel as AvailableModel, Settings, SettingsStore};
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use ui::{ButtonLink, ConfiguredApiCard, List, ListBulletItem, prelude::*};
use ui_input::InputField;
use util::ResultExt;

use language_model::util::{fix_streamed_json, parse_tool_arguments};

mod configuration_view;
use configuration_view::ConfigurationView;

mod model;
use model::OpenRouterLanguageModel;

mod provider;

mod request;
use request::into_open_router;

mod stream_mapper;
use stream_mapper::OpenRouterEventMapper;

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("openrouter");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("OpenRouter");

const API_KEY_ENV_VAR_NAME: &str = "OPENROUTER_API_KEY";
static API_KEY_ENV_VAR: LazyLock<EnvVar> = env_var!(API_KEY_ENV_VAR_NAME);
pub(crate) const RESERVED_HEADER_NAMES: &[&str] = &["HTTP-Referer", "X-Title"];
const MAX_OPEN_ROUTER_SESSION_ID_LENGTH: usize = 256;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct OpenRouterSettings {
    pub api_url: String,
    pub available_models: Vec<AvailableModel>,
    pub custom_headers: CustomHeaders,
}

pub struct OpenRouterLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    state: Entity<State>,
}

pub struct State {
    api_key_state: ApiKeyState,
    credentials_provider: Arc<dyn CredentialsProvider>,
    http_client: Arc<dyn HttpClient>,
    available_models: Vec<open_router::Model>,
    fetch_models_task: Option<Task<Result<(), LanguageModelCompletionError>>>,
}

#[cfg(test)]
mod tests;
