use anyhow::{Result, anyhow};
use collections::HashMap;
use credentials_provider::CredentialsProvider;
use fs::Fs;
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::BoxStream};
use futures::{Stream, TryFutureExt, stream};
use gpui::{AnyView, App, AsyncApp, Context, CursorStyle, Entity, Task, TaskExt};
use http_client::{CustomHeaders, HttpClient};
use language_model::{
    ApiKeyState, AuthenticateError, DisabledReason, EnvVar, IconOrSvg, LanguageModel,
    LanguageModelCompletionError, LanguageModelCompletionEvent, LanguageModelId, LanguageModelName,
    LanguageModelProvider, LanguageModelProviderId, LanguageModelProviderName,
    LanguageModelProviderState, LanguageModelRequest, LanguageModelRequestTool,
    LanguageModelToolChoice, LanguageModelToolUse, LanguageModelToolUseId, MessageContent,
    RateLimiter, Role, StopReason, TokenUsage, env_var,
};
use menu;
use ollama::{
    ChatMessage, ChatOptions, ChatRequest, ChatResponseDelta, OLLAMA_API_URL, OllamaFunctionCall,
    OllamaFunctionTool, OllamaToolCall, get_models, show_model, stream_chat_completion,
};
pub use settings::OllamaAvailableModel as AvailableModel;
use settings::{Settings, SettingsStore, update_settings_file};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::LazyLock;
use ui::{
    ButtonLike, ButtonLink, ConfiguredApiCard, ElevationIndex, List, ListBulletItem, Tooltip,
    prelude::*,
};
use ui_input::InputField;

use crate::AllLanguageModelSettings;

const OLLAMA_DOWNLOAD_URL: &str = "https://ollama.com/download";
const OLLAMA_LIBRARY_URL: &str = "https://ollama.com/library";
const OLLAMA_SITE: &str = "https://ollama.com/";

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("ollama");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("Ollama");

const API_KEY_ENV_VAR_NAME: &str = "OLLAMA_API_KEY";
static API_KEY_ENV_VAR: LazyLock<EnvVar> = env_var!(API_KEY_ENV_VAR_NAME);

#[derive(Default, Debug, Clone, PartialEq)]
pub struct OllamaSettings {
    pub api_url: String,
    pub auto_discover: bool,
    pub available_models: Vec<AvailableModel>,
    pub context_window: Option<u64>,
    pub custom_headers: CustomHeaders,
}

mod configuration_view;
mod model;
mod provider;
mod state;

#[cfg(test)]
mod tests;

use configuration_view::ConfigurationView;
use model::OllamaLanguageModel;
pub use provider::OllamaLanguageModelProvider;
use state::State;
