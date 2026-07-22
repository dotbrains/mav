use anyhow::Result;
use collections::{HashMap, HashSet};
use credentials_provider::CredentialsProvider;
use fs::Fs;
use futures::Stream;
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::BoxStream};
use gpui::{AnyView, App, AsyncApp, Context, CursorStyle, Entity, Task, TaskExt};
use http_client::{CustomHeaders, HttpClient};
use language_model::util::parse_tool_arguments;
use language_model::{
    ApiKeyState, AuthenticateError, EnvVar, IconOrSvg, LanguageModel, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelId, LanguageModelName, LanguageModelProvider,
    LanguageModelProviderId, LanguageModelProviderName, LanguageModelProviderState,
    LanguageModelRequest, LanguageModelToolChoice, LanguageModelToolResultContent,
    LanguageModelToolUse, MessageContent, RateLimiter, Role, StopReason, TokenUsage, env_var,
};
use llama_cpp::{
    LLAMA_CPP_API_URL, ModelEntry, Props, get_models, get_props, stream_chat_completion,
    stream_model_events,
};
pub use settings::LlamaCppAvailableModel as AvailableModel;
use settings::{Settings, SettingsStore, update_settings_file};
use std::pin::Pin;
use std::sync::LazyLock;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;
use ui::{
    ButtonLike, ButtonLink, ConfiguredApiCard, ElevationIndex, List, ListBulletItem, Tooltip,
    prelude::*,
};
use ui_input::InputField;
use util::ResultExt;

use crate::AllLanguageModelSettings;

const LLAMA_CPP_DOWNLOAD_URL: &str = "https://llama.app";
const LLAMA_CPP_MODELS_URL: &str = "https://huggingface.co/models?library=gguf&sort=trending";

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("llama.cpp");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("llama.cpp");

const API_KEY_ENV_VAR_NAME: &str = "LLAMACPP_API_KEY";
static API_KEY_ENV_VAR: LazyLock<EnvVar> = env_var!(API_KEY_ENV_VAR_NAME);

/// How long to wait before reconnecting to `/models/sse` after the stream ends.
const MODEL_EVENT_RECONNECT_INTERVAL: Duration = Duration::from_secs(5);

/// Context length assumed for an unloaded router model (it can't be probed
/// without loading it). Generous so early messages work; re-discovery refines
/// it once the model loads.
const ASSUMED_UNLOADED_CONTEXT: u64 = 131_072;

mod capabilities;
mod configuration;
mod events;
mod model;
mod model_trait;
mod provider;
mod request;
mod settings_merge;
mod state;
#[cfg(test)]
mod tests;

use self::{capabilities::*, configuration::*, events::*, model::*, request::*, settings_merge::*};

#[derive(Default, Debug, Clone, PartialEq)]
pub struct LlamaCppSettings {
    pub api_url: String,
    pub auto_discover: bool,
    pub available_models: Vec<AvailableModel>,
    pub context_window: Option<u64>,
    pub custom_headers: CustomHeaders,
}

pub struct LlamaCppLanguageModelProvider {
    http_client: Arc<dyn HttpClient>,
    state: Entity<State>,
    /// Live capabilities shared with the agent's models (see [`LiveCapabilities`]).
    capability_cells: CapabilityCells,
    /// Live model-load progress shared with the models (see [`LoadingProgress`]).
    loading_progress: LoadingProgress,
}

pub struct State {
    api_key_state: ApiKeyState,
    credentials_provider: Arc<dyn CredentialsProvider>,
    http_client: Arc<dyn HttpClient>,
    fetched_models: Vec<llama_cpp::Model>,
    fetch_model_task: Option<Task<Result<()>>>,
    /// Router-mode task on `/models/sse`; re-runs discovery as models load/unload.
    model_event_task: Option<Task<()>>,
    /// Same `Arc` as the provider's; re-discovery keeps these cells in sync.
    capability_cells: CapabilityCells,
    /// Same `Arc` as the provider's; the event stream updates it as a model loads.
    loading_progress: LoadingProgress,
}
