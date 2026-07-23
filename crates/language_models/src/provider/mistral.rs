use anyhow::{Result, anyhow};
use collections::{BTreeMap, HashMap};
use credentials_provider::CredentialsProvider;

use futures::{FutureExt, Stream, StreamExt, future::BoxFuture, stream::BoxStream};
use gpui::{AnyView, App, AsyncApp, Context, Entity, Global, SharedString, Task, TaskExt, Window};
use http_client::{CustomHeaders, HttpClient};
use language_model::{
    ApiKeyState, AuthenticateError, EnvVar, IconOrSvg, LanguageModel, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelId, LanguageModelName, LanguageModelProvider,
    LanguageModelProviderId, LanguageModelProviderName, LanguageModelProviderState,
    LanguageModelRequest, LanguageModelToolChoice, LanguageModelToolResultContent,
    LanguageModelToolUse, MessageContent, ProviderConfigurationView, RateLimiter, Role, StopReason,
    TokenUsage, env_var,
};
pub use mistral::{MISTRAL_API_URL, StreamResponse};
pub use settings::MistralAvailableModel as AvailableModel;
use settings::{Settings, SettingsStore};
use std::pin::Pin;
use std::sync::{Arc, LazyLock};
use strum::IntoEnumIterator;
use ui::{ButtonLink, ConfiguredApiCard, List, ListBulletItem, prelude::*};
use ui_input::InputField;
use util::ResultExt;

use language_model::util::{fix_streamed_json, parse_tool_arguments};

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("mistral");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("Mistral");

const API_KEY_ENV_VAR_NAME: &str = "MISTRAL_API_KEY";
static API_KEY_ENV_VAR: LazyLock<EnvVar> = env_var!(API_KEY_ENV_VAR_NAME);
pub(crate) const RESERVED_HEADER_NAMES: &[&str] = &["x-affinity"];

#[derive(Default, Clone, Debug, PartialEq)]
pub struct MistralSettings {
    pub api_url: String,
    pub available_models: Vec<AvailableModel>,
    pub custom_headers: CustomHeaders,
}

mod configuration_view;
mod event_mapper;
mod model;
mod provider;
mod request;
mod state;

#[cfg(test)]
mod tests;

use configuration_view::ConfigurationView;
use event_mapper::MistralEventMapper;
use model::MistralLanguageModel;
pub use provider::MistralLanguageModelProvider;
pub use request::into_mistral;
use state::State;
